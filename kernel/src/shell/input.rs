/*
 * Interactive line input for the HeineOS shell.
 *
 * This module does not implement a keyboard driver. It consumes the `KeyEvent`s
 * produced by the existing interrupt driven keyboard driver
 * (`crate::device::keyboard`) and turns them into a line of text.
 *
 * Besides typing and Backspace, the line editor supports recalling commands from
 * the history with the arrow keys and completing command names with Tab.
 *
 * License: GPLv3
 */

use crate::device::key::{KeyEvent, KeyModifiers, Scancode};
use crate::device::keyboard::keyboard_buffer;
use crate::device::terminal::terminal;
use crate::library::input::{is_ctrl, is_ctrl_c};
use crate::shell::history::History;
use crate::shell::registry::{self, CommandInfo};
use alloc::string::String;

/// The prompt that is printed before every input line.
///
/// The line editor needs it as well, because it has to print the prompt again
/// after listing the candidates of an ambiguous Tab completion.
pub const PROMPT: &str = "heineos> ";

/// Keys that do not produce text and are therefore ignored while editing a line.
///
/// This list is required because `KeyEvent::set_ascii()` keeps the ASCII code of the
/// previously decoded key when the current key has none. Without this filter, pressing
/// an arrow key would insert the character of the key that was pressed before it.
const IGNORED_KEYS: &[Scancode] = &[
    Scancode::Escape,
    Scancode::Tab,
    Scancode::Up,
    Scancode::Down,
    Scancode::Left,
    Scancode::Right,
    Scancode::Home,
    Scancode::End,
    Scancode::PageUp,
    Scancode::PageDown,
    Scancode::Insert,
    Scancode::Del,
    Scancode::Meta,
    Scancode::F1,
    Scancode::F2,
    Scancode::F3,
    Scancode::F4,
    Scancode::F5,
    Scancode::F6,
    Scancode::F7,
    Scancode::F8,
    Scancode::F9,
    Scancode::F10,
    Scancode::F11,
    Scancode::F12,
];

/// A keyboard shortcut of the line editor.
///
/// The shortcuts are recognized from the physical key and the Ctrl modifier, so
/// they never depend on the ASCII code the keyboard driver reports.
enum Shortcut {
    /// Ctrl+L: clear the screen and redraw prompt and input.
    ClearScreen,
    /// Ctrl+U: discard the text of the current input line.
    ClearLine,
    /// Ctrl+Up / Ctrl+PageUp: show older terminal output.
    ScrollUp(usize),
    /// Ctrl+Down / Ctrl+PageDown: show newer terminal output.
    ScrollDown(usize),
}

impl Shortcut {
    /// Recognize a shortcut in a key event, if there is one.
    ///
    /// Returns `None` for every event that is not one of the Ctrl combinations
    /// handled by the line editor, so the caller can continue with normal keys.
    /// This is what separates Ctrl+Up (scrolling) from a plain Up (history).
    fn from_event(event: &KeyEvent) -> Option<Shortcut> {
        if is_ctrl(event, Scancode::L) {
            Some(Shortcut::ClearScreen)
        } else if is_ctrl(event, Scancode::U) {
            Some(Shortcut::ClearLine)
        } else if is_ctrl(event, Scancode::Up) {
            Some(Shortcut::ScrollUp(1))
        } else if is_ctrl(event, Scancode::Down) {
            Some(Shortcut::ScrollDown(1))
        } else if is_ctrl(event, Scancode::PageUp) {
            Some(Shortcut::ScrollUp(page_size()))
        } else if is_ctrl(event, Scancode::PageDown) {
            Some(Shortcut::ScrollDown(page_size()))
        } else {
            None
        }
    }
}

/// Number of lines a single page scroll moves the viewport.
///
/// One line less than the terminal height, so that one line of context stays
/// visible after scrolling by a page.
fn page_size() -> usize {
    let (_, rows) = terminal().lock().size();
    rows.saturating_sub(1).max(1)
}

/// The result of reading a single input line.
pub enum ReadLineResult {
    /// The user confirmed the line with Enter.
    Line(String),
    /// The user cancelled the line with Ctrl+C.
    Interrupted,
}

