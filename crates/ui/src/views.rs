//! Table, detail, and tree views. Every label and column comes from the model.

use gpui::{
    canvas, div, point, px, prelude::*, rgb, uniform_list, AnyElement, Context, CursorStyle,
    DispatchPhase, ListHorizontalSizingBehavior, MouseButton, MouseDownEvent, ScrollStrategy,
    ScrollWheelEvent, SharedString,
};
use idmc_model::Record;
use idmc_backend::Backend;
use idmc_model::{FieldType, FieldValue, ViewKind};

use crate::app::Root;
use crate::theme::Theme;

impl Root {
    pub fn render_view(&self, id: &str, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(spec) = self.session.spec(id).cloned() else {
            return div().p_4().child("View closed").into_any_element();
        };
        match spec.kind {
            ViewKind::Table => self.render_table(id, &spec.entity, &spec.columns, theme, cx),
            ViewKind::Detail => self.render_detail(id, &spec.entity, theme, cx),
            ViewKind::Tree => self.render_tree(id, &spec.entity, theme, cx),
            ViewKind::Graph => self.render_graph(id, theme, cx),
        }
    }

    fn view_header(&self, id: &str, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.session.view_title(id);
        let caption = self.session.link_caption(id);
        let kind = self.session.spec(id).map(|spec| spec.kind);
        let count = if kind == Some(ViewKind::Tree) {
            self.session.tree_rows(self.session.spec(id).map(|s| s.entity.as_str()).unwrap_or("")).len()
        } else {
            self.session.table_len(id)
        };
        let menu_id = id.to_string();
        let fields_id = id.to_string();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(36.))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(div().text_size(px(13.)).child(title))
            .child(div().text_size(px(11.)).text_color(rgb(theme.muted)).child(format!("{count}")))
            .when(self.session.is_master_table(id), |el| {
                let view = id.to_string();
                let filtering = self.session.filter_for.as_deref() == Some(id);
                let mut row = el.child(self.header_button(id, "filter", "Filter", theme, cx, move |this, cx| {
                    let current = this.session.spec(&view).map(|spec| spec.query.clone()).unwrap_or_default();
                    this.session.filter_edit = crate::edit::TextEdit::new();
                    for ch in current.chars() {
                        this.session.filter_edit.insert(ch);
                    }
                    this.session.filter_for = Some(view.clone());
                    cx.notify();
                }));
                if filtering {
                    row = row.child(
                        div()
                            .w(px(180.))
                            .text_size(px(12.))
                            .child(crate::edit::render_edit_line(&self.session.filter_edit, "Search or field=value", theme)),
                    );
                }
                row
            })
            .when(caption.is_some(), |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(theme.accent))
                        .child(caption.unwrap_or_default()),
                )
            })
            .child(div().flex_1())
            .when(matches!(kind, Some(ViewKind::Table | ViewKind::Detail)), |el| {
                let id = menu_id.clone();
                el.child(self.header_button(&format!("graph-{id}"), "graph", "Show in graph", theme, cx, move |this, cx| {
                    this.session.file.focus = Some(id.clone());
                    this.exec("record.show_in_graph", cx);
                }))
            })
            .when(kind == Some(ViewKind::Table), |el| {
                let id = fields_id.clone();
                let button = id.clone();
                el.child(self.header_button(&button, "columns", "Columns", theme, cx, move |this, cx| {
                    this.session.file.focus = Some(id.clone());
                    this.exec("view.fields", cx);
                }))
            })
            .child(self.header_button(id, "link", "Choose source link", theme, cx, move |this, cx| {
                this.session.file.focus = Some(menu_id.clone());
                this.exec("view.link", cx);
            }))
            .when(matches!(kind, Some(ViewKind::Table | ViewKind::Detail)), |el| {
                let fresh = id.to_string();
                let remove = id.to_string();
                let save_id = id.to_string();
                let revert_id = id.to_string();
                let mut row = el
                    .child(self.header_button(&format!("new-{id}"), "plus", "New record", theme, cx, move |this, cx| {
                        this.session.file.focus = Some(fresh.clone());
                        this.exec("record.new", cx);
                    }))
                    .child(self.header_button(&format!("del-{id}"), "trash", "Delete", theme, cx, move |this, cx| {
                        this.session.file.focus = Some(remove.clone());
                        this.exec("record.delete", cx);
                    }));
                if self.session.dirty_records() {
                    row = row
                        .child(self.header_button(&format!("save-{id}"), "save", "Save", theme, cx, move |this, cx| {
                            this.session.file.focus = Some(save_id.clone());
                            this.exec("record.save", cx);
                        }))
                        .child(self.header_button(&format!("revert-{id}"), "undo", "Revert", theme, cx, move |this, cx| {
                            this.session.file.focus = Some(revert_id.clone());
                            this.exec("record.revert", cx);
                        }));
                }
                row
            })
    }

    fn header_button(
        &self,
        view: &str,
        icon_name: &'static str,
        tip: &'static str,
        theme: &Theme,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        div()
            .id(SharedString::from(format!("hdr-{view}-{icon_name}")))
            .w(px(26.))
            .h(px(26.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .text_color(rgb(theme.text))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme.hover)))
            .tooltip(crate::icons::tooltip(tip, *theme))
            .child(crate::icons::icon(icon_name).w(px(14.)).h(px(14.)).text_color(rgb(theme.text)))
            .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| action(this, cx)))
    }

    fn render_table(
        &self,
        id: &str,
        entity: &str,
        columns: &[String],
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let master = self.session.is_master_table(id);
        let rows = if master { std::sync::Arc::new(Vec::new()) } else { self.session.rows_cached(id) };
        let selected = self.session.file.selections.get(id).cloned();
        let column_labels: Vec<String> = columns
            .iter()
            .map(|column| {
                self.session
                    .backend
                    .model()
                    .entity(entity)
                    .and_then(|ent| ent.field(column))
                    .map(|field| field.label.clone())
                    .unwrap_or_else(|| column.clone())
            })
            .collect();
        let widths: Vec<f32> = columns.iter().map(|column| self.session.column_width(id, column)).collect();
        let width_px: f32 = widths.iter().sum();
        let mut header = div().flex().flex_row().h(px(28.)).bg(rgb(theme.raised)).items_center();
        for (index, ((column, label), width)) in columns.iter().zip(column_labels.iter()).zip(widths.iter()).enumerate() {
            let view_id = id.to_string();
            let field = column.clone();
            let label_text = label.clone();
            let origin_width = *width;
            let drop_here = self.session.column_drop.as_ref().is_some_and(|(view, at)| view == id && *at == index);
            let mut cell = div()
                .id(SharedString::from(format!("head-{id}-{column}")))
                .relative()
                .w(px(*width))
                .h_full()
                .flex_shrink_0()
                .px_2()
                .flex()
                .items_center()
                .text_size(px(11.))
                .text_color(rgb(theme.muted))
                .overflow_hidden()
                .whitespace_nowrap()
                .when(drop_here, |cell| cell.border_l(px(2.)).border_color(rgb(theme.accent)))
                .child(
                    div()
                        .id(SharedString::from(format!("drag-{id}-{column}")))
                        .flex_1()
                        .overflow_hidden()
                        .cursor(CursorStyle::OpenHand)
                        .child(label_text.clone())
                        .on_drag(
                            crate::app::DragColumn { view: view_id.clone(), field: field.clone(), label: label_text },
                            |drag, _, _, cx| cx.new(|_| crate::app::Ghost { title: drag.label.clone() }),
                        ),
                )
                .on_drag_move(cx.listener({
                    let view_id = view_id.clone();
                    move |this, event: &gpui::DragMoveEvent<crate::app::DragColumn>, _, cx| {
                        let bounds = event.bounds;
                        let pos = event.event.position;
                        let insert = if pos.x < bounds.origin.x + bounds.size.width / 2. {
                            index
                        } else {
                            index + 1
                        };
                        this.session.column_drop = Some((view_id.clone(), insert));
                        cx.notify();
                    }
                }))
                .on_drop(cx.listener({
                    let view_id = view_id.clone();
                    move |this, drag: &crate::app::DragColumn, _, cx| {
                        if drag.view == view_id {
                            if let Some((view, at)) = this.session.column_drop.take() {
                                this.session.drop_column(&view, &drag.field, at);
                                this.session.status = this.session.save().unwrap_or_else(|err| err);
                            }
                        }
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .id(SharedString::from(format!("resize-{id}-{column}")))
                        .absolute()
                        .right_0()
                        .top_0()
                        .w(px(8.))
                        .h_full()
                        .cursor(CursorStyle::ResizeColumn)
                        .on_mouse_down(MouseButton::Left, cx.listener({
                            let view_id = view_id.clone();
                            let field = field.clone();
                            move |this, event: &MouseDownEvent, _, cx| {
                                if event.click_count >= 2 {
                                    this.session.autofit_column(&view_id, &field);
                                    this.session.column_resize = None;
                                    this.session.status = this.session.save().unwrap_or_else(|err| err);
                                } else {
                                    this.session.column_resize = Some(crate::session::ColumnResize {
                                        view: view_id.clone(),
                                        field: field.clone(),
                                        origin_x: f32::from(event.position.x),
                                        origin_width,
                                    });
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }
                        })),
                );
            if master {
                let sort_view = view_id.clone();
                let sort_field = field.clone();
                cell = cell.on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                    this.session.toggle_sort(&sort_view, &sort_field);
                    cx.notify();
                }));
            }
            header = header.child(cell);
        }
        let width = px(width_px.max(1.));
        let body = if master {
            self.session.sync_master(id);
            let total = self.session.table_len(id);
            let known = self.session.backend.master_known(entity);
            if !known {
                div().flex_1().p_4().text_color(rgb(theme.muted)).child("Loading the first window…").into_any_element()
            } else if total == 0 {
                div().flex_1().p_4().text_color(rgb(theme.muted)).child("No records match.").into_any_element()
            } else {
                let theme = *theme;
                let entity = entity.to_string();
                let columns = columns.to_vec();
                let widths = widths.clone();
                let view_id = id.to_string();
                let selected = selected.clone();
                let this = cx.weak_entity();
                let v_scroll = self.list_scroll(id);
                uniform_list(SharedString::from(format!("rows-{id}")), total, move |range, _window, app| {
                    let entity = entity.clone();
                    let columns = columns.clone();
                    let widths = widths.clone();
                    let view_id = view_id.clone();
                    let selected = selected.clone();
                    this.update(app, |root, cx| {
                        root.session.backend.master_touch(&entity, range.start, range.end);
                        let mut items = Vec::with_capacity(range.len());
                        for index in range {
                            if let Some(row) = root.session.master_row(&entity, index) {
                                let on = selected.as_deref() == Some(row.id.as_str());
                                items.push(root.table_row(index, &view_id, &entity, &columns, &widths, &row, on, &theme, cx));
                            } else {
                                items.push(root.placeholder_row(index, &view_id, &theme));
                            }
                        }
                        items
                    })
                    .unwrap_or_default()
                })
                .track_scroll(v_scroll)
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .flex_1()
                .min_h_0()
                .w(width)
                .into_any_element()
            }
        } else if rows.is_empty() {
            div()
                .flex_1()
                .min_h_0()
                .p_4()
                .text_color(rgb(theme.muted))
                .child("No records. Select a source row, or clear the link to see every record.")
                .into_any_element()
        } else {
            let theme = *theme;
            let entity = entity.to_string();
            let columns = columns.to_vec();
            let widths = widths.clone();
            let view_id = id.to_string();
            let this = cx.weak_entity();
            let v_scroll = self.list_scroll(id);
            uniform_list(
                SharedString::from(format!("rows-{id}")),
                rows.len(),
                move |range, _window, app| {
                    let rows = rows.clone();
                    let entity = entity.clone();
                    let columns = columns.clone();
                    let widths = widths.clone();
                    let view_id = view_id.clone();
                    let selected = selected.clone();
                    this.update(app, |root, cx| {
                        let mut items = Vec::with_capacity(range.len());
                        for index in range {
                            let Some(row) = rows.get(index) else { continue };
                            let on = selected.as_deref() == Some(row.id.as_str());
                            items.push(root.table_row(
                                index, &view_id, &entity, &columns, &widths, row, on, &theme, cx,
                            ));
                        }
                        items
                    })
                    .unwrap_or_default()
                },
            )
            .track_scroll(v_scroll)
            .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
            .flex_1()
            .min_h_0()
            .w(width)
            .into_any_element()
        };
        let h_scroll = self.horizontal_scroll(id);
        if self.session.scroll_cell.get() {
            if let Some(cursor) = self.session.cell.clone().filter(|cursor| cursor.view == id) {
                self.list_scroll(id).scroll_to_item(cursor.row, ScrollStrategy::Center);
                let mut left = 0.;
                for (index, column_width) in widths.iter().enumerate() {
                    if index == cursor.col {
                        let offset = f32::from(h_scroll.offset().x);
                        let view_w = f32::from(h_scroll.bounds().size.width);
                        let next = crate::session::Session::reveal_offset(offset, view_w, left, *column_width);
                        h_scroll.set_offset(point(px(next), h_scroll.offset().y));
                        break;
                    }
                    left += column_width;
                }
            }
            self.session.scroll_cell.set(false);
        }
        let grid_id = id.to_string();
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.view_header(id, theme, cx))
            .child(
                div()
                    .id(SharedString::from(format!("grid-{id}")))
                    .relative()
                    .overflow_hidden()
                    .track_scroll(&h_scroll)
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(header.w(width))
                    .child(body)
                    .child(self.horizontal_wheel(&grid_id, cx)),
            )
            .children(self.horizontal_bar(id, width_px, theme, cx))
            .into_any_element()
    }

    fn horizontal_wheel(&self, id: &str, cx: &mut Context<Self>) -> AnyElement {
        let grid_id = id.to_string();
        let entity = cx.entity();
        canvas(
            |bounds, _, _| bounds,
            move |bounds, _bounds, window, _app| {
                let grid_id = grid_id.clone();
                let entity = entity.clone();
                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, _, app| {
                    if phase != DispatchPhase::Capture || !bounds.contains(&event.position) {
                        return;
                    }
                    let delta = event.delta.pixel_delta(px(20.));
                    let Some(side) = crate::session::sideways_delta(
                        event.modifiers.shift,
                        event.delta.precise(),
                        f32::from(delta.x),
                        f32::from(delta.y),
                    ) else {
                        return;
                    };
                    let _ = entity.update(app, |root, cx| {
                        let handle = root.horizontal_scroll(&grid_id);
                        let offset = handle.offset();
                        handle.set_offset(point(offset.x + px(side), px(0.)));
                        cx.stop_propagation();
                        cx.notify();
                    });
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .into_any_element()
    }

    fn horizontal_bar(&self, id: &str, content: f32, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let handle = self.horizontal_scroll(id);
        let view = f32::from(handle.bounds().size.width);
        let offset = f32::from(handle.offset().x);
        let geom = crate::session::hbar_geom(content, view, offset)?;
        let dragging = self.session.h_scroll_drag.as_ref().is_some_and(|drag| drag.view == id);
        let view_id = id.to_string();
        let origin = f32::from(handle.bounds().origin.x);
        let thumb_x = geom.thumb_x;
        let thumb = geom.thumb;
        let range = geom.range;
        let travel = geom.travel;
        Some(
            div()
                .id(SharedString::from(format!("hbar-{id}")))
                .relative()
                .h(px(12.))
                .w_full()
                .flex_shrink_0()
                .rounded(px(6.))
                .bg(rgb(theme.raised))
                .border_1()
                .border_color(rgb(theme.border))
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, cx.listener({
                    let view_id = view_id.clone();
                    move |this, event: &MouseDownEvent, _, cx| {
                        let local = f32::from(event.position.x) - origin;
                        if local >= thumb_x && local <= thumb_x + thumb {
                            return;
                        }
                        let handle = this.horizontal_scroll(&view_id);
                        let page = (view * 0.9).max(40.);
                        let next = crate::session::hbar_page(offset, page, range, local, thumb_x, thumb);
                        handle.set_offset(point(px(next), px(0.)));
                        cx.stop_propagation();
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .id(SharedString::from(format!("hbar-thumb-{id}")))
                        .absolute()
                        .left(px(thumb_x))
                        .top(px(2.))
                        .w(px(thumb))
                        .h(px(8.))
                        .rounded(px(4.))
                        .bg(rgb(if dragging { theme.accent } else { theme.muted }))
                        .hover(|style| style.bg(rgb(theme.accent)))
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, cx.listener({
                            let view_id = view_id.clone();
                            move |this, event: &MouseDownEvent, _, cx| {
                                this.session.h_scroll_drag = Some(crate::session::HScrollDrag {
                                    view: view_id.clone(),
                                    origin_x: f32::from(event.position.x),
                                    origin_thumb: thumb_x,
                                    travel,
                                    range,
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }
                        })),
                )
                .into_any_element(),
        )
    }

    fn lookup_tracker(&self, _cx: &mut Context<Self>) -> AnyElement {
        let slot = self.session.lookup_anchor.clone();
        canvas(
            move |bounds, window, _| {
                let next = (f32::from(bounds.origin.x), f32::from(bounds.origin.y + bounds.size.height));
                let current = slot.get();
                let changed = current.is_none_or(|(x, y)| (x - next.0).abs() > 0.5 || (y - next.1).abs() > 0.5);
                if changed {
                    slot.set(Some(next));
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .into_any_element()
    }

    pub(crate) fn horizontal_scroll(&self, id: &str) -> gpui::ScrollHandle {
        let mut map = self.h_scroll.borrow_mut();
        map.entry(id.to_string()).or_insert_with(gpui::ScrollHandle::new).clone()
    }

    fn list_scroll(&self, id: &str) -> gpui::UniformListScrollHandle {
        let mut map = self.v_scroll.borrow_mut();
        map.entry(id.to_string()).or_insert_with(gpui::UniformListScrollHandle::new).clone()
    }

    fn table_row(
        &self,
        index: usize,
        view_id: &str,
        entity: &str,
        columns: &[String],
        widths: &[f32],
        row: &Record,
        on: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view_id = view_id.to_string();
        let record_id = row.id.clone();
        let entity_id = entity.to_string();
        let cursor_col = self.session.cell.as_ref().filter(|cursor| cursor.view == view_id && cursor.row == index).map(|cursor| cursor.col);
        let mut line = div()
            .id(SharedString::from(format!("row-{view_id}-{index}")))
            .flex()
            .flex_row()
            .flex_shrink_0()
            .h(px(28.))
            .items_center()
            .cursor_pointer()
            .bg(rgb(if on { theme.accent_dim } else { theme.panel }))
            .text_color(rgb(theme.text))
            .border_l(px(3.))
            .border_color(rgb(if on { theme.accent } else { theme.panel }))
            .hover(|style| style.bg(rgb(if on { theme.accent_dim } else { theme.hover })).text_color(rgb(theme.text)))
            .on_click({
                let view_id = view_id.clone();
                let record_id = record_id.clone();
                cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                    this.session.select(&view_id, &record_id);
                    cx.notify();
                })
            })
            .on_mouse_down(MouseButton::Right, {
                let view_id = view_id.clone();
                let record_id = record_id.clone();
                let entity_id = entity_id.clone();
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.open_row_menu(&view_id, &record_id, &entity_id, event.position);
                    cx.stop_propagation();
                    cx.notify();
                })
            });
        for (col_index, column) in columns.iter().enumerate() {
            let column_id = column.clone();
            let record_for_cell = record_id.clone();
            let view_for_cell = view_id.clone();
            let width = widths.get(col_index).copied().unwrap_or(148.);
            let focused = cursor_col == Some(col_index);
            let editing = self.session.editing.as_ref().filter(|edit| {
                edit.view == view_id && edit.record_id == row.id && edit.field == column.as_str()
            });
            let editor = editing.map(|edit| edit.edit.clone());
            let invalid = editing.and_then(|edit| edit.error.clone());
            line = line.child(
                div()
                    .id(SharedString::from(format!("cell-{view_id}-{index}-{column}")))
                    .w(px(width))
                    .h_full()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .px_2()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(12.))
                    .text_color(rgb(if invalid.is_some() { theme.warn } else { theme.text }))
                    .relative()
                    .border_1()
                    .border_color(rgb(if focused { theme.accent } else { if on { theme.accent_dim } else { theme.panel } }))
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()))
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                        let same = this.session.editing.as_ref().is_some_and(|edit| {
                            edit.view == view_for_cell && edit.record_id == record_for_cell && edit.field == column_id
                        });
                        this.session.select(&view_for_cell, &record_for_cell);
                        if let Some(cursor) = this.session.cell.as_mut() {
                            cursor.col = col_index;
                        }
                        this.session.scroll_cell.set(true);
                        if event.click_count() >= 2 {
                            this.session.begin_edit(&view_for_cell, &record_for_cell, &column_id);
                        } else if !same {
                            this.session.cancel_edit();
                        }
                        cx.notify();
                    }))
                    .child(match editor {
                        Some(edit) => crate::edit::render_edit_line(&edit, "", theme),
                        None => div().child(self.session.cell(entity, column, row)).into_any_element(),
                    })
                    .when(editing.is_some_and(|edit| edit.lookup), |cell| cell.child(self.lookup_tracker(cx))),
            );
        }
        line.into_any_element()
    }

    fn placeholder_row(&self, index: usize, view_id: &str, theme: &Theme) -> AnyElement {
        div()
            .id(SharedString::from(format!("row-{view_id}-{index}")))
            .flex()
            .items_center()
            .h(px(28.))
            .px_2()
            .text_size(px(12.))
            .text_color(rgb(theme.subtle))
            .child("…")
            .into_any_element()
    }

    pub(crate) fn render_chooser(
        &self,
        id: &str,
        entity: &str,
        columns: &[String],
        theme: &Theme,
        viewport: gpui::Size<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let filter = self.session.chooser_filter.text.to_ascii_lowercase();
        let fields = self
            .session
            .backend
            .model()
            .entity(entity)
            .map(|ent| ent.leaves().into_iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let mut list = div().flex().flex_col().gap(px(1.));
        for field in fields {
            if !filter.is_empty()
                && !field.label.to_ascii_lowercase().contains(&filter)
                && !field.id.to_ascii_lowercase().contains(&filter)
            {
                continue;
            }
            let on = columns.iter().any(|column| column == &field.id);
            let view_id = id.to_string();
            let field_id = field.id.clone();
            let section = if field.system { "System" } else { "" };
            list = list.child(
                div()
                    .id(SharedString::from(format!("col-{id}-{}", field.id)))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py(px(3.))
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(theme.hover)))
                    .child(
                        div()
                            .w(px(14.))
                            .text_color(rgb(if on { theme.accent } else { theme.subtle }))
                            .child(if on { "✓" } else { "" }),
                    )
                    .child(field.label.clone())
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(theme.subtle))
                            .child(section),
                    )
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                        this.session.toggle_column(&view_id, &field_id);
                        this.dirty = true;
                        cx.notify();
                    })),
            );
        }
        let max_h = (crate::popover::popover_max_height(f32::from(viewport.height)) - 40.).max(120.);
        let origin = point(viewport.width - px(16.), px(80.));
        crate::popover::anchor_popover(origin, true, div()
            .id(SharedString::from(format!("chooser-{id}")))
            .w(px(280.))
            .max_h(px(max_h.max(120.)))
            .overflow_y_scroll()
            .p_2()
            .rounded_md()
            .bg(rgb(theme.raised))
            .border_1()
            .border_color(rgb(theme.border))
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_size(px(11.)).text_color(rgb(theme.muted)).child("Fields"))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(theme.text))
                    .child(crate::edit::render_edit_line(&self.session.chooser_filter, "Type to filter", theme)),
            )
            .child(list)
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation())),
        )
    }

    fn render_detail(&self, id: &str, entity: &str, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        if let Some(selected) = self.session.file.selections.get(id).cloned() {
            self.session.backend.prepare(entity, &selected);
        }
        let record = self.session.current_record(id);
        let definition = self.session.backend.model().entity(entity).cloned();
        let mut body = div()
            .id(SharedString::from(format!("detail-body-{id}")))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .p_3()
            .gap_1();
        if let (Some(record), Some(definition)) = (record, definition) {
            body = body.child(
                div()
                    .text_size(px(16.))
                    .pb_2()
                    .child(self.session.backend.label(entity, &record.id)),
            );
            for field in &definition.fields {
                if field.system {
                    continue;
                }
                if field.ty == FieldType::Group || field.many {
                    let count = match record.values.get(&field.id).or_else(|| record.values.get(&field.name)) {
                        Some(FieldValue::List(items)) => items.len(),
                        Some(FieldValue::Group(_)) => 1,
                        Some(_) => 1,
                        None => 0,
                    };
                    body = body.child(
                        div().flex().flex_row().gap_3().py(px(3.)).child(
                            div().w(px(180.)).text_color(rgb(theme.muted)).child(field.label.clone()),
                        ).child(div().text_color(rgb(theme.text)).child(format!("{count}"))),
                    );
                } else if field.children.is_empty() {
                    let text = self.session.cell(entity, &field.id, &record);
                    let field_id = field.id.clone();
                    let record_id = record.id.clone();
                    let view_id = id.to_string();
                    let editor = self.session.editing.as_ref().filter(|edit| {
                        edit.view == id && edit.record_id == record.id && edit.field == field.id
                    }).map(|edit| edit.edit.clone());
                    let invalid = self.session.editing.as_ref().and_then(|edit| {
                        (edit.view == id && edit.field == field.id).then(|| edit.error.clone()).flatten()
                    });
                    body = body.child(
                        div()
                            .id(SharedString::from(format!("field-{id}-{}", field.id)))
                            .flex()
                            .flex_row()
                            .gap_3()
                            .py(px(3.))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(theme.hover)))
                            .relative()
                            .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                                this.session.select(&view_id, &record_id);
                                this.session.begin_edit(&view_id, &record_id, &field_id);
                                cx.notify();
                            }))
                            .child(div().w(px(180.)).text_color(rgb(theme.muted)).child(field.label.clone()))
                            .child(
                                div()
                                    .flex_1()
                                    .text_color(rgb(if invalid.is_some() { theme.warn } else { theme.text }))
                                    .child(match editor {
                                        Some(edit) => crate::edit::render_edit_line(&edit, "Empty", theme),
                                        None => div().child(if text.is_empty() { "—".to_string() } else { text }).into_any_element(),
                                    }),
                            )
                            .when(
                                self.session.editing.as_ref().is_some_and(|edit| {
                                    edit.lookup && edit.view == id && edit.record_id == record.id && edit.field == field.id
                                }),
                                |row| row.child(self.lookup_tracker(cx)),
                            ),
                    );
                }
            }
        } else {
            body = body.child(div().text_color(rgb(theme.muted)).child("Nothing selected."));
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.view_header(id, theme, cx))
            .child(body)
            .into_any_element()
    }

    pub(crate) fn render_lookup(
        &self,
        theme: &Theme,
        viewport: gpui::Size<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let highlight = self.session.editing.as_ref().filter(|edit| edit.lookup).map(|edit| edit.choice_at)?;
        let anchor = self.session.lookup_anchor.get()?;
        let rows = self.session.lookup_rows();
        let max_h = (crate::popover::popover_max_height(f32::from(viewport.height)) - 48.).max(120.);
        let mut list = div()
            .id("lookup-list")
            .w(px(240.))
            .max_h(px(max_h))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py_1()
            .rounded_md()
            .bg(rgb(theme.raised))
            .border_1()
            .border_color(rgb(theme.border))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()));
        if rows.is_empty() {
            list = list.child(div().px_3().py_2().text_color(rgb(theme.muted)).child("No matching value"));
        }
        for (index, row) in rows.iter().enumerate() {
            let on = index == highlight;
            let label = row.label.clone();
            let code = row.code.clone();
            let clear = row.clear;
            list = list.child(
                div()
                    .id(SharedString::from(format!("lookup-{index}")))
                    .h(px(28.))
                    .px_2()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .bg(rgb(if on { theme.accent_dim } else { theme.raised }))
                    .border_l(px(3.))
                    .border_color(rgb(if on { theme.accent } else { theme.raised }))
                    .hover(|style| style.bg(rgb(theme.hover)))
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                        this.session.choose_lookup(index);
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(14.))
                            .text_color(rgb(theme.accent))
                            .child(if row.checked { "✓" } else { "" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(if clear { theme.subtle } else { theme.text }))
                            .child(label),
                    )
                    .child(div().text_size(px(11.)).text_color(rgb(theme.subtle)).child(if clear { String::new() } else { code })),
            );
        }
        Some(crate::popover::anchor_popover(point(px(anchor.0), px(anchor.1)), false, list))
    }

    fn render_tree(&self, id: &str, entity: &str, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.session.tree_rows(entity);
        let selected = self.session.file.selections.get(id).cloned();
        let mut body = div()
            .id(SharedString::from(format!("tree-{id}")))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py_1();
        if rows.is_empty() {
            body = body.child(
                div()
                    .p_4()
                    .text_color(rgb(theme.muted))
                    .child("This entity has no hierarchy in the model."),
            );
        }
        for (index, (depth, row)) in rows.iter().enumerate() {
            let on = selected.as_deref() == Some(row.id.as_str());
            let view_id = id.to_string();
            let record_id = row.id.clone();
            let label = self.session.backend.label(entity, &row.id);
            body = body.child(
                div()
                    .id(SharedString::from(format!("node-{id}-{index}")))
                    .h(px(26.))
                    .pl(px(12. + (*depth as f32) * 16.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .bg(rgb(if on { theme.accent_dim } else { theme.panel }))
                    .text_color(rgb(theme.text))
                    .border_l(px(3.))
                    .border_color(rgb(if on { theme.accent } else { theme.panel }))
                    .hover(|style| style.bg(rgb(if on { theme.accent_dim } else { theme.hover })).text_color(rgb(theme.text)))
                    .child(label)
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                        this.session.select(&view_id, &record_id);
                        cx.notify();
                    })),
            );
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.view_header(id, theme, cx))
            .child(body)
            .into_any_element()
    }
}
