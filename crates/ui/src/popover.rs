//! Popovers stay inside the window. GPUI's anchored element snaps to the
//! window edges; callers cap the height so a tall menu scrolls instead of
//! painting past the bottom.

use gpui::{anchored, px, prelude::*, AnyElement, AnchoredPositionMode, Corner, IntoElement, Pixels, Point};

pub fn menu_corner(grow_left: bool) -> Corner {
    if grow_left { Corner::TopRight } else { Corner::TopLeft }
}

/// Room under a small margin. A menu taller than this scrolls.
pub fn popover_max_height(viewport_px: f32) -> f32 {
    (viewport_px - 16.).max(120.)
}

pub fn anchor_popover(origin: Point<Pixels>, grow_left: bool, body: impl IntoElement) -> AnyElement {
    anchored()
        .snap_to_window_with_margin(px(8.))
        .anchor(menu_corner(grow_left))
        .position(origin)
        .position_mode(AnchoredPositionMode::Window)
        .child(body)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menus_flip_toward_free_space_and_keep_a_scroll_budget() {
        assert_eq!(menu_corner(true), Corner::TopRight);
        assert_eq!(menu_corner(false), Corner::TopLeft);
        assert_eq!(popover_max_height(900.), 884.);
        assert_eq!(popover_max_height(80.), 120.);
    }
}
