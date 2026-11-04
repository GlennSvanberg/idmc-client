//! Caret and selection for the command palette and the field chooser.
//! Offsets are Unicode scalar values, not bytes.

use gpui::{div, prelude::*, px, rgb, AnyElement};
use crate::theme::Theme;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub text: String,
    pub caret: usize,
    pub anchor: Option<usize>,
}

impl TextEdit {
    pub fn new() -> Self {
        Self { text: String::new(), caret: 0, anchor: None }
    }

    pub fn len_chars(&self) -> usize {
        self.text.chars().count()
    }

    pub fn clear(&mut self) {
        *self = Self::new();
    }

    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        let (lo, hi) = if anchor < self.caret { (anchor, self.caret) } else { (self.caret, anchor) };
        if lo == hi { None } else { Some((lo, hi)) }
    }

    pub fn caret_at_selection_start(&self) -> bool {
        match self.selection() {
            Some((lo, _)) => self.caret == lo,
            None => true,
        }
    }

    pub fn parts(&self) -> (String, String, String) {
        let chars: Vec<char> = self.text.chars().collect();
        let (lo, hi) = self.selection().unwrap_or((self.caret, self.caret));
        let lo = lo.min(chars.len());
        let hi = hi.min(chars.len());
        (
            chars[..lo].iter().collect(),
            chars[lo..hi].iter().collect(),
            chars[hi..].iter().collect(),
        )
    }

    fn delete_selection(&mut self) -> bool {
        let Some((lo, hi)) = self.selection() else { return false };
        let start = byte_at(&self.text, lo);
        let end = byte_at(&self.text, hi);
        self.text.replace_range(start..end, "");
        self.caret = lo;
        self.anchor = None;
        true
    }

    pub fn insert(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        self.delete_selection();
        let at = byte_at(&self.text, self.caret);
        self.text.insert(at, ch);
        self.caret += 1;
        self.anchor = None;
    }

    pub fn backspace(&mut self) {
        if self.delete_selection() || self.caret == 0 {
            return;
        }
        let end = byte_at(&self.text, self.caret);
        self.caret -= 1;
        let start = byte_at(&self.text, self.caret);
        self.text.replace_range(start..end, "");
        self.anchor = None;
    }

    pub fn delete_forward(&mut self) {
        if self.delete_selection() || self.caret >= self.len_chars() {
            return;
        }
        let start = byte_at(&self.text, self.caret);
        let end = byte_at(&self.text, self.caret + 1);
        self.text.replace_range(start..end, "");
        self.anchor = None;
    }

    pub fn move_left(&mut self, shift: bool) {
        self.arm_selection(shift);
        self.caret = self.caret.saturating_sub(1);
        if !shift {
            self.anchor = None;
        }
    }

    pub fn move_right(&mut self, shift: bool) {
        self.arm_selection(shift);
        self.caret = (self.caret + 1).min(self.len_chars());
        if !shift {
            self.anchor = None;
        }
    }

    pub fn home(&mut self, shift: bool) {
        self.arm_selection(shift);
        self.caret = 0;
        if !shift {
            self.anchor = None;
        }
    }

    pub fn end(&mut self, shift: bool) {
        self.arm_selection(shift);
        self.caret = self.len_chars();
        if !shift {
            self.anchor = None;
        }
    }

    pub fn select_all(&mut self) {
        if self.text.is_empty() {
            self.anchor = None;
            self.caret = 0;
            return;
        }
        self.anchor = Some(0);
        self.caret = self.len_chars();
    }

    fn arm_selection(&mut self, shift: bool) {
        if shift && self.anchor.is_none() {
            self.anchor = Some(self.caret);
        }
    }
}

impl Default for TextEdit {
    fn default() -> Self { Self::new() }
}

fn byte_at(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map(|(index, _)| index).unwrap_or(text.len())
}

pub fn render_edit_line(edit: &TextEdit, placeholder: &str, theme: &Theme) -> AnyElement {
    let (before, mid, after) = edit.parts();
    let caret_first = edit.caret_at_selection_start();
    let mut row = div().flex().flex_row().items_center().min_h(px(18.));
    if edit.text.is_empty() {
        return row
            .child(caret_bar(theme))
            .child(div().text_color(rgb(theme.subtle)).child(placeholder.to_string()))
            .into_any_element();
    }
    if !before.is_empty() {
        row = row.child(before);
    }
    if caret_first {
        row = row.child(caret_bar(theme));
    }
    if !mid.is_empty() {
        row = row.child(div().bg(rgb(theme.accent_dim)).text_color(rgb(theme.text)).child(mid));
    }
    if !caret_first {
        row = row.child(caret_bar(theme));
    }
    if !after.is_empty() {
        row = row.child(after);
    }
    row.into_any_element()
}

fn caret_bar(theme: &Theme) -> AnyElement {
    div().w(px(1.)).h(px(14.)).bg(rgb(theme.accent)).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_edits_the_middle_and_selects_all() {
        let mut edit = TextEdit::new();
        edit.insert('o');
        edit.insert('p');
        edit.insert('e');
        edit.insert('n');
        edit.home(false);
        edit.move_right(false);
        edit.insert(' ');
        assert_eq!(edit.text, "o pen");
        edit.end(false);
        edit.move_left(true);
        edit.move_left(true);
        edit.backspace();
        assert_eq!(edit.text, "o p");
        edit.select_all();
        edit.insert('a');
        assert_eq!(edit.text, "a");
        edit.home(false);
        edit.delete_forward();
        assert!(edit.text.is_empty());
    }
}