/// Read a single line from the keyboard, echoing the input to the terminal.
///
/// The function blocks until the line is confirmed with Enter or cancelled with
/// Ctrl+C. The history is only read, so recalling a command does not change it.
pub fn read_line(history: &History) -> ReadLineResult {
    let mut editor = LineEditor::new(history);

    loop {
        let event = keyboard_buffer().poll_key_event();

        // Ignore key release events, they would otherwise be echoed twice.
        if !event.pressed() {
            continue;
        }

        // Ctrl+C has to be checked before all other keys, because the keyboard
        // driver reports the ASCII character 'c' for it as well.
        if is_ctrl_c(&event) {
            println!("^C");
            return ReadLineResult::Interrupted;
        }

        // The remaining Ctrl combinations are handled before the plain keys, so
        // that Ctrl+Up does not navigate the history and Ctrl+L does not type an 'l'.
        if let Some(shortcut) = Shortcut::from_event(&event) {
            editor.handle_shortcut(shortcut);
            continue;
        }

        match event.scancode() {
            Some(Scancode::Enter) => {
                println!("");
                return ReadLineResult::Line(editor.into_line());
            }
            Some(Scancode::Backspace) => editor.erase_last_char(),
            Some(Scancode::Up) => editor.recall_older(),
            Some(Scancode::Down) => editor.recall_newer(),
            Some(Scancode::Tab) => editor.complete(),
            _ => editor.insert_printable(&event),
        }
    }
}

/// The line that is currently being edited, together with its history position.
struct LineEditor<'a> {
    /// The text that has been entered so far. It always matches what is shown
    /// on the screen after the prompt.
    buffer: String,
    /// The history the arrow keys walk through.
    history: &'a History,
    /// The position in the history, counted from the newest entry.
    /// `None` means that the user is editing a new line instead of a recalled one.
    position: Option<usize>,
    /// The line the user had typed before starting to walk through the history.
    /// It is restored when the user returns to the newest position.
    draft: String,
}

impl<'a> LineEditor<'a> {
    /// Create an editor for an empty input line.
    fn new(history: &'a History) -> Self {
        LineEditor {
            buffer: String::new(),
            history,
            position: None,
            draft: String::new(),
        }
    }

    /// Consume the editor and return the line that was entered.
    fn into_line(self) -> String {
        self.buffer
    }

    /// Execute a keyboard shortcut.
    fn handle_shortcut(&mut self, shortcut: Shortcut) {
        match shortcut {
            Shortcut::ClearScreen => self.redraw_screen(),
            Shortcut::ClearLine => self.clear_line(),
            // Scrolling only changes what the terminal displays. The input buffer
            // is not touched, so the command being typed survives it unchanged.
            Shortcut::ScrollUp(lines) => terminal().lock().scroll_view_up(lines),
            Shortcut::ScrollDown(lines) => terminal().lock().scroll_view_down(lines),
        }
    }

    /// Clear the screen and print prompt and current input again (Ctrl+L).
    ///
    /// The input buffer is not touched, so the command that is currently being
    /// typed stays editable, exactly like in a normal interactive shell.
    fn redraw_screen(&mut self) {
        // The lock is released at the end of this statement, because `print!`
        // locks the terminal again and the spinlock is not reentrant.
        terminal().lock().clear();
        print!("{}{}", PROMPT, self.buffer);
    }

    /// Discard the text of the current input line (Ctrl+U).
    ///
    /// Only the text after the prompt is removed. The history is not modified,
    /// and no command is executed.
    fn clear_line(&mut self) {
        self.replace_line("");

        // The cleared line is a new one, so history navigation starts over.
        self.position = None;
        self.draft.clear();
    }

    /// Remove the last character from the buffer and from the screen.
    ///
    /// If the buffer is already empty, nothing happens. This is what keeps
    /// Backspace from erasing the shell prompt.
    fn erase_last_char(&mut self) {
        if self.buffer.pop().is_some() {
            terminal().lock().backspace();
        }
    }

