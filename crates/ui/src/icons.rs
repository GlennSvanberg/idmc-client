//! Lucide icons (ISC, https://github.com/lucide-icons/lucide) embedded as SVG
//! assets. Strokes are black so GPUI can tint the alpha mask with the text color.

use std::borrow::Cow;

use gpui::{
    div, px, prelude::*, rgb, svg, AnyView, App, AssetSource, Context, Render, Result, SharedString,
    Window,
};

use crate::theme::Theme;

pub struct IconAssets;

impl AssetSource for IconAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(svg_for(path).map(|text| Cow::Borrowed(text.as_bytes())))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

pub fn icon(name: &str) -> gpui::Svg {
    svg().path(SharedString::from(format!("icons/{name}.svg")))
}

pub fn tooltip(text: impl Into<SharedString>, theme: Theme) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    let bg = theme.raised;
    let fg = theme.text;
    let border = theme.border;
    move |_window, cx| {
        let text = text.clone();
        cx.new(|_| Tip { text, bg, fg, border }).into()
    }
}

struct Tip {
    text: SharedString,
    bg: u32,
    fg: u32,
    border: u32,
}

impl Render for Tip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py(px(2.))
            .rounded_sm()
            .bg(rgb(self.bg))
            .text_color(rgb(self.fg))
            .border_1()
            .border_color(rgb(self.border))
            .text_size(px(12.))
            .child(self.text.clone())
    }
}

fn svg_for(path: &str) -> Option<&'static str> {
    let name = path.trim_start_matches("icons/").trim_end_matches(".svg");
    Some(match name {
        "x" => X,
        "plus" => PLUS,
        "ellipsis" => ELLIPSIS,
        "maximize" => MAXIMIZE,
        "minimize" => MINIMIZE,
        "filter" => FILTER,
        "columns" => COLUMNS,
        "link" => LINK,
        "graph" => GRAPH,
        "fit" => FIT,
        "expand" => EXPAND,
        "open" => OPEN,
        "unpin" => UNPIN,
        _ => return None,
    })
}

const X: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="M6 6l12 12"/></svg>"##;
const PLUS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14"/><path d="M12 5v14"/></svg>"##;
const ELLIPSIS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="#000" stroke="none"><circle cx="5" cy="12" r="1.6"/><circle cx="12" cy="12" r="1.6"/><circle cx="19" cy="12" r="1.6"/></svg>"##;
const MAXIMIZE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" x2="14" y1="3" y2="10"/><line x1="3" x2="10" y1="21" y2="14"/></svg>"##;
const MINIMIZE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 14 10 14 10 20"/><polyline points="20 10 14 10 14 4"/><line x1="14" x2="21" y1="10" y2="3"/><line x1="3" x2="10" y1="21" y2="14"/></svg>"##;
const FILTER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3"/></svg>"##;
const COLUMNS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/><path d="M15 3v18"/></svg>"##;
const LINK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>"##;
const GRAPH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="3"/><circle cx="18" cy="6" r="3"/><circle cx="12" cy="18" r="3"/><path d="M18 9v1a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V9"/><path d="M12 12v3"/></svg>"##;
const FIT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/><path d="M7 21H5a2 2 0 0 1-2-2v-2"/></svg>"##;
const EXPAND: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 15 6 6"/><path d="m15 9 6-6"/><path d="M21 16v5h-5"/><path d="M21 8V3h-5"/><path d="M3 16v5h5"/><path d="m3 21 6-6"/><path d="M3 8V3h5"/><path d="M9 9 3 3"/></svg>"##;
const OPEN: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/></svg>"##;
const UNPIN: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1z"/><path d="m2 2 20 20"/></svg>"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_an_svg() {
        for name in ["x", "plus", "ellipsis", "maximize", "minimize", "filter", "columns", "link", "graph", "fit", "expand", "open", "unpin"] {
            let bytes = IconAssets.load(&format!("icons/{name}.svg")).unwrap().unwrap();
            let text = std::str::from_utf8(&bytes).unwrap();
            assert!(text.contains("<svg"), "{name}");
            assert!(text.contains("viewBox"), "{name}");
        }
        assert!(IconAssets.load("icons/missing.svg").unwrap().is_none());
    }
}
