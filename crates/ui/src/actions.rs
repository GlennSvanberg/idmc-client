//! Every command the client can run. The palette is this list, filtered.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    pub id: &'static str,
    pub title: &'static str,
    pub shortcut: Option<&'static str>,
}

pub static ACTIONS: &[Action] = &[
    Action { id: "palette.open", title: "Command palette", shortcut: Some("ctrl-k") },
    Action { id: "theme.toggle", title: "Toggle light/dark theme", shortcut: Some("ctrl-shift-l") },
    Action { id: "workspace.save", title: "Save workspace", shortcut: Some("ctrl-s") },
    Action { id: "workspace.reload", title: "Reload saved workspace", shortcut: None },
    Action { id: "workspace.reset", title: "Reset workspace to the model default", shortcut: None },
    Action { id: "workspace.new", title: "New workspace from template", shortcut: None },
    Action { id: "workspace.duplicate", title: "Duplicate workspace", shortcut: None },
    Action { id: "workspace.rename", title: "Rename workspace", shortcut: None },
    Action { id: "workspace.close", title: "Close workspace", shortcut: None },
    Action { id: "record.search", title: "Search records", shortcut: None },
    Action { id: "view.open_graph", title: "Open Graph view", shortcut: None },
    Action { id: "record.show_in_graph", title: "Show in graph", shortcut: None },
    Action { id: "data.reload", title: "Reload records", shortcut: None },
    Action { id: "app.quit", title: "Quit", shortcut: Some("ctrl-q") },
    Action { id: "view.close", title: "Close focused view", shortcut: Some("ctrl-w") },
    Action { id: "view.zoom", title: "Maximize or restore focused view", shortcut: Some("ctrl-alt-z") },
    Action { id: "view.focus_next", title: "Focus next view", shortcut: Some("ctrl-pagedown") },
    Action { id: "view.focus_prev", title: "Focus previous view", shortcut: Some("ctrl-pageup") },
    Action { id: "view.move_left", title: "Move focused view left", shortcut: Some("ctrl-alt-left") },
    Action { id: "view.move_right", title: "Move focused view right", shortcut: Some("ctrl-alt-right") },
    Action { id: "view.move_up", title: "Move focused view up", shortcut: Some("ctrl-alt-up") },
    Action { id: "view.move_down", title: "Move focused view down", shortcut: Some("ctrl-alt-down") },
    Action { id: "view.new_table", title: "New table view", shortcut: None },
    Action { id: "view.new_detail", title: "New detail view", shortcut: None },
    Action { id: "view.new_tree", title: "New tree view", shortcut: None },
    Action { id: "view.open", title: "Open view", shortcut: None },
    Action { id: "view.fields", title: "Toggle field chooser", shortcut: Some("ctrl-alt-c") },
    Action { id: "view.link", title: "Choose source link", shortcut: Some("ctrl-alt-l") },
    Action { id: "view.unlink", title: "Clear source link", shortcut: None },
    Action { id: "selection.next", title: "Select next row", shortcut: Some("down") },
    Action { id: "selection.prev", title: "Select previous row", shortcut: Some("up") },
    Action { id: "record.new", title: "New record", shortcut: None },
    Action { id: "record.new_child", title: "New child row", shortcut: None },
    Action { id: "record.save", title: "Save record changes", shortcut: None },
    Action { id: "record.revert", title: "Revert record changes", shortcut: None },
    Action { id: "record.delete", title: "Delete record", shortcut: None },
];

pub fn registry() -> &'static [Action] {
    ACTIONS
}

pub fn palette_actions(query: &str) -> Vec<&'static Action> {
    let q: String = query.chars().filter(|ch| !ch.is_whitespace()).flat_map(|ch| ch.to_lowercase()).collect();
    if q.is_empty() {
        return registry().iter().collect();
    }
    let mut scored: Vec<(usize, &Action)> = registry()
        .iter()
        .filter_map(|action| {
            let title = action.title.to_ascii_lowercase();
            let id = action.id.to_ascii_lowercase();
            let shortcut = action.shortcut.map(|item| shortcut_label(item).to_ascii_lowercase()).unwrap_or_default();
            let score = fuzzy_score(&q, &title).or_else(|| fuzzy_score(&q, &id)).or_else(|| fuzzy_score(&q, &shortcut))?;
            Some((score, action))
        })
        .collect();
    scored.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.id.cmp(right.1.id)));
    scored.into_iter().map(|(_, action)| action).collect()
}

pub fn fuzzy_match(query: &str, text: &str) -> bool {
    let q: String = query.chars().filter(|ch| !ch.is_whitespace()).flat_map(|ch| ch.to_lowercase()).collect();
    if q.is_empty() {
        return true;
    }
    fuzzy_score(&q, &text.to_ascii_lowercase()).is_some()
}

fn fuzzy_score(query: &str, text: &str) -> Option<usize> {
    let mut score = 0;
    let mut from = 0;
    for ch in query.chars() {
        let rest = text.get(from..)?;
        let pos = rest.find(ch)?;
        score += pos;
        from += pos + ch.len_utf8();
    }
    Some(score)
}

pub fn shortcut_label(shortcut: &str) -> String {
    shortcut
        .split('-')
        .map(|part| match part {
            "ctrl" => "Ctrl".to_string(),
            "shift" => "Shift".to_string(),
            "alt" => "Alt".to_string(),
            "pageup" => "PageUp".to_string(),
            "pagedown" => "PageDown".to_string(),
            other => {
                let mut chars = other.chars();
                match chars.next() {
                    Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn palette_covers_every_registered_action() {
        let shown = palette_actions("");
        let shown_ids: BTreeSet<_> = shown.iter().map(|a| a.id).collect();
        for action in registry() {
            assert!(
                shown_ids.contains(action.id),
                "palette is missing {}",
                action.id
            );
        }
        assert_eq!(shown_ids.len(), registry().len());
        let filtered = palette_actions("workspace");
        assert!(filtered.iter().all(|a| registry().iter().any(|r| r.id == a.id)));
        assert!(filtered.len() < registry().len());
        assert!(palette_actions("zzzz-not-a-command").is_empty());
    }

    #[test]
    fn shortcuts_are_unique() {
        let mut seen = BTreeSet::new();
        for action in registry() {
            if let Some(shortcut) = action.shortcut {
                assert!(seen.insert(shortcut), "duplicate shortcut {shortcut}");
                assert!(shortcut
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit()));
            }
        }
    }
}
