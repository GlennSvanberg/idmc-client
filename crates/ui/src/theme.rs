use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    pub fn toggled(self) -> Self {
        match self {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Dark => "Dark",
            ThemeMode::Light => "Light",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: u32,
    pub panel: u32,
    pub raised: u32,
    pub text: u32,
    pub muted: u32,
    pub subtle: u32,
    pub border: u32,
    pub accent: u32,
    pub accent_dim: u32,
    pub on_accent: u32,
    pub hover: u32,
    pub drop: u32,
    pub ok: u32,
    pub warn: u32,
    pub edge: u32,
    pub node_entity: u32,
    pub node_group: u32,
    pub node_child: u32,
    pub node_field: u32,
    pub node_type: u32,
    pub on_node: u32,
}

pub fn theme(mode: ThemeMode) -> Theme {
    match mode {
        ThemeMode::Dark => Theme {
            bg: 0x121418,
            panel: 0x1b1e24,
            raised: 0x262b33,
            text: 0xe7e9ee,
            muted: 0x9aa3b2,
            subtle: 0x6d7686,
            border: 0x343b46,
            accent: 0x47ff9a,
            accent_dim: 0x14663a,
            on_accent: 0x052e16,
            hover: 0x3a4654,
            drop: 0x145c38,
            ok: 0x47ff9a,
            warn: 0xfcd34d,
            edge: 0xd5dde6,
            node_entity: 0x47ff9a,
            node_group: 0x67e8f9,
            node_child: 0x93c5fd,
            node_field: 0xfcd34d,
            node_type: 0xc4b5fd,
            on_node: 0x052e16,
        },
        ThemeMode::Light => Theme {
            bg: 0xf3f5f7,
            panel: 0xffffff,
            raised: 0xe8edf2,
            text: 0x1c2128,
            muted: 0x3e4a57,
            subtle: 0x5c6773,
            border: 0xd5dbe3,
            accent: 0x0c8a48,
            accent_dim: 0x47ff9a,
            on_accent: 0x052e16,
            hover: 0xdff8ea,
            drop: 0xb6ffd6,
            ok: 0x047857,
            warn: 0xb45309,
            edge: 0x3e4a57,
            node_entity: 0x047857,
            node_group: 0x0e7490,
            node_child: 0x1d4ed8,
            node_field: 0xb45309,
            node_type: 0x6d28d9,
            on_node: 0xffffff,
        },
    }
}

/// WCAG contrast ratio. Used to lock the pairs the graph and the selection row depend on.
pub fn contrast(foreground: u32, background: u32) -> f32 {
    fn channel(value: u32) -> f32 {
        let c = value as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    fn lum(hex: u32) -> f32 {
        let r = channel(hex >> 16);
        let g = channel((hex >> 8) & 0xff);
        let b = channel(hex & 0xff);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }
    let a = lum(foreground);
    let b = lum(background);
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_node_fills_meet_wcag_aa() {
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let theme = theme(mode);
            assert!(contrast(theme.text, theme.bg) >= 4.5, "{mode:?} body");
            let selection = contrast(theme.text, theme.accent_dim);
            assert!(selection >= 4.5, "{mode:?} selection {selection:.2}");
            let mint = if mode == ThemeMode::Light { theme.accent_dim } else { theme.accent };
            let mint_ink = contrast(theme.on_accent, mint);
            assert!(mint_ink >= 4.5, "{mode:?} mint {mint_ink:.2}");
            assert!(contrast(theme.on_node, theme.node_entity) >= 4.5, "{mode:?} entity");
            assert!(contrast(theme.on_node, theme.node_group) >= 4.5, "{mode:?} group");
            assert!(contrast(theme.on_node, theme.node_child) >= 4.5, "{mode:?} child");
            assert!(contrast(theme.on_node, theme.node_field) >= 4.5, "{mode:?} field");
            assert!(contrast(theme.on_node, theme.node_type) >= 4.5, "{mode:?} type");
            assert!(contrast(theme.edge, theme.bg) >= 4.5, "{mode:?} edge");
            assert!(contrast(theme.muted, theme.bg) >= 4.5, "{mode:?} muted");
        }
    }
}
