//! Table, detail, and tree views. Every label and column comes from the model.

use gpui::{div, px, prelude::*, rgb, AnyElement, Context, MouseButton, MouseDownEvent, SharedString};
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
        }
    }

    fn view_header(&self, id: &str, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.session.view_title(id);
        let caption = self.session.link_caption(id);
        let kind = self.session.spec(id).map(|spec| spec.kind);
        let count = if kind == Some(ViewKind::Tree) {
            self.session.tree_rows(self.session.spec(id).map(|s| s.entity.as_str()).unwrap_or("")).len()
        } else {
            self.session.rows(id).len()
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
            .when(caption.is_some(), |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(theme.accent))
                        .child(caption.unwrap_or_default()),
                )
            })
            .child(div().flex_1())
            .when(kind == Some(ViewKind::Table), |el| {
                let id = fields_id.clone();
                let button = id.clone();
                el.child(self.header_button(&button, "Columns", theme, cx, move |this, cx| {
                    this.session.file.focus = Some(id.clone());
                    this.exec("view.fields", cx);
                }))
            })
            .child(self.header_button(id, "Link", theme, cx, move |this, cx| {
                this.session.file.focus = Some(menu_id.clone());
                this.exec("view.link", cx);
            }))
    }

    fn header_button(
        &self,
        view: &str,
        label: &'static str,
        theme: &Theme,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        div()
            .id(SharedString::from(format!("hdr-{view}-{label}")))
            .px_2()
            .py(px(2.))
            .rounded_sm()
            .text_size(px(11.))
            .text_color(rgb(theme.muted))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme.hover)).text_color(rgb(theme.text)))
            .child(label)
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
        let rows = self.session.rows(id);
        let selected = self.session.file.selections.get(id).cloned();
        let mut header = div().flex().flex_row().h(px(28.)).bg(rgb(theme.raised)).items_center();
        for column in columns {
            let label = self
                .session
                .backend
                .model()
                .entity(entity)
                .and_then(|ent| ent.field(column))
                .map(|field| field.label.clone())
                .unwrap_or_else(|| column.clone());
            header = header.child(
                div()
                    .w(px(148.))
                    .flex_shrink_0()
                    .px_2()
                    .text_size(px(11.))
                    .text_color(rgb(theme.muted))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(label),
            );
        }
        let mut body = div()
            .id(SharedString::from(format!("rows-{id}")))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if rows.is_empty() {
            body = body.child(
                div()
                    .p_4()
                    .text_color(rgb(theme.muted))
                    .child("No records. Select a source row, or clear the link to see every record."),
            );
        }
        for (index, row) in rows.iter().enumerate() {
            let on = selected.as_deref() == Some(row.id.as_str());
            let view_id = id.to_string();
            let record_id = row.id.clone();
            let mut line = div()
                .id(SharedString::from(format!("row-{id}-{index}")))
                .flex()
                .flex_row()
                .flex_shrink_0()
                .h(px(28.))
                .items_center()
                .cursor_pointer()
                .bg(rgb(if on { theme.accent_dim } else { theme.panel }))
                .hover(|style| style.bg(rgb(if on { theme.accent_dim } else { theme.hover })))
                .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                    this.session.select(&view_id, &record_id);
                    cx.notify();
                }));
            for column in columns {
                line = line.child(
                    div()
                        .w(px(148.))
                        .flex_shrink_0()
                        .px_2()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(px(12.))
                        .child(self.session.cell(entity, column, row)),
                );
            }
            body = body.child(line);
        }
        let chooser = if self.session.chooser_for.as_deref() == Some(id) {
            Some(self.render_chooser(id, entity, columns, theme, cx))
        } else {
            None
        };
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
                    .overflow_x_scroll()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(header)
                    .child(body),
            )
            .children(chooser)
            .into_any_element()
    }

    fn render_chooser(
        &self,
        id: &str,
        entity: &str,
        columns: &[String],
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let filter = self.session.chooser_filter.to_ascii_lowercase();
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
        div()
            .id(SharedString::from(format!("chooser-{id}")))
            .absolute()
            .top(px(40.))
            .right(px(8.))
            .w(px(280.))
            .max_h(px(360.))
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
                    .text_size(px(11.))
                    .text_color(rgb(theme.subtle))
                    .child(if filter.is_empty() {
                        "Type to filter".to_string()
                    } else {
                        format!("Filter: {filter}")
                    }),
            )
            .child(list)
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()))
            .into_any_element()
    }

    fn render_detail(&self, id: &str, entity: &str, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
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
                    body = body.child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_3()
                            .py(px(3.))
                            .child(div().w(px(180.)).text_color(rgb(theme.muted)).child(field.label.clone()))
                            .child(div().flex_1().child(if text.is_empty() { "—".to_string() } else { text })),
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
                    .hover(|style| style.bg(rgb(if on { theme.accent_dim } else { theme.hover })))
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
