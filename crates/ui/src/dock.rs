//! Tiling layout: splits, tabs, and drag-to-dock.

use gpui::{
    AnyElement, Context, CursorStyle, MouseButton, MouseDownEvent, Point, SharedString, div, px,
    prelude::*, relative, rgb,
};
use idmc_layout::{Dir, Node, Zone};

use crate::app::{DragDivider, DragView, Empty, Ghost, Root};
use crate::theme::Theme;

impl Root {
    pub fn render_node(
        &self,
        node: &Node,
        path: Vec<usize>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node {
            Node::Tabs { views, active } => self.render_pane(views, *active, theme, cx),
            Node::Split { dir, ratios, children } => {
                let dir = *dir;
                let path_for_drag = path.clone();
                let mut row = div()
                    .id(SharedString::from(format!("split{path:?}")))
                    .flex()
                    .size_full()
                    .relative()
                    .on_drag_move(cx.listener(move |this, event: &gpui::DragMoveEvent<DragDivider>, _, cx| {
                        let drag = event.drag(cx).clone();
                        if drag.path != path_for_drag || drag.dir != dir {
                            return;
                        }
                        let bounds = event.bounds;
                        let pos = event.event.position;
                        let fraction = if dir == Dir::Horizontal {
                            (pos.x - bounds.origin.x) / bounds.size.width
                        } else {
                            (pos.y - bounds.origin.y) / bounds.size.height
                        };
                        this.session.file.layout.set_divider(&drag.path, drag.idx, fraction);
                        this.dirty = true;
                        cx.notify();
                    }));
                row = if dir == Dir::Horizontal { row.flex_row() } else { row.flex_col() };
                let last = children.len().saturating_sub(1);
                for (index, (child, ratio)) in children.iter().zip(ratios).enumerate() {
                    let mut child_path = path.clone();
                    child_path.push(index);
                    let mut cell = div().relative().overflow_hidden().flex_shrink_0().min_h_0().min_w_0();
                    cell = if dir == Dir::Horizontal {
                        cell.w(relative(*ratio)).h_full()
                    } else {
                        cell.h(relative(*ratio)).w_full()
                    };
                    if index < last {
                        cell = if dir == Dir::Horizontal { cell.pr(px(5.)) } else { cell.pb(px(5.)) };
                    }
                    cell = cell.child(self.render_node(child, child_path, theme, cx));
                    if index < last {
                        let handle_path = path.clone();
                        let line = div().absolute().bg(rgb(theme.border));
                        let line = if dir == Dir::Horizontal {
                            line.left(px(2.)).top_0().w(px(1.)).h_full()
                        } else {
                            line.top(px(2.)).left_0().h(px(1.)).w_full()
                        };
                        let mut handle = div()
                            .id(SharedString::from(format!("div{path:?}-{index}")))
                            .absolute()
                            .child(line)
                            .on_drag(
                                DragDivider { path: handle_path, idx: index, dir },
                                |_, _, _, cx| cx.new(|_| Empty),
                            )
                            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &gpui::MouseUpEvent, _, cx| {
                                if this.dirty {
                                    this.session.status = this.session.save().unwrap_or_else(|err| err);
                                    this.dirty = false;
                                    cx.notify();
                                }
                            }));
                        handle = if dir == Dir::Horizontal {
                            handle.right(px(-3.)).top_0().h_full().w(px(8.)).cursor(CursorStyle::ResizeLeftRight)
                        } else {
                            handle.bottom(px(-3.)).left_0().w_full().h(px(8.)).cursor(CursorStyle::ResizeUpDown)
                        };
                        cell = cell.child(handle);
                    }
                    row = row.child(cell);
                }
                row.into_any_element()
            }
        }
    }

    pub fn render_pane(
        &self,
        views: &[String],
        active: usize,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(current) = views.get(active).or(views.first()).cloned() else {
            return div().size_full().into_any_element();
        };
        let focused = views.iter().any(|id| self.session.file.focus.as_deref() == Some(id.as_str()));
        let zoomed = self.session.file.zoom.as_deref() == Some(current.as_str());
        let mut tabs = div()
            .flex()
            .flex_row()
            .items_end()
            .h(px(32.))
            .px_1()
            .bg(rgb(theme.panel))
            .overflow_hidden();
        for id in views {
            tabs = tabs.child(self.pane_tab(id, id == &current, focused, theme, cx));
        }
        let add_target = current.clone();
        let menu_target = current.clone();
        let zoom_target = current.clone();
        tabs = tabs
            .child(
                div()
                    .id(SharedString::from(format!("add-{current}")))
                    .ml_1()
                    .mb(px(4.))
                    .px_2()
                    .rounded_sm()
                    .text_color(rgb(theme.subtle))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(theme.hover)).text_color(rgb(theme.text)))
                    .child("+")
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        this.open_add_menu(&add_target, event.position);
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id(SharedString::from(format!("more-{current}")))
                    .mb(px(4.))
                    .px_2()
                    .rounded_sm()
                    .text_color(rgb(theme.subtle))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(theme.hover)).text_color(rgb(theme.text)))
                    .child("···")
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        this.open_view_menu(&menu_target, event.position);
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id(SharedString::from(format!("zoom-{current}")))
                    .mr_1()
                    .mb(px(4.))
                    .px_2()
                    .rounded_sm()
                    .cursor_pointer()
                    .text_color(rgb(if zoomed { theme.accent } else { theme.subtle }))
                    .hover(|style| style.bg(rgb(theme.hover)).text_color(rgb(theme.text)))
                    .child(if zoomed { "Restore" } else { "Max" })
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                        this.session.file.focus = Some(zoom_target.clone());
                        this.exec("view.zoom", cx);
                    })),
            );
        let focus_id = current.clone();
        let body = div()
            .id(SharedString::from(format!("body-{current}")))
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(self.render_view(&current, theme, cx))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                if this.session.file.focus.as_deref() != Some(focus_id.as_str()) {
                    this.session.file.focus = Some(focus_id.clone());
                    this.session.file.layout.activate(&focus_id);
                    cx.notify();
                }
            }));
        let drop_target = current.clone();
        let drop_check = current.clone();
        let mut pane = div()
            .id(SharedString::from(format!("pane-{current}")))
            .relative()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(rgb(theme.panel))
            .border_1()
            .border_color(rgb(if focused { theme.accent_dim } else { theme.panel }))
            .rounded_md()
            .child(tabs)
            .child(body)
            .on_drag_move(cx.listener(move |this, event: &gpui::DragMoveEvent<DragView>, _, cx| {
                let bounds = event.bounds;
                let pos = event.event.position;
                if bounds.contains(&pos) {
                    let fx = (pos.x - bounds.origin.x) / bounds.size.width;
                    let fy = (pos.y - bounds.origin.y) / bounds.size.height;
                    let zone = if fx < 0.22 {
                        Zone::Left
                    } else if fx > 0.78 {
                        Zone::Right
                    } else if fy < 0.22 {
                        Zone::Top
                    } else if fy > 0.78 {
                        Zone::Bottom
                    } else {
                        Zone::Center
                    };
                    let next = Some((drop_target.clone(), zone));
                    if this.session.dock_hover != next {
                        this.session.dock_hover = next;
                        cx.notify();
                    }
                } else if this.session.dock_hover.as_ref().is_some_and(|(id, _)| id == &drop_target) {
                    this.session.dock_hover = None;
                    cx.notify();
                }
            }))
            .on_drop(cx.listener(move |this, drag: &DragView, _, cx| {
                if let Some((target, zone)) = this.session.dock_hover.take() {
                    if target == drop_check && !(drag.id == target && zone == Zone::Center) {
                        this.session.file.layout.move_view(&drag.id, &target, zone).ok();
                        this.session.file.focus = Some(drag.id.clone());
                        this.session.file.layout.activate(&drag.id);
                        this.session.status = this.session.save().unwrap_or_else(|err| err);
                    }
                }
                cx.notify();
            }));
        if cx.has_active_drag() {
            if let Some((target, zone)) = &self.session.dock_hover {
                if target == &current {
                    pane = pane.child(dock_preview(*zone, theme));
                }
            }
        }
        pane.into_any_element()
    }

    fn pane_tab(
        &self,
        id: &str,
        selected: bool,
        focused: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let title = self.session.view_title(id);
        let kind = self.session.spec(id).map(|spec| spec.kind);
        let mark = match kind {
            Some(idmc_model::ViewKind::Tree) => "Tree",
            Some(idmc_model::ViewKind::Detail) => "Detail",
            _ => "Table",
        };
        let focus_id = id.to_string();
        let close_id = id.to_string();
        let menu_id = id.to_string();
        let drag_id = id.to_string();
        div()
            .id(SharedString::from(format!("tab-{id}")))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .pl_2()
            .pr_1()
            .h(px(26.))
            .rounded_t_sm()
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(rgb(if selected { theme.text } else { theme.muted }))
            .when(selected, |el| {
                el.bg(rgb(theme.raised))
                    .border_t_2()
                    .border_color(rgb(if focused { theme.accent } else { theme.raised }))
            })
            .when(!selected, |el| el.hover(|style| style.bg(rgb(theme.hover)).text_color(rgb(theme.text))))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(if selected && focused { theme.accent } else { theme.subtle }))
                    .child(mark),
            )
            .child(title.clone())
            .child(
                div()
                    .id(SharedString::from(format!("x-{id}")))
                    .px(px(4.))
                    .text_color(rgb(theme.subtle))
                    .hover(|style| style.text_color(rgb(theme.text)))
                    .child("×")
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                        this.session.close_view(&close_id);
                        this.session.status = this.session.save().unwrap_or_else(|err| err);
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                this.session.file.focus = Some(focus_id.clone());
                this.session.file.layout.activate(&focus_id);
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Middle, cx.listener({
                let id = menu_id.clone();
                move |this, _: &MouseDownEvent, _, cx| {
                    this.session.close_view(&id);
                    let _ = this.session.save();
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                this.open_view_menu(&menu_id, event.position);
                cx.notify();
            }))
            .on_drag(
                DragView { id: drag_id, title },
                |drag: &DragView, _: Point<Pixels>, _, cx| {
                    let title = drag.title.clone();
                    cx.new(|_| Ghost { title })
                },
            )
    }
}

fn dock_preview(zone: Zone, theme: &Theme) -> impl IntoElement {
    let (x, y, w, h) = match zone {
        Zone::Left => (0.0, 0.0, 0.5, 1.0),
        Zone::Right => (0.5, 0.0, 0.5, 1.0),
        Zone::Top => (0.0, 0.0, 1.0, 0.5),
        Zone::Bottom => (0.0, 0.5, 1.0, 0.5),
        Zone::Center => (0.0, 0.0, 1.0, 1.0),
    };
    let label = match zone {
        Zone::Center => "Add as tab",
        Zone::Left => "Dock left",
        Zone::Right => "Dock right",
        Zone::Top => "Dock above",
        Zone::Bottom => "Dock below",
    };
    div()
        .absolute()
        .left(relative(x))
        .top(relative(y))
        .w(relative(w))
        .h(relative(h))
        .p(px(4.))
        .child(
            div()
                .size_full()
                .rounded_md()
                .bg(rgb(theme.drop))
                .border_2()
                .border_color(rgb(theme.accent))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(theme.accent_dim))
                        .text_color(rgb(theme.text))
                        .child(label),
                ),
        )
}

use gpui::Pixels;
