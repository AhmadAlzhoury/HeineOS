/*
 * Driver to emulate a simple terminal for text output on a framebuffer.
 * This module also implements the `print!()` and `println!()` macros for formatted output.
 *
 * Author: Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-07
 * License: GPLv3
 */

use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::fmt::Write;
use crate::device::framebuffer;
use crate::device::framebuffer::Framebuffer;
use crate::library::once::Once;
use crate::library::spinlock::Spinlock;

/// Global terminal instance protected by a spinlock.
/// This instance is initialized once during kernel startup.
/// After initialization, it can be accessed via the `terminal()` function.
static TERMINAL: Once<Spinlock<Terminal>> = Once::new();

/// Initialize the global terminal instance with the given framebuffer.
/// This function should be called once during kernel startup.
/// After calling this function, the terminal can be accessed via the `terminal()` function.
pub fn init_terminal(framebuffer: Framebuffer) {
    TERMINAL.init(|| Spinlock::new(Terminal::new(framebuffer)));
}

/// Get a reference to the global terminal instance.
/// This function panics if the terminal has not been initialized yet.
pub fn terminal() -> &'static Spinlock<Terminal> {
    TERMINAL.get().expect("Terminal not initialized")
}

/// Get access to the framebuffer used by the global terminal instance.
/// This way, other parts of the kernel can access the framebuffer for graphics output.
/// The framebuffer is protected by a spinlock, so the caller must lock it before use.
/// While the framebuffer is locked, no other thread can access it, and the terminal is effectively paused.
/// This function panics if the terminal has not been initialized yet.
pub fn framebuffer() -> &'static Spinlock<Framebuffer> {
    // This looks unsafe, but is actually safe because the `framebuffer()` method
    // does not mutate the terminal instance, and the framebuffer is protected by a spinlock.
    unsafe { terminal().inner().framebuffer() }
}

/// Default text foreground color.
const DEFAULT_FG_COLOR: u32 = framebuffer::GREEN;
/// Default text background color.
const DEFAULT_BG_COLOR: u32 = framebuffer::BLACK;

/// Number of lines that are kept after they have scrolled off the top of the screen.
///
/// The scrollback is bounded on purpose: terminal output would otherwise grow
/// until the kernel heap is exhausted. With 256 lines the user can look back
/// about five screens, which costs roughly 190 KiB of the 16 MiB heap.
const SCROLLBACK_LINES: usize = 256;

/// The text the terminal has written, used to redraw the screen when scrolling back.
///
/// The framebuffer only stores pixels, so it cannot tell which characters used to
/// be on a line that has scrolled away. This struct therefore keeps a copy of the
/// text as a character matrix, organized as one ring buffer of lines:
///
///   - the first `history_lines` lines have already scrolled off the top
///   - the following `rows` lines are the ones currently visible on the screen
///
/// Mirroring the screen cell by cell (instead of storing logical lines) means that
/// `set_pos()` keeps working: a write simply lands in a different cell of the matrix.
///
/// All lines are allocated once in `new()` and are reused afterwards, so printing
/// never allocates memory. This is important, because printing happens while the
/// terminal lock is held: an allocation at that point would take the terminal lock
/// and the allocator lock in this order, while `dump_free_list()` takes them in the
/// opposite order. Reusing the lines avoids that lock order inversion completely.
struct Scrollback {
    /// Ring buffer of stored lines. Its length is `SCROLLBACK_LINES + rows`, so it
    /// holds the visible screen plus the maximum number of lines to look back at.
    lines: Vec<Vec<char>>,
    /// Index of the oldest stored line inside `lines`.
    start: usize,
    /// Number of lines that have already scrolled off the top of the screen.
    history_lines: usize,
    /// Number of lines that are visible on the screen.
    rows: usize,
    /// How many lines the viewport is scrolled up from the newest output.
    /// Zero means that the screen shows the current output.
    view_offset: usize,
}

impl Scrollback {
    /// Create an empty scrollback for a terminal of the given size.
    /// This is the only place where memory for the scrollback is allocated.
    fn new(cols: usize, rows: usize) -> Scrollback {
        Scrollback {
            lines: vec![vec![' '; cols]; SCROLLBACK_LINES + rows],
            start: 0,
            history_lines: 0,
            rows,
            view_offset: 0,
        }
    }