    /// Append the character of a printable key event to the buffer and echo it.
    ///
    /// Events of non-text keys (see `IGNORED_KEYS`), control characters and
    /// combinations with Ctrl or Alt are ignored.
    fn insert_printable(&mut self, event: &KeyEvent) {
        const IGNORED_MODIFIERS: KeyModifiers = KeyModifiers::CTRL_LEFT
            .union(KeyModifiers::CTRL_RIGHT)
            .union(KeyModifiers::ALT_LEFT)
            .union(KeyModifiers::ALT_RIGHT);

        if !produces_text(event) || event.modifiers().intersects(IGNORED_MODIFIERS) {
            return;
        }

        let Some(character) = event.ascii() else {
            return;
        };

        // Control characters have no useful representation on screen.
        if character.is_control() {
            return;
        }

        self.buffer.push(character);
        print!("{}", character);
    }

    /// Show the next older entry of the history (Up arrow key).
    ///
    /// At the oldest entry, nothing happens. The line that was typed before the
    /// first Up is remembered, so it can be restored later.
    fn recall_older(&mut self) {
        let history = self.history;
        let next = match self.position {
            None => 0,
            Some(current) => current + 1,
        };

        let Some(entry) = history.get(next) else {
            // Either the history is empty or the oldest entry is already shown.
            return;
        };

        if self.position.is_none() {
            self.draft = self.buffer.clone();
        }

        self.position = Some(next);
        self.replace_line(entry);
    }

    /// Show the next newer entry of the history (Down arrow key).
    ///
    /// After the newest entry, the line that was typed before walking through
    /// the history is restored.
    fn recall_newer(&mut self) {
        let history = self.history;

        match self.position {
            // The user is not walking through the history, so there is nothing newer.
            None => {}
            Some(0) => {
                // `take` moves the draft out of the editor, which avoids borrowing
                // the editor immutably and mutably at the same time.
                let draft = core::mem::take(&mut self.draft);
                self.position = None;
                self.replace_line(&draft);
            }
            Some(current) => {
                let newer = current - 1;
                if let Some(entry) = history.get(newer) {
                    self.position = Some(newer);
                    self.replace_line(entry);
                }
            }
        }
    }

    /// Complete the command name that is currently being typed (Tab key).
    ///
    /// Only the first word of the line is completed, because only command names
    /// are known to the shell. The candidates come from the same table that is
    /// used for the `help` output.
    fn complete(&mut self) {
        // Completion applies to the command name only, so a line that already
        // contains an argument is left alone.
        if self.buffer.is_empty() || self.buffer.contains(char::is_whitespace) {
            return;
        }

        let candidates = registry::completions(&self.buffer);
        match candidates.len() {
            0 => {}
            1 => self.replace_line(candidates[0].name),
            _ => self.complete_ambiguous(&candidates),
        }
    }

    /// Handle a Tab completion with more than one candidate.
    ///
    /// The candidates are listed and the input line is printed again, completed
    /// as far as all candidates have their name in common.
    fn complete_ambiguous(&mut self, candidates: &[&'static CommandInfo]) {
        let prefix = registry::longest_common_prefix(candidates);

        println!("");
        for candidate in candidates {
            println!("  {}", candidate.name);
        }
        println!("");

        // The listing has moved the cursor, so prompt and input are printed again.
        print!("{}{}", PROMPT, self.buffer);

        if prefix.len() > self.buffer.len() {
            self.replace_line(&prefix);
        }
    }

    /// Replace the text of the current input line on screen and in the buffer.
    ///
    /// The characters of the old line are erased first, so a shorter new line
    /// cannot leave parts of the old one on the screen.
    fn replace_line(&mut self, text: &str) {
        let visible_characters = self.buffer.chars().count();
        if visible_characters > 0 {
            // The lock is released at the end of this statement, because the
            // `print!` below locks the terminal again and it is not reentrant.
            terminal().lock().erase(visible_characters);
        }

        self.buffer.clear();
        self.buffer.push_str(text);
        print!("{}", self.buffer);
    }
}

/// Check whether the given key event belongs to a key that produces text.
///
/// Events without a known scancode are ignored as well, because their reported
/// ASCII code may be a leftover of the previous key event.
fn produces_text(event: &KeyEvent) -> bool {
    match event.scancode() {
        Some(scancode) => !IGNORED_KEYS.contains(&scancode),
        None => false,
    }
}
