//! GPUI shell: command palette, menus, theme, and the docking surface.

use std::time::Duration;

use gpui::{
    actions, div, hsla, img, point, px, prelude::*, rgb, size, transparent_black, AnyElement, App, Application,
    Bounds, ClickEvent, Context, CursorStyle, Decorations, FocusHandle, KeyBinding, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, ObjectFit, Render, ScrollHandle, SharedString, Timer,
    TitlebarOptions, UniformListScrollHandle, Window, WindowBounds, WindowControlArea, WindowDecorations,
    WindowOptions,
};
use idmc_backend::Backend;
use idmc_layout::Dir;

use crate::edit::render_edit_line;
use crate::session::{launch_from_args, Effect, Launch, MenuCommand, PaletteMode, Session};
use crate::theme::{theme, Theme};

actions!(
    idmc,
    [
        ShowPalette,
        ToggleTheme,
        SaveWorkspace,
        ReloadWorkspace,
        ResetWorkspace,
        ReloadData,
        Quit,
        CloseView,
        ZoomView,
        FocusNext,
        FocusPrev,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        NewTable,
        NewDetail,
        NewTree,
        FieldChooser,
        ChooseLink,
        ClearLink,
        SelectNext,
        SelectPrev,
    ]
);

#[derive(Clone)]
pub struct DragView {
    pub id: String,
    pub title: String,
}

#[derive(Clone)]
pub struct DragDivider {
    pub path: Vec<usize>,
    pub idx: usize,
    pub dir: Dir,
}

pub struct Empty;

impl Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

pub struct Ghost {
    pub title: String,
}

impl Render for Ghost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgb(0x143d2c))
            .text_color(rgb(0xe7e9ee))
            .text_size(px(12.))
            .child(self.title.clone())
    }
}

#[derive(Clone)]
pub struct DragColumn {
    pub view: String,
    pub field: String,
    pub label: String,
}

pub struct Root {
    pub session: Session,
    pub focus: FocusHandle,
    pub dirty: bool,
    pub(crate) graph_pane: std::cell::RefCell<Option<gpui::Entity<crate::graph::GraphPane>>>,
    pub(crate) h_scroll: std::cell::RefCell<std::collections::HashMap<String, ScrollHandle>>,
    pub(crate) v_scroll: std::cell::RefCell<std::collections::HashMap<String, UniformListScrollHandle>>,
}