    /// Translate a line number counted from the oldest stored line into a ring index.
    fn ring_index(&self, line: usize) -> usize {
        (self.start + line) % self.lines.len()
    }

    /// Remember a character that has been drawn at the given screen position.
    fn write(&mut self, pos: (usize, usize), c: char) {
        let index = self.ring_index(self.history_lines + pos.1);

        if let Some(cell) = self.lines[index].get_mut(pos.0) {
            *cell = c;
        }
    }

    /// Record that the terminal has scrolled its screen up by one line.
    ///
    /// The topmost line of the screen becomes the newest history line, and the line
    /// that appears at the bottom is blanked. No line is allocated or freed here:
    /// once the history is full, the oldest line is reused as the new bottom line.
    fn scroll_up(&mut self) {
        if self.history_lines < SCROLLBACK_LINES {
            // The window of visible lines simply moves one line further down.
            self.history_lines += 1;
        } else {
            // The history is full, so the oldest line is dropped.
            self.start = self.ring_index(1);
        }

        let bottom = self.ring_index(self.history_lines + self.rows - 1);
        self.lines[bottom].fill(' ');
    }

    /// Forget all stored text and return to the newest position.
    fn clear(&mut self) {
        self.history_lines = 0;
        self.view_offset = 0;

        for row in 0..self.rows {
            let index = self.ring_index(row);
            self.lines[index].fill(' ');
        }
    }

    /// Get the line that is shown in the given screen row for the current viewport.
    ///
    /// History and screen together form one continuous list of lines. The viewport
    /// shows a window of `rows` lines, moved up by `view_offset` lines.
    fn visible_line(&self, row: usize) -> &[char] {
        let index = self.ring_index(self.history_lines + row - self.view_offset);
        &self.lines[index]
    }

    /// The largest offset the viewport can be moved up by.
    fn max_offset(&self) -> usize {
        self.history_lines
    }
}

/// A simple terminal driver for text output on a framebuffer.
/// It supports basic character output and a static cursor.
/// The terminal takes ownership of the framebuffer.
pub struct Terminal {
    /// Number of columns in the terminal (based on framebuffer width and font width).
    cols: usize,
    /// Number of rows in the terminal (based on framebuffer height and font height).
    rows: usize,
    /// Current cursor position (column, row) where the next character will be printed.
    pos: (usize, usize),
    /// The framebuffer used for rendering text.
    /// Protected by a spinlock to allow safe concurrent access.
    /// This allows safe access to the framebuffer from multiple threads via the `framebuffer()` method.
    framebuffer: Spinlock<Framebuffer>,
    /// The stored text used for scrolling back through older output.
    ///
    /// This is an `Option` because the terminal is created before the kernel heap
    /// exists (see `boot.rs`), so it cannot allocate the buffers in `new()`.
    /// While it is `None`, all scrollback operations are silently ignored.
    scrollback: Option<Scrollback>,
}

impl Terminal {
    /// Create a new Terminal instance with the given framebuffer.
    /// The terminal calculates its size based on the framebuffer dimensions and the font size.
    pub fn new(mut framebuffer: Framebuffer) -> Terminal {
        let cols = framebuffer.width / framebuffer::CHAR_WIDTH;
        let rows = framebuffer.height / framebuffer::CHAR_HEIGHT;

        framebuffer.clear();

        Terminal {
            cols,
            rows,
            pos: (0, 0),
            framebuffer: Spinlock::new(framebuffer),
            scrollback: None,
        }
    }

    /// Start recording terminal output for the scrollback.
    ///
    /// This has to be called once after the kernel heap has been initialized,
    /// because the scrollback buffers are allocated on the heap. Output that was
    /// printed before this call is not recorded.
    pub fn enable_scrollback(&mut self) {
        if self.scrollback.is_none() {
            self.scrollback = Some(Scrollback::new(self.cols, self.rows));
        }
    }

