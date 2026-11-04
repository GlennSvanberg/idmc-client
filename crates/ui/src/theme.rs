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
    pub hover: u32,
    pub drop: u32,
    pub ok: u32,
    pub warn: u32,
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
            accent: 0x3cb4d8,
            accent_dim: 0x1a4552,
            hover: 0x2c333d,
            drop: 0x143844,
            ok: 0x3dd68c,
            warn: 0xe6b450,
        },
        ThemeMode::Light => Theme {
            bg: 0xf3f5f7,
            panel: 0xffffff,
            raised: 0xe8edf2,
            text: 0x1c2128,
            muted: 0x5c6773,
            subtle: 0x8b95a1,
            border: 0xd5dbe3,
            accent: 0x0b7f86,
            accent_dim: 0xd5eef0,
            hover: 0xe7eef4,
            drop: 0xd7f1f4,
            ok: 0x0f7a45,
            warn: 0x8a5a00,
        },
    }
}