impl Root {
    fn new(session: Session, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(150)).await;
                if this
                    .update(cx, |root, cx| {
                        let loaded = root.session.backend.poll();
                        let extra = root.session.absorb_async();
                        if loaded {
                            root.session.reconcile();
                        }
                        if extra {
                            if let Some(pane) = root.graph_pane.borrow().clone() {
                                pane.update(cx, |_, cx| cx.notify());
                            }
                        }
                        if loaded || extra {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            session,
            focus: cx.focus_handle(),
            dirty: false,
            graph_pane: std::cell::RefCell::new(None),
            h_scroll: std::cell::RefCell::new(std::collections::HashMap::new()),
            v_scroll: std::cell::RefCell::new(std::collections::HashMap::new()),
        }
    }

    pub fn exec(&mut self, id: &str, cx: &mut Context<Self>) {
        let effect = self.session.perform(id);
        if matches!(
            id,
            "theme.toggle"
                | "workspace.save"
                | "workspace.reset"
                | "workspace.new"
                | "workspace.duplicate"
                | "workspace.rename"
                | "workspace.close"
                | "view.close"
                | "view.zoom"
                | "view.move_left"
                | "view.move_right"
                | "view.move_up"
                | "view.move_down"
                | "view.fields"
                | "view.unlink"
        ) {
            if let Ok(message) = self.session.save() {
                if id == "workspace.save" {
                    self.session.status = message;
                }
            }
            self.dirty = false;
        }
        if effect == Effect::Quit {
            cx.quit();
        }
        cx.notify();
    }

    pub fn open_add_menu(&mut self, target: &str, pos: gpui::Point<gpui::Pixels>) {
        self.session.file.focus = Some(target.to_string());
        self.session.pending_dock = Some(target.to_string());
        self.session.menu = Some(crate::session::MenuState {
            x: f32::from(pos.x),
            y: f32::from(pos.y),
            grow_left: false,
            items: vec![
                item("Open view", "", MenuCommand::Run("view.open")),
                item("New table view", "", MenuCommand::Run("view.new_table")),
                item("New detail view", "", MenuCommand::Run("view.new_detail")),
                item("New tree view", "", MenuCommand::Run("view.new_tree")),
                item("Open Graph view", "", MenuCommand::Run("view.open_graph")),
                item("Show in graph", "", MenuCommand::Run("record.show_in_graph")),
            ],
        });
    }

    pub fn open_view_menu(&mut self, view: &str, pos: gpui::Point<gpui::Pixels>) {
        self.session.file.focus = Some(view.to_string());
        let table = self.session.spec(view).is_some_and(|spec| spec.kind == idmc_model::ViewKind::Table);
        let mut items = Vec::new();
        if table {
            items.push(item("Field chooser", "Ctrl+Alt+C", MenuCommand::Run("view.fields")));
        }
        items.push(item("New record", "", MenuCommand::Run("record.new")));
        items.push(item("New child row", "", MenuCommand::Run("record.new_child")));
        items.push(item("Save changes", "", MenuCommand::Run("record.save")));
        items.push(item("Revert changes", "", MenuCommand::Run("record.revert")));
        items.push(item("Delete record", "", MenuCommand::Run("record.delete")));
        items.push(item("Show in graph", "", MenuCommand::Run("record.show_in_graph")));
        items.push(item("Choose source link", "Ctrl+Alt+L", MenuCommand::Run("view.link")));
        items.push(item("Clear source link", "", MenuCommand::Run("view.unlink")));
        items.push(item("Maximize / restore", "Ctrl+Alt+Z", MenuCommand::Run("view.zoom")));
        items.push(item("Close view", "Ctrl+W", MenuCommand::Run("view.close")));
        self.session.menu = Some(crate::session::MenuState {
            x: f32::from(pos.x),
            y: f32::from(pos.y),
            grow_left: true,
            items,
        });
    }

    pub fn open_row_menu(&mut self, view: &str, record: &str, entity: &str, pos: gpui::Point<gpui::Pixels>) {
        self.session.select(view, record);
        let mut items = Vec::new();
        for target in self.session.row_targets(entity) {
            items.push(item(
                &format!("Open view · {}", target.label),
                "",
                MenuCommand::OpenLinked {
                    source: view.to_string(),
                    record: record.to_string(),
                    kind: target.kind,
                    entity: target.entity,
                    relationship: target.relationship,
                },
            ));
        }
        items.push(item(
            "Open detail",
            "",
            MenuCommand::OpenLinked {
                source: view.to_string(),
                record: record.to_string(),
                kind: idmc_model::ViewKind::Detail,
                entity: entity.to_string(),
                relationship: "identity".to_string(),
            },
        ));
        items.push(item("Show in graph", "", MenuCommand::Run("record.show_in_graph")));
        self.session.menu = Some(crate::session::MenuState {
            x: f32::from(pos.x),
            y: f32::from(pos.y),
            grow_left: false,
            items,
        });
    }

    fn on_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.to_ascii_lowercase();
        let mods = &event.keystroke.modifiers;
        let editing = self.session.palette.open || self.session.chooser_for.is_some() || self.session.filter_for.is_some() || self.session.graph_query;
        if self.session.editing.is_some() && !editing {
            self.on_cell_key(&key, mods, event, cx);
            return;
        }
        if !editing && self.session.graph_focused() {
            let plain = !mods.control && !mods.alt && !mods.platform && !mods.shift;
            let effect = match key.as_str() {
                "escape" => {
                    self.session.graph.as_mut().map(|board| board.clear_selection());
                    None
                }
                "enter" if plain => Some(self.session.graph_enter()),
                "left" | "right" if plain => {
                    self.session.graph.as_mut().map(|board| board.arrow(key.as_str()));
                    None
                }
                "f" if plain => {
                    if let Some(board) = self.session.graph.as_mut() {
                        board.user_camera = false;
                        let (w, h) = (board.width, board.height);
                        board.fit(w, h);
                    }
                    None
                }
                _ => None,
            };
            if let Some(effect) = effect {
                self.apply_graph_effect(effect);
            }
            if matches!(key.as_str(), "escape" | "enter" | "left" | "right" | "f") && plain || key == "escape" {
                cx.notify();
            }
            return;
        }
        if !editing && self.session.editing.is_none() && self.session.focused_table().is_some() {
            let plain = !mods.control && !mods.alt && !mods.platform && !mods.shift;
            let command = (mods.control || mods.platform) && !mods.alt && !mods.shift;
            let handled = match key.as_str() {
                "left" if plain => {
                    self.session.move_cell(-1, 0);
                    true
                }
                "right" if plain => {
                    self.session.move_cell(1, 0);
                    true
                }
                "home" if command => {
                    self.session.cell_home(true);
                    true
                }
                "end" if command => {
                    self.session.cell_end(true);
                    true
                }
                "home" if plain => {
                    self.session.cell_home(false);
                    true
                }
                "end" if plain => {
                    self.session.cell_end(false);
                    true
                }
                "pageup" if plain => {
                    self.session.cell_page(-1);
                    true
                }
                "pagedown" if plain => {
                    self.session.cell_page(1);
                    true
                }
                "enter" | "f2" if plain => {
                    self.session.edit_focused_cell();
                    true
                }
                _ if plain && self.session.focused_lookup() => event
                    .keystroke
                    .key_char
                    .as_ref()
                    .and_then(|text| {
                        let mut chars = text.chars();
                        let ch = chars.next()?;
                        if chars.next().is_none() && !ch.is_control() {
                            self.session.edit_focused_cell();
                            self.session.edit_insert(ch);
                            Some(())
                        } else {
                            None
                        }
                    })
                    .is_some(),
                _ => false,
            };
            if handled {
                cx.stop_propagation();
                cx.notify();
                return;
            }
        }
        if !editing {
            if key == "escape" {
                self.session.escape();
                cx.notify();
            }
            return;
        }
        let command = mods.control || mods.platform;
        if command && !mods.alt && key == "a" {
            self.session.select_all_text();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if mods.control || mods.alt || mods.platform {
            return;
        }
        let shift = mods.shift;
        let mut handled = true;
        match key.as_str() {
            "escape" => self.session.escape(),
            "enter" => {
                if self.session.palette.open {
                    let effect = self.session.confirm_palette();
                    if effect == Effect::Quit {
                        cx.quit();
                    }
                } else if let Some(view) = self.session.filter_for.clone() {
                    self.session.apply_filter(&view);
                } else if self.session.graph_query {
                    let query = self.session.graph_edit.text.clone();
                    if self.session.backend.is_live() {
                        self.session.backend.schedule_record_search(&query);
                    } else {
                        let hits = self.session.backend.search_records(&query, 8);
                        self.session.seed_graph(&hits);
                        self.session.graph_query = false;
                    }
                }
            }
            "backspace" => {
                if self.session.palette.open && self.session.palette_key_back() {
                } else {
                    self.session.backspace();
                }
            }
            "delete" | "forwarddelete" => self.session.delete_forward(),
            "left" => {
                if self.session.palette.open && !shift && self.session.palette_key_back() {
                } else {
                    self.session.move_left(shift);
                }
            }
            "right" => {
                if self.session.palette.open && !shift && self.session.palette_key_right() {
                } else {
                    self.session.move_right(shift);
                }
            }
            "home" => self.session.move_home(shift),
            "end" => self.session.move_end(shift),
            "space" => self.session.type_char(' '),
            "up" | "down" => handled = false,
            _ => {
                handled = false;
                if let Some(text) = &event.keystroke.key_char {
                    if let Some(ch) = text.chars().next() {
                        if text.chars().count() == 1 && !ch.is_control() {
                            self.session.type_char(ch);
                            handled = true;
                        }
                    }
                }
            }
        }
        if handled {
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn on_cell_key(&mut self, key: &str, mods: &gpui::Modifiers, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if mods.control || mods.platform || mods.alt {
            return;
        }
        let shift = mods.shift;
        let lookup = self.session.editing.as_ref().is_some_and(|edit| edit.lookup);
        match key {
            "escape" => self.session.cancel_edit(),
            "enter" if lookup => {
                self.session.confirm_lookup();
            }
            "enter" => {
                self.session.commit_edit(crate::session::EditMove::Down);
            }
            "tab" => {
                let next = if shift {
                    crate::session::EditMove::PrevField
                } else {
                    crate::session::EditMove::NextField
                };
                self.session.commit_edit(next);
            }
            "backspace" => self.session.edit_backspace(),
            "delete" | "forwarddelete" => self.session.edit_delete_forward(),
            "left" => self.session.edit_left(shift),
            "right" => self.session.edit_right(shift),
            "home" => self.session.edit_home(shift),
            "end" => self.session.edit_end(shift),
            "up" if lookup => self.session.move_lookup(-1),
            "down" if lookup => self.session.move_lookup(1),
            "up" => {
                self.session.commit_edit(crate::session::EditMove::Up);
            }
            "down" => {
                self.session.commit_edit(crate::session::EditMove::Down);
            }
            "space" => self.session.edit_insert(' '),
            _ => {
                if let Some(text) = &event.keystroke.key_char {
                    if let Some(ch) = text.chars().next() {
                        if text.chars().count() == 1 && !ch.is_control() {
                            self.session.edit_insert(ch);
                        }
                    }
                }
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn render_palette(&self, theme: &Theme, viewport: gpui::Size<gpui::Pixels>, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.session.palette_rows();
        let crumb = self.session.palette_crumb();
        let nested = crumb.len() > 1;
        let list_h = (f32::from(viewport.height) - 280.).max(160.);
        let mut list = div()
            .id("palette-list")
            .flex()
            .flex_col()
            .max_h(px(list_h))
            .overflow_y_scroll();
        if rows.is_empty() {
            list = list.child(
                div()
                    .px_3()
                    .py_3()
                    .text_color(rgb(theme.muted))
                    .child(format!("No results for \"{}\"", self.session.palette.query().trim())),
            );
        }
        for (index, row) in rows.iter().enumerate() {
            let on = index == self.session.palette.cursor && !row.header;
            let subtitle = if row.header {
                String::new()
            } else if !row.subtitle.is_empty() {
                row.subtitle.clone()
            } else {
                match &self.session.palette.mode {
                    PaletteMode::OpenView => "Open view".into(),
                    PaletteMode::PickLink(_) => "Use this view as the source".into(),
                    PaletteMode::PickTemplate => "New workspace".into(),
                    PaletteMode::PickEntity(_) => "Choose an entity".into(),
                    _ => String::new(),
                }
            };
            let drill = self.session.drill_row(row);
            list = list.child(
                div()
                    .id(SharedString::from(format!("pal-{index}")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .px_3()
                    .h(px(if row.header { 22. } else { 46. }))
                    .gap_2()
                    .bg(rgb(if on { theme.accent_dim } else { theme.raised }))
                    .border_l(px(3.))
                    .border_color(rgb(if on { theme.accent } else { theme.raised }))
                    .when(!row.header, |row_el| {
                        row_el.cursor_pointer().hover(|style| style.bg(rgb(theme.hover)))
                    })
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(if row.header { 11. } else { 13. }))
                                    .text_color(rgb(if row.header { theme.subtle } else { theme.text }))
                                    .child(row.title.clone()),
                            )
                            .when(!subtitle.is_empty(), |col| {
                                col.child(div().text_size(px(11.)).text_color(rgb(theme.subtle)).child(subtitle))
                            }),
                    )
                    .when(drill, |row_el| {
                        row_el.child(div().text_size(px(14.)).text_color(rgb(theme.accent)).child("›"))
                    })
                    .when(!row.shortcut.is_empty(), |row_el| {
                        row_el.child(
                            div()
                                .px_1()
                                .py(px(1.))
                                .rounded_sm()
                                .bg(rgb(theme.bg))
                                .border_1()
                                .border_color(rgb(theme.border))
                                .text_size(px(10.))
                                .text_color(rgb(theme.muted))
                                .child(row.shortcut.clone()),
                        )
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if this.session.palette_rows().get(index).is_some_and(|row| row.header) {
                            return;
                        }
                        this.session.palette.cursor = index;
                        let effect = this.session.confirm_palette();
                        if effect == Effect::Quit {
                            cx.quit();
                        }
                        cx.notify();
                    })),
            );
        }
        let placeholder = match &self.session.palette.mode {
            PaletteMode::Commands => "Search commands and records",
            PaletteMode::RenameWorkspace => "Workspace name",
            PaletteMode::OpenView => "Search views",
            PaletteMode::PickTemplate => "Search templates",
            PaletteMode::PickLink(_) => "Search sources",
            PaletteMode::PickEntity(_) => "Search entities",
        };
        div()
            .absolute()
            .size_full()
            .bg(hsla(0., 0., 0., 0.55))
            .flex()
            .flex_col()
            .items_center()
            .pt(px(72.))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.session.palette.open = false;
                cx.stop_propagation();
                cx.notify();
            }))
            .child(
                div()
                    .w(px(560.))
                    .max_h(px((f32::from(viewport.height) - 120.).max(240.)))
                    .rounded_md()
                    .bg(rgb(theme.raised))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()))
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .border_2()
                            .border_color(rgb(theme.accent))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .when(nested, |block| {
                                block.child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(theme.accent))
                                        .child(crumb.join("  ›  ")),
                                )
                            })
                            .child(
                                div().text_size(px(16.)).text_color(rgb(theme.text)).child(render_edit_line(
                                    &self.session.palette.edit,
                                    placeholder,
                                    theme,
                                )),
                            ),
                    )
                    .child(list)
                    .child(self.palette_footer(theme, nested)),
            )
            .into_any_element()
    }

    fn palette_footer(&self, theme: &Theme, nested: bool) -> impl IntoElement {
        let mut bar = div()
            .px_3()
            .py_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .border_t_1()
            .border_color(rgb(theme.border))
            .text_size(px(11.))
            .text_color(rgb(theme.subtle));
        for (key, label) in [("Enter", "run"), ("Esc", "close")] {
            bar = bar.child(self.palette_hint(theme, key, label));
        }
        if nested {
            bar = bar.child(self.palette_hint(theme, "⌫", "back"));
        }
        bar.child(self.palette_hint(theme, "Right", "open"))
    }

    fn palette_hint(&self, theme: &Theme, key: &str, label: &str) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(
                div()
                    .px_1()
                    .rounded_sm()
                    .bg(rgb(theme.bg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .text_color(rgb(theme.text))
                    .child(key.to_string()),
            )
            .child(label.to_string())
    }

    fn render_menu(&self, theme: &Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = &self.session.menu else {
            return div().into_any_element();
        };
        let max_h = crate::popover::popover_max_height(f32::from(window.viewport_size().height));
        let mut list = div()
            .id("menu-list")
            .min_w(px(220.))
            .max_h(px(max_h))
            .overflow_y_scroll()
            .py_1()
            .rounded_md()
            .bg(rgb(theme.raised))
            .border_1()
            .border_color(rgb(theme.border))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()));
        for (index, item) in menu.items.iter().enumerate() {
            let command = item.command.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("menu-{index}")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .px_3()
                    .h(px(28.))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(theme.hover)))
                    .child(div().flex_1().child(item.label.clone()))
                    .child(div().text_size(px(11.)).text_color(rgb(theme.subtle)).child(item.detail.clone()))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        this.session.menu = None;
                        match &command {
                            MenuCommand::Run(id) => this.exec(id, cx),
                            MenuCommand::Dismiss => cx.notify(),
                            MenuCommand::OpenLinked { source, record, kind, entity, relationship } => {
                                this.session.select(source, record);
                                this.session.open_linked_view(source, *kind, entity, relationship);
                                cx.notify();
                            }
                            MenuCommand::Open { .. } | MenuCommand::Link { .. } => cx.notify(),
                        }
                        cx.stop_propagation();
                    })),
            );
        }
        crate::popover::anchor_popover(point(px(menu.x), px(menu.y)), menu.grow_left, list)
    }
}

