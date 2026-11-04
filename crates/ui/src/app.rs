//! GPUI shell: command palette, menus, theme, and the docking surface.

use gpui::{
    actions, div, px, prelude::*, rgb, size, AnyElement, App, Application, Bounds, ClickEvent,
    Context, FocusHandle, KeyBinding, KeyDownEvent, MouseButton, MouseDownEvent, Render,
    SharedString, Window, WindowBounds, WindowOptions,
};
use idmc_backend::Backend;
use idmc_layout::Dir;

use crate::session::{launch_from_args, Effect, MenuCommand, PaletteMode, Session};
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
            .bg(rgb(0x1a4552))
            .text_color(rgb(0xe7e9ee))
            .text_size(px(12.))
            .child(self.title.clone())
    }
}

pub struct Root {
    pub session: Session,
    pub focus: FocusHandle,
    pub dirty: bool,
}

impl Root {
    fn new(session: Session, cx: &mut Context<Self>) -> Self {
        Self {
            session,
            focus: cx.focus_handle(),
            dirty: false,
        }
    }

    pub fn exec(&mut self, id: &str, cx: &mut Context<Self>) {
        let effect = self.session.perform(id);
        if matches!(
            id,
            "theme.toggle"
                | "workspace.save"
                | "workspace.reset"
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
            items: vec![
                item("New table view", "", MenuCommand::Run("view.new_table")),
                item("New detail view", "", MenuCommand::Run("view.new_detail")),
                item("New tree view", "", MenuCommand::Run("view.new_tree")),
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
        items.push(item("Choose source link", "Ctrl+Alt+L", MenuCommand::Run("view.link")));
        items.push(item("Clear source link", "", MenuCommand::Run("view.unlink")));
        items.push(item("Maximize / restore", "Ctrl+Alt+Z", MenuCommand::Run("view.zoom")));
        items.push(item("Close view", "Ctrl+W", MenuCommand::Run("view.close")));
        self.session.menu = Some(crate::session::MenuState {
            x: f32::from(pos.x),
            y: f32::from(pos.y),
            items,
        });
    }

    fn on_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let mods = &event.keystroke.modifiers;
        if mods.control || mods.alt || mods.platform {
            return;
        }
        if self.session.palette.open || self.session.chooser_for.is_some() {
            match key {
                "escape" => self.session.escape(),
                "enter" => {
                    if self.session.palette.open {
                        let effect = self.session.confirm_palette();
                        if effect == Effect::Quit {
                            cx.quit();
                        }
                    }
                }
                "backspace" => self.session.backspace(),
                _ => {
                    if let Some(text) = &event.keystroke.key_char {
                        if let Some(ch) = text.chars().next() {
                            if text.chars().count() == 1 && !ch.is_control() {
                                self.session.type_char(ch);
                            }
                        }
                    }
                }
            }
            cx.notify();
        } else if key == "escape" {
            self.session.escape();
            cx.notify();
        }
    }

    fn render_palette(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.session.palette_rows();
        let title = match &self.session.palette.mode {
            PaletteMode::Commands => "Commands",
            PaletteMode::PickEntity(kind) => match kind {
                idmc_model::ViewKind::Table => "New table",
                idmc_model::ViewKind::Detail => "New detail",
                idmc_model::ViewKind::Tree => "New tree",
            },
            PaletteMode::PickLink(_) => "Source link",
        };
        let mut list = div()
            .id("palette-list")
            .flex()
            .flex_col()
            .max_h(px(360.))
            .overflow_y_scroll();
        if rows.is_empty() {
            list = list.child(div().px_3().py_2().text_color(rgb(theme.muted)).child("No matches"));
        }
        for (index, row) in rows.iter().enumerate() {
            let on = index == self.session.palette.cursor;
            list = list.child(
                div()
                    .id(SharedString::from(format!("pal-{index}")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .px_3()
                    .h(px(30.))
                    .gap_2()
                    .bg(rgb(if on { theme.accent_dim } else { theme.raised }))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(theme.hover)))
                    .child(div().flex_1().child(row.title.clone()))
                    .child(div().text_size(px(11.)).text_color(rgb(theme.subtle)).child(row.shortcut.clone()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.session.palette.cursor = index;
                        let effect = this.session.confirm_palette();
                        if effect == Effect::Quit {
                            cx.quit();
                        }
                        cx.notify();
                    })),
            );
        }
        div()
            .absolute()
            .top(px(72.))
            .left(px(0.))
            .right(px(0.))
            .flex()
            .justify_center()
            .child(
                div()
                    .w(px(520.))
                    .rounded_md()
                    .bg(rgb(theme.raised))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(div().text_size(px(11.)).text_color(rgb(theme.subtle)).child(title))
                            .child(div().text_size(px(14.)).child(if self.session.palette.query.is_empty() {
                                "Type to filter".to_string()
                            } else {
                                self.session.palette.query.clone()
                            })),
                    )
                    .child(list),
            )
            .into_any_element()
    }

    fn render_menu(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = &self.session.menu else {
            return div().into_any_element();
        };
        let mut list = div()
            .absolute()
            .left(px(menu.x))
            .top(px(menu.y))
            .min_w(px(220.))
            .py_1()
            .rounded_md()
            .bg(rgb(theme.raised))
            .border_1()
            .border_color(rgb(theme.border))
            .flex()
            .flex_col();
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
                            MenuCommand::Open { .. } | MenuCommand::Link { .. } => cx.notify(),
                        }
                        cx.stop_propagation();
                    })),
            );
        }
        list.into_any_element()
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
        window.set_window_title("IDMC Client");
        self.session.reconcile();
        let theme = theme(self.session.file.theme);
        let summary = self.session.backend.model().summary();
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
        let palette = self.session.palette.open.then(|| self.render_palette(&theme, cx));
        let menu = self.session.menu.is_some().then(|| self.render_menu(&theme, cx));
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
            }))
            .child(self.chrome(&summary, &theme, cx))
            .child(div().flex_1().min_h_0().p_2().child(surface))
            .child(self.status_bar(&theme));
        if let Some(palette) = palette {
            root = root.child(palette);
        }
        if let Some(menu) = menu {
            root = root.child(menu);
        }
        root
    }
}

impl Root {
    fn chrome(&self, summary: &str, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(40.))
            .px_3()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .bg(rgb(theme.panel))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(div().text_size(px(13.)).child("IDMC Client"))
            .child(div().text_size(px(11.)).text_color(rgb(theme.muted)).child(summary.to_string()))
            .child(div().flex_1())
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
            )
    }

    fn status_bar(&self, theme: &Theme) -> impl IntoElement {
        div()
            .h(px(26.))
            .px_3()
            .flex()
            .items_center()
            .bg(rgb(theme.panel))
            .border_t_1()
            .border_color(rgb(theme.border))
            .text_size(px(11.))
            .text_color(rgb(theme.muted))
            .child(self.session.status.clone())
    }
}

pub fn run(session: Session) {
    Application::new().run(move |cx: &mut App| {
        bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
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
        Ok(session) => run(session),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(if err.starts_with("idmc-client") { 0 } else { 1 });
        }
    }
}
