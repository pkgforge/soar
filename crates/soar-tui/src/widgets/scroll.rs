//! Cursor and scrolling math for lists longer than the screen.

use std::ops::Range;

/// How many rows stay visible above and below the cursor when the list is
/// long enough.
const CONTEXT: usize = 2;

/// The cursor and scroll offset of a list longer than the screen.
///
/// Views build their rows only for the window [`ListCursor::window`]
/// returns, so a catalog with many packages costs the same to draw as one
/// that fills the screen.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ListCursor {
    /// The row the cursor sits on.
    pub(crate) selected: usize,
    /// The first row on screen.
    pub(crate) offset: usize,
}

impl ListCursor {
    /// Moves the cursor `n` rows down, stopping at the last row.
    pub(crate) fn down(&mut self, n: usize, len: usize) {
        self.selected = (self.selected + n).min(len.saturating_sub(1));
    }

    /// Moves the cursor `n` rows up, stopping at the first row.
    pub(crate) fn up(&mut self, n: usize) {
        self.selected = self.selected.saturating_sub(n);
    }

    /// Moves the cursor to the first row.
    pub(crate) fn top(&mut self) {
        self.selected = 0;
    }

    /// Moves the cursor to the last row.
    pub(crate) fn bottom(&mut self, len: usize) {
        self.selected = len.saturating_sub(1);
    }

    /// Keeps the cursor inside a list of `len` rows, for when the list
    /// shrank under it.
    pub(crate) fn clamp(&mut self, len: usize) {
        self.selected = self.selected.min(len.saturating_sub(1));
    }

    /// Adjusts `offset` so `selected` stays visible in `height` rows,
    /// keeping two rows of context above and below it when the list is long
    /// enough, and returns the range of rows to draw.
    pub(crate) fn window(&mut self, len: usize, height: usize) -> Range<usize> {
        if len == 0 || height == 0 {
            self.offset = 0;
            return 0..0;
        }
        let height = height.min(len);
        let context = CONTEXT.min((height - 1) / 2);
        let last_offset = len - height;
        self.offset = self.offset.min(last_offset);
        if self.selected + context >= self.offset + height {
            self.offset = (self.selected + context + 1)
                .saturating_sub(height)
                .min(last_offset);
        }
        if self.selected < self.offset + context {
            self.offset = self.selected.saturating_sub(context);
        }
        self.offset..self.offset + height
    }
}

#[cfg(test)]
mod tests {
    use super::ListCursor;

    #[test]
    fn moving_down_past_the_end_clamps_to_the_last_row() {
        let mut cursor = ListCursor::default();
        cursor.down(10, 3);
        assert_eq!(cursor.selected, 2);
        cursor.down(5, 0);
        assert_eq!(cursor.selected, 0);
    }

    #[test]
    fn moving_up_past_the_start_clamps_to_the_first_row() {
        let mut cursor = ListCursor::default();
        cursor.down(2, 3);
        cursor.up(5);
        assert_eq!(cursor.selected, 0);
    }

    #[test]
    fn the_jump_keys_go_to_the_ends() {
        let mut cursor = ListCursor::default();
        cursor.bottom(4);
        assert_eq!(cursor.selected, 3);
        cursor.bottom(0);
        assert_eq!(cursor.selected, 0);
        cursor.top();
        assert_eq!(cursor.selected, 0);
    }

    #[test]
    fn clamping_keeps_the_cursor_inside_a_shrunk_list() {
        let mut cursor = ListCursor::default();
        cursor.down(5, 100);
        cursor.clamp(3);
        assert_eq!(cursor.selected, 2);
        cursor.clamp(0);
        assert_eq!(cursor.selected, 0);
    }

    #[test]
    fn the_window_keeps_the_cursor_visible() {
        let mut cursor = ListCursor::default();
        cursor.down(50, 100);
        let range = cursor.window(100, 10);
        assert!(range.contains(&50), "window is {range:?}");
    }

    #[test]
    fn the_window_keeps_two_rows_of_context_around_the_cursor() {
        let mut cursor = ListCursor::default();
        cursor.down(50, 100);
        assert_eq!(cursor.window(100, 10), 43..53);
    }

    #[test]
    fn the_window_stays_put_while_the_cursor_has_context() {
        let mut cursor = ListCursor::default();
        cursor.down(50, 100);
        let first = cursor.window(100, 10);
        let second = cursor.window(100, 10);
        assert_eq!(first, second);
        assert_eq!(cursor.selected, 50);
    }

    #[test]
    fn the_window_sticks_to_the_top_while_the_cursor_is_near_it() {
        let mut cursor = ListCursor::default();
        cursor.down(3, 100);
        assert_eq!(cursor.window(100, 10), 0..10);
    }

    #[test]
    fn the_window_reaches_the_end_with_the_cursor_on_the_last_row() {
        let mut cursor = ListCursor::default();
        cursor.bottom(100);
        assert_eq!(cursor.window(100, 10), 90..100);
    }

    #[test]
    fn a_short_list_fills_the_whole_window() {
        let mut cursor = ListCursor::default();
        cursor.down(2, 3);
        assert_eq!(cursor.window(3, 10), 0..3);
    }

    #[test]
    fn an_empty_list_yields_an_empty_window_without_panicking() {
        let mut cursor = ListCursor::default();
        cursor.bottom(0);
        assert_eq!(cursor.window(0, 10), 0..0);
        assert_eq!(cursor.window(0, 0), 0..0);
        assert_eq!(cursor.offset, 0);
    }

    #[test]
    fn a_zero_height_window_yields_an_empty_window() {
        let mut cursor = ListCursor::default();
        cursor.down(4, 100);
        assert_eq!(cursor.window(100, 0), 0..0);
    }
}