fn item(label: &str, detail: &str, command: MenuCommand) -> crate::session::MenuItem {
    crate::session::MenuItem {
        label: label.to_string(),
        detail: detail.to_string(),
        command,
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.session.reconcile();
        let theme = theme(self.session.file.theme);
        let layout = if let Some(zoom) = &self.session.file.zoom {
            if self.session.file.layout.contains(zoom) {
                idmc_layout::Node::tabs(zoom)
            } else {
                self.session.file.layout.clone()
            }
        } else {
            self.session.file.layout.clone()
        };
        let surface = if layout.views().is_empty() {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(theme.muted))
                .child("No views. Ctrl+K, then New table view.")
                .into_any_element()
        } else {
            self.render_node(&layout, Vec::new(), &theme, cx)
        };
        let viewport = window.viewport_size();
        let palette = self.session.palette.open.then(|| self.render_palette(&theme, viewport, cx));
        let menu = self.session.menu.is_some().then(|| self.render_menu(&theme, window, cx));
        let failure = self.session.backend.login_failure();
        let surface = if failure.is_some() {
            self.login_panel(&theme, cx)
        } else {
            surface
        };
        let mut root = div()
            .id("root")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .text_size(px(12.))
            .on_action(cx.listener(|this, _: &ShowPalette, _, cx| this.exec("palette.open", cx)))
            .on_action(cx.listener(|this, _: &ToggleTheme, _, cx| this.exec("theme.toggle", cx)))
            .on_action(cx.listener(|this, _: &SaveWorkspace, _, cx| this.exec("workspace.save", cx)))
            .on_action(cx.listener(|this, _: &ReloadWorkspace, _, cx| this.exec("workspace.reload", cx)))
            .on_action(cx.listener(|this, _: &ResetWorkspace, _, cx| this.exec("workspace.reset", cx)))
            .on_action(cx.listener(|this, _: &ReloadData, _, cx| this.exec("data.reload", cx)))
            .on_action(cx.listener(|this, _: &Quit, _, cx| this.exec("app.quit", cx)))
            .on_action(cx.listener(|this, _: &CloseView, _, cx| this.exec("view.close", cx)))
            .on_action(cx.listener(|this, _: &ZoomView, _, cx| this.exec("view.zoom", cx)))
            .on_action(cx.listener(|this, _: &FocusNext, _, cx| this.exec("view.focus_next", cx)))
            .on_action(cx.listener(|this, _: &FocusPrev, _, cx| this.exec("view.focus_prev", cx)))
            .on_action(cx.listener(|this, _: &MoveLeft, _, cx| this.exec("view.move_left", cx)))
            .on_action(cx.listener(|this, _: &MoveRight, _, cx| this.exec("view.move_right", cx)))
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| this.exec("view.move_up", cx)))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| this.exec("view.move_down", cx)))
            .on_action(cx.listener(|this, _: &NewTable, _, cx| this.exec("view.new_table", cx)))
            .on_action(cx.listener(|this, _: &NewDetail, _, cx| this.exec("view.new_detail", cx)))
            .on_action(cx.listener(|this, _: &NewTree, _, cx| this.exec("view.new_tree", cx)))
            .on_action(cx.listener(|this, _: &FieldChooser, _, cx| this.exec("view.fields", cx)))
            .on_action(cx.listener(|this, _: &ChooseLink, _, cx| this.exec("view.link", cx)))
            .on_action(cx.listener(|this, _: &ClearLink, _, cx| this.exec("view.unlink", cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.exec("selection.next", cx)))
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.exec("selection.prev", cx)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.on_key(event, cx)))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _: &MouseDownEvent, _, cx| {
                if this.session.menu.is_some() {
                    this.session.menu = None;
                    cx.notify();
                }
                if this.session.editing.as_ref().is_some_and(|edit| edit.lookup) {
                    this.session.cancel_edit();
                    cx.notify();
                }
            }))
            .child(self.title_bar(&theme, window, cx))
            .child(div().flex_1().min_h_0().p_2().child(surface))
            .children(self.confirm_bar(&theme, cx))
            .child(self.status_bar(&theme))
            .children(self.column_resize_overlay(cx));
        if let Some(palette) = palette {
            root = root.child(palette);
        }
        if let Some(menu) = menu {
            root = root.child(menu);
        }
        if failure.is_none() {
            if let Some(id) = self.session.chooser_for.clone() {
                if let Some(spec) = self.session.spec(&id).cloned() {
                    root = root.child(self.render_chooser(&id, &spec.entity, &spec.columns, &theme, viewport, cx));
                }
            }
            if let Some(menu) = self.render_lookup(&theme, viewport, cx) {
                root = root.child(menu);
            }
        }
        root
    }
}