    /// Get direct access to the framebuffer.
    /// The framebuffer is protected by a spinlock, so the caller must lock it before use.
    /// While the framebuffer is locked, no other thread can access it, and the terminal is effectively paused.
    pub fn framebuffer(&self) -> &Spinlock<Framebuffer> {
        &self.framebuffer
    }

    /// Get the current cursor position (column, row).
    pub fn pos(&self) -> (usize, usize) {
        self.pos
    }

    /// Get the size of the terminal as (columns, rows).
    pub fn size(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    /// Set the cursor position to the given position.
    /// If the position is out of bounds, it is clamped to the terminal size.
    pub fn set_pos(&mut self, col: usize, row: usize) {
        let new_pos = (
            col.min(self.cols.saturating_sub(1)),
            row.min(self.rows.saturating_sub(1)),
        );
        let mut framebuffer = self.framebuffer.lock();

        Self::snap_to_bottom(&mut self.scrollback, &mut framebuffer);
        Self::clear_cursor(self.pos, &mut framebuffer);
        self.pos = new_pos;
        Self::draw_cursor(self.pos, &mut framebuffer);
    }

    /// Clear the terminal screen and reset the cursor position to the top-left corner.
    /// The scrollback is discarded as well, so `clear` really removes all output.
    pub fn clear(&mut self) {
        let mut framebuffer = self.framebuffer.lock();
        framebuffer.clear();
        self.pos = (0, 0);

        if let Some(scrollback) = self.scrollback.as_mut() {
            scrollback.clear();
        }

        Self::draw_cursor(self.pos, &mut framebuffer);
    }

    /// Move the viewport towards older output by the given number of lines.
    ///
    /// Scrolling stops at the oldest line that is still stored, so calling this
    /// repeatedly is safe. Without a scrollback buffer, nothing happens.
    pub fn scroll_view_up(&mut self, lines: usize) {
        let target = {
            let Some(scrollback) = self.scrollback.as_ref() else {
                return;
            };
            (scrollback.view_offset + lines).min(scrollback.max_offset())
        };

        self.set_view_offset(target);
    }

    /// Move the viewport towards newer output by the given number of lines.
    ///
    /// Scrolling stops at the newest output, so calling this repeatedly is safe.
    pub fn scroll_view_down(&mut self, lines: usize) {
        let target = {
            let Some(scrollback) = self.scrollback.as_ref() else {
                return;
            };
            scrollback.view_offset.saturating_sub(lines)
        };

        self.set_view_offset(target);
    }

    /// Show the viewport at the given offset and redraw the screen.
    fn set_view_offset(&mut self, offset: usize) {
        let Some(scrollback) = self.scrollback.as_mut() else {
            return;
        };

        if scrollback.view_offset == offset {
            return;
        }
        scrollback.view_offset = offset;

        let mut framebuffer = self.framebuffer.lock();
        Self::render_viewport(scrollback, &mut framebuffer);

        // The cursor marks the position of the next character, which only exists
        // at the newest output. While looking at older lines, it stays hidden.
        if offset == 0 {
            Self::draw_cursor(self.pos, &mut framebuffer);
        }
    }

    /// Return the viewport to the newest output if the user has scrolled up.
    ///
    /// New output is always shown, so the user does not type into a screen that
    /// displays something else. The caller has to hold the framebuffer lock.
    fn snap_to_bottom(scrollback: &mut Option<Scrollback>, framebuffer: &mut Framebuffer) {
        let Some(scrollback) = scrollback.as_mut() else {
            return;
        };

        if scrollback.view_offset != 0 {
            scrollback.view_offset = 0;
            Self::render_viewport(scrollback, framebuffer);
        }
    }

    /// Draw all lines of the current viewport onto the framebuffer.
    fn render_viewport(scrollback: &Scrollback, framebuffer: &mut Framebuffer) {
        for row in 0..scrollback.rows {
            let y = row * framebuffer::CHAR_HEIGHT;

            for (column, character) in scrollback.visible_line(row).iter().enumerate() {
                let x = column * framebuffer::CHAR_WIDTH;
                framebuffer.draw_char(*character, x, y, DEFAULT_FG_COLOR, DEFAULT_BG_COLOR);
            }
        }
    }

    /// Move the cursor one position back and erase the character at that position.
    ///
    /// If the cursor is at the beginning of a line, it wraps to the end of the previous line.
    /// At the very beginning of the terminal (0, 0), nothing happens.
    ///
    /// This is a generic terminal operation. It does not know anything about prompts,
    /// so protecting a prompt from being erased is the responsibility of the caller.
    pub fn backspace(&mut self) {
        self.erase(1);
    }

    /// Erase the last `count` characters before the cursor.
    ///
    /// This is the multi character version of `backspace()`. It is used to replace the
    /// text that is currently being edited, for example when the shell shows a different
    /// command from its history. Erasing stops at the top-left corner of the terminal.
    ///
    /// Like `backspace()`, this is a generic terminal operation without any knowledge
    /// about prompts. The caller decides how many characters may be erased.
    pub fn erase(&mut self, count: usize) {
        if count == 0 {
            return;
        }

        // The framebuffer is locked only once for the whole operation, because `set_pos()`
        // would lock it again and the spinlock is not reentrant.
        let mut framebuffer = self.framebuffer.lock();
        Self::snap_to_bottom(&mut self.scrollback, &mut framebuffer);
        Self::clear_cursor(self.pos, &mut framebuffer);

        // `&mut self.pos` borrows a single field, so it does not conflict with the
        // framebuffer guard that borrows `self.framebuffer`.
        for _ in 0..count {
            Self::erase_previous_char(&mut self.pos, self.cols, &mut framebuffer);

            // The character is gone from the screen, so the scrollback must not
            // show it again when the user scrolls back to this line.
            if let Some(scrollback) = self.scrollback.as_mut() {
                scrollback.write(self.pos, ' ');
            }
        }

        Self::draw_cursor(self.pos, &mut framebuffer);
    }

    /// Move the cursor one position back and clear the character at that position.
    ///
    /// If the cursor is at the beginning of a line, it wraps to the end of the previous line.
    /// At the very beginning of the terminal (0, 0), nothing happens.
    /// The caller has to hold the framebuffer lock and to handle the cursor.
    ///
    /// This is an associated function instead of a method, because the caller already
    /// holds the framebuffer guard. Taking `&mut self` here would borrow the whole
    /// terminal a second time while that guard is still alive.
    fn erase_previous_char(pos: &mut (usize, usize), cols: usize, framebuffer: &mut Framebuffer) {
        if pos.0 > 0 {
            pos.0 -= 1;
        } else if pos.1 > 0 {
            pos.1 -= 1;
            pos.0 = cols.saturating_sub(1);
        } else {
            // Already at the top-left corner, there is nothing to erase.
            return;
        }

        let x = pos.0 * framebuffer::CHAR_WIDTH;
        let y = pos.1 * framebuffer::CHAR_HEIGHT;
        framebuffer.draw_char(' ', x, y, DEFAULT_BG_COLOR, DEFAULT_BG_COLOR);
    }

    /// Draw a character with the default colors at the current cursor position and advance the cursor.
    /// A newline character moves the cursor to the beginning of the next line.
    /// If the cursor reaches the end of the terminal, the screen scrolls up.
    pub fn put_char(&mut self, c: char) {
        self.put_char_colored(c, DEFAULT_FG_COLOR, DEFAULT_BG_COLOR);
    }

    /// Draw a colored character at the current cursor position and advance the cursor.
    /// A newline character moves the cursor to the beginning of the next line.
    /// If the cursor reaches the end of the terminal, the screen scrolls up using `framebuffer::scroll_up()`
    pub fn put_char_colored(&mut self, c: char, fg_color: u32, bg_color: u32) {
        let mut framebuffer = self.framebuffer.lock();

        // New output is always shown, even if the user is currently looking at
        // older lines. The output itself is recorded in the scrollback below.
        Self::snap_to_bottom(&mut self.scrollback, &mut framebuffer);
        Self::clear_cursor(self.pos, &mut framebuffer);

        if c == '\n' {
            self.pos.0 = 0;
            self.pos.1 += 1;
        } else {
            let x = self.pos.0 * framebuffer::CHAR_WIDTH;
            let y = self.pos.1 * framebuffer::CHAR_HEIGHT;
            framebuffer.draw_char(c, x, y, fg_color, bg_color);

            if let Some(scrollback) = self.scrollback.as_mut() {
                scrollback.write(self.pos, c);
            }

            self.pos.0 += 1;
            if self.pos.0 >= self.cols {
                self.pos.0 = 0;
                self.pos.1 += 1;
            }
        }

        if self.pos.1 >= self.rows {
            framebuffer.scroll_up(framebuffer::CHAR_HEIGHT);
            self.pos.1 = self.rows.saturating_sub(1);

            // The framebuffer has lost the topmost line, so the scrollback keeps it.
            if let Some(scrollback) = self.scrollback.as_mut() {
                scrollback.scroll_up();
            }
        }

        Self::draw_cursor(self.pos, &mut framebuffer);
    }

    /// Draw the cursor at the given position by drawing a white space character in the default foreground color.
    fn draw_cursor(pos: (usize, usize), framebuffer: &mut Framebuffer) {
        let x = pos.0 * framebuffer::CHAR_WIDTH;
        let y = pos.1 * framebuffer::CHAR_HEIGHT;

        framebuffer.draw_char(' ', x, y, DEFAULT_FG_COLOR, DEFAULT_FG_COLOR);
    }

    /// Clear the cursor at the given position by drawing a white space character in the default background color.
    fn clear_cursor(pos: (usize, usize), framebuffer: &mut Framebuffer) {
        let x = pos.0 * framebuffer::CHAR_WIDTH;
        let y = pos.1 * framebuffer::CHAR_HEIGHT;

        framebuffer.draw_char(' ', x, y, DEFAULT_BG_COLOR, DEFAULT_BG_COLOR);
    }
}

// Implement the `fmt::Write` trait for the Terminal to support formatted output.
// We only need to implement the `write_str()` method, which writes a string to the terminal.
// This allows formatted output via the `write_fmt()` method provided by the `fmt::Write` trait.
impl Write for Terminal {
    /// Write a string to the COM port by iterating over each byte in the string and writing it using `put_char()`.
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.put_char(c);
        }