impl Root {
    fn title_bar(&self, theme: &Theme, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let controls = cfg!(windows) || matches!(window.window_decorations(), Decorations::Client { .. });
        let mut bar = div()
            .h(px(40.))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .bg(rgb(theme.panel))
            .border_b_1()
            .border_color(rgb(theme.border))
            .when(cfg!(target_os = "macos"), |row| row.pl(px(72.)))
            .child(self.drag_cluster(theme));
        for (index, name) in self.session.file.tab_names().into_iter().enumerate() {
            let on = index == self.session.file.active;
            bar = bar.child(
                div()
                    .id(SharedString::from(format!("ws-{index}")))
                    .px_2()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .bg(rgb(if on { theme.raised } else { theme.panel }))
                    .text_color(rgb(if on { theme.text } else { theme.muted }))
                    .border_b_1()
                    .border_color(rgb(if on { theme.accent } else { theme.panel }))
                    .child(name)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.session.switch_workspace(index);
                        cx.notify();
                    })),
            );
        }
        bar = bar
            .child(
                div()
                    .id("ws-add")
                    .px_2()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .tooltip(crate::icons::tooltip("New workspace", *theme))
                    .child(crate::icons::icon("plus").w(px(14.)).h(px(14.)).text_color(rgb(theme.accent)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.exec("workspace.new", cx))),
            )
            .child(self.drag_spacer())
            .child(
                div()
                    .id("theme-toggle")
                    .px_2()
                    .py(px(2.))
                    .rounded_sm()
                    .cursor_pointer()
                    .text_color(rgb(theme.muted))
                    .hover(|style| style.bg(rgb(theme.hover)).text_color(rgb(theme.text)))
                    .child(self.session.file.theme.label())
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.exec("theme.toggle", cx))),
            )
            .child(
                div()
                    .id("palette-button")
                    .px_2()
                    .py(px(2.))
                    .rounded_sm()
                    .cursor_pointer()
                    .bg(rgb(theme.raised))
                    .text_color(rgb(theme.text))
                    .child("Ctrl+K")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.exec("palette.open", cx))),
            );
        if controls {
            bar = bar.child(self.window_controls(theme, window, cx));
        }
        bar
    }

    fn drag_cluster(&self, theme: &Theme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .pl_3()
            .h_full()
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(MouseButton::Left, |event: &MouseDownEvent, window, cx| {
                if cfg!(windows) {
                    return;
                }
                if event.click_count >= 2 {
                    window.zoom_window();
                } else {
                    window.start_window_move();
                }
                cx.stop_propagation();
            })
            .child(img("logo/mark.svg").w(px(18.)).h(px(20.)).object_fit(ObjectFit::Contain))
            .child(div().text_size(px(13.)).text_color(rgb(theme.text)).child("IDMC Client"))
    }

    fn drag_spacer(&self) -> impl IntoElement {
        div()
            .flex_1()
            .h_full()
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(MouseButton::Left, |event: &MouseDownEvent, window, cx| {
                if cfg!(windows) {
                    return;
                }
                if event.click_count >= 2 {
                    window.zoom_window();
                } else {
                    window.start_window_move();
                }
                cx.stop_propagation();
            })
    }

    fn window_controls(&self, theme: &Theme, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let maximized = window.is_maximized();
        div()
            .flex()
            .flex_row()
            .items_center()
            .h_full()
            .child(self.window_button("win-min", WindowControlArea::Min, "–", theme, cx, |window| window.minimize_window()))
            .child(self.window_button(
                "win-max",
                WindowControlArea::Max,
                if maximized { "❐" } else { "□" },
                theme,
                cx,
                |window| window.zoom_window(),
            ))
            .child(
                div()
                    .id("win-close")
                    .w(px(46.))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(14.))
                    .text_color(rgb(theme.text))
                    .window_control_area(WindowControlArea::Close)
                    .hover(|style| style.bg(rgb(theme.hover)))
                    .child("✕")
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _: &MouseDownEvent, _, cx| {
                        if !cfg!(windows) {
                            cx.quit();
                        }
                        cx.stop_propagation();
                    })),
            )
    }

    fn window_button(
        &self,
        id: &'static str,
        area: WindowControlArea,
        glyph: &'static str,
        theme: &Theme,
        cx: &mut Context<Self>,
        action: fn(&mut Window),
    ) -> impl IntoElement {
        div()
            .id(id)
            .w(px(46.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_size(px(14.))
            .text_color(rgb(theme.text))
            .window_control_area(area)
            .hover(|style| style.bg(rgb(theme.hover)))
            .child(glyph)
            .on_mouse_down(MouseButton::Left, cx.listener(move |_, _: &MouseDownEvent, window, cx| {
                if !cfg!(windows) {
                    action(window);
                }
                cx.stop_propagation();
            }))
    }

    fn login_panel(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let failure = self.session.backend.login_failure();
        let busy = self.session.backend.login_busy();
        let title = failure.as_ref().map(|item| item.cause.title()).unwrap_or("Login failed");
        let explanation = failure.as_ref().map(|item| item.cause.explanation()).unwrap_or("");
        let detail = failure.as_ref().map(|item| item.detail.clone()).unwrap_or_default();
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(460.))
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .rounded_md()
                    .bg(rgb(theme.raised))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .child(div().text_size(px(16.)).text_color(rgb(theme.text)).child(title))
                    .child(div().text_color(rgb(theme.muted)).child(explanation))
                    .child(div().text_size(px(11.)).text_color(rgb(theme.subtle)).child(detail))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .pt_2()
                            .child(
                                div()
                                    .id("login-retry")
                                    .px_3()
                                    .py_1()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .bg(rgb(theme.accent))
                                    .text_color(rgb(theme.on_accent))
                                    .child(if busy { "Retrying…" } else { "Retry" })
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        if !this.session.backend.login_busy() {
                                            this.session.retry_login();
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("login-mock")
                                    .px_3()
                                    .py_1()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .bg(rgb(theme.panel))
                                    .text_color(rgb(theme.text))
                                    .child("Continue with mock data")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        if let Err(err) = this.session.continue_with_mock() {
                                            this.session.status = err;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn column_resize_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.session.column_resize.is_none() {
            return None;
        }
        Some(
            div()
                .id("column-resize")
                .absolute()
                .size_full()
                .bg(transparent_black())
                .cursor(CursorStyle::ResizeColumn)
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                    let Some(resize) = this.session.column_resize.clone() else { return };
                    let dx = f32::from(event.position.x) - resize.origin_x;
                    this.session.set_column_width(&resize.view, &resize.field, resize.origin_width + dx);
                    cx.notify();
                }))
                .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &gpui::MouseUpEvent, _, cx| {
                    if this.session.column_resize.take().is_some() {
                        this.session.status = this.session.save().unwrap_or_else(|err| err);
                    }
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    fn confirm_bar(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (_, _, label) = self.session.pending_delete.clone()?;
        Some(
            div()
                .h(px(36.))
                .px_3()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .bg(rgb(theme.raised))
                .border_t_1()
                .border_color(rgb(theme.border))
                .text_size(px(12.))
                .child(format!("Delete {label}?"))
                .child(
                    div()
                        .id("confirm-delete")
                        .px_2()
                        .py(px(2.))
                        .rounded_sm()
                        .cursor_pointer()
                        .bg(rgb(theme.accent))
                        .text_color(rgb(theme.on_accent))
                        .child("Delete")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.exec("record.delete_confirm", cx))),
                )
                .child(
                    div()
                        .id("cancel-delete")
                        .px_2()
                        .py(px(2.))
                        .rounded_sm()
                        .cursor_pointer()
                        .child("Cancel")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.session.cancel_delete();
                            cx.notify();
                        })),
                ),
        )
    }

    fn status_bar(&self, theme: &Theme) -> impl IntoElement {
        let dirty = self.session.dirty_records();
        let connection = self.session.backend.connection();
        let summary = self.session.backend.model().summary();
        let mut parts = vec![connection, summary];
        if dirty {
            parts.push("Unsaved changes".into());
        }
        if !self.session.status.is_empty() {
            parts.push(self.session.status.clone());
        }
        let text = parts.join(" · ");
        div()
            .h(px(26.))
            .px_3()
            .flex()
            .items_center()
            .bg(rgb(theme.panel))
            .border_t_1()
            .border_color(rgb(theme.border))
            .text_size(px(11.))
            .text_color(rgb(if dirty { theme.accent } else { theme.muted }))
            .child(text)
    }
}

pub fn run(session: Session) {
    Application::new().with_assets(crate::icons::IconAssets).run(move |cx: &mut App| {
        bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("IDMC Client".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(12.), px(12.))),
                }),
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            },
            |window, cx| {
                let root = cx.new(|cx| Root::new(session, cx));
                let focus = root.read(cx).focus.clone();
                window.focus(&focus);
                root
            },
        )
        .expect("open window");
        cx.activate(true);
    });
}

fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-k", ShowPalette, None),
        KeyBinding::new("ctrl-shift-l", ToggleTheme, None),
        KeyBinding::new("ctrl-s", SaveWorkspace, None),
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("ctrl-w", CloseView, None),
        KeyBinding::new("ctrl-alt-z", ZoomView, None),
        KeyBinding::new("ctrl-pagedown", FocusNext, None),
        KeyBinding::new("ctrl-pageup", FocusPrev, None),
        KeyBinding::new("ctrl-alt-left", MoveLeft, None),
        KeyBinding::new("ctrl-alt-right", MoveRight, None),
        KeyBinding::new("ctrl-alt-up", MoveUp, None),
        KeyBinding::new("ctrl-alt-down", MoveDown, None),
        KeyBinding::new("ctrl-alt-c", FieldChooser, None),
        KeyBinding::new("ctrl-alt-l", ChooseLink, None),
        KeyBinding::new("down", SelectNext, None),
        KeyBinding::new("up", SelectPrev, None),
    ]);
}

pub fn main() {
    match launch_from_args(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(Launch::Run(session)) => run(session),
        Ok(Launch::Done { code, message }) => {
            if code == 0 {
                println!("{message}");
            } else {
                eprintln!("{message}");
            }
            std::process::exit(code);
        }
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(if err.starts_with("idmc-client") { 0 } else { 1 });
        }
    }
}