        Ok(())
    }
}

#[macro_export]
/// Print a formatted string to the CGA text buffer.
/// This macro locks the CGA instance and writes the formatted string to it.
macro_rules! print {
    ($($arg:tt)*) => ({
        let mut terminal = $crate::device::terminal::terminal().lock();
        $crate::terminal::print(&mut terminal, format_args!($($arg)*));
    });
}

#[macro_export]
/// Print a formatted string to the CGA text buffer with a newline.
/// This is a convenience macro, wrapping the `print!` macro to add a newline at the end.
/// This macro locks the CGA instance and writes the formatted string to it.
macro_rules! println {
    ($fmt:expr) => (print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => (print!(concat!($fmt, "\n"), $($arg)*));
}

#[macro_export]
/// Print a formatted string to the terminal.
/// This macro is similar to `print!()`, but it takes a mutable reference to a terminal instance,
/// instead of locking the global terminal instance. This is useful for performing multiple writes
/// without locking and unlocking the terminal instance each time.
macro_rules! print_terminal {
    ($cga:expr, $($arg:tt)*) => ({
        $crate::device::terminal::print($cga, format_args!($($arg)*));
    });
}

#[macro_export]
/// Print a formatted string to the terminal text buffer with a newline.
/// This macro is similar to `println!()`, but it takes a mutable reference to a terminal instance,
/// instead of locking the global terminal instance. This is useful for performing multiple writes
/// without locking and unlocking the CGA instance each time.
macro_rules! println_terminal {
    ($cga:expr, $fmt:expr) => (print_cga!($cga, concat!($fmt, "\n")));
    ($cga:expr, $fmt:expr, $($arg:tt)*) => (print_cga!($cga, concat!($fmt, "\n"), $($arg)*));
}

/// Helper function for the print macros.
pub fn print(terminal: &mut Terminal, args: fmt::Arguments) {
    terminal.write_fmt(args).expect("Failed to write to terminal");
}