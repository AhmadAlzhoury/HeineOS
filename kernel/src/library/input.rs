/*
 * Utility functions for reading text input from the keyboard.
 *
 * Author: Michael Schoetter, Heinrich Heine University Duesseldorf, 2024-05-06
 *         Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-14
 * License: GPLv3
 */

use crate::device::key::{KeyEvent, KeyModifiers, Scancode};
use crate::device::keyboard::keyboard_buffer;
use crate::device::pit;

/// Time to wait between the two drain passes of `settle_and_drain()`.
/// A key combination like Ctrl+C produces its release events a few milliseconds
/// after the press event, so a single drain would not catch them.
const DRAIN_SETTLE_MS: usize = 50;

/// Wait for a key press and return the corresponding ASCII character.
/// If the key pressed does not correspond to an ASCII character (e.g., function keys),
/// the function will continue to wait until a valid ASCII character is pressed.
pub fn read_char() -> char {
    keyboard_buffer().poll_char()
}

/// Wait until the 'Return' (Enter) key is pressed.
pub fn wait_for_return() {
    while read_char() != '\r' {}
}

/// Check whether the given event is a key press of `key` together with either Ctrl key.
///
/// The check is based on the scancode and the modifier flags, *not* on the ASCII
/// code: the keyboard driver has no separate translation table for control
/// combinations, so a Ctrl+L event still reports the ASCII character 'l'.
/// Callers therefore have to test for control combinations *before* handling
/// printable characters, otherwise the plain character would be inserted as well.
pub fn is_ctrl(event: &KeyEvent, key: Scancode) -> bool {
    event.pressed()
        && event.scancode() == Some(key)
        && event
            .modifiers()
            .intersects(KeyModifiers::CTRL_LEFT | KeyModifiers::CTRL_RIGHT)
}

/// Check whether the given event is a Ctrl+C key press (left or right Ctrl).
///
/// Ctrl+C is used in several places (shell input and demo menu), so it has its
/// own name instead of spelling out the scancode at every call site.
pub fn is_ctrl_c(event: &KeyEvent) -> bool {
    is_ctrl(event, Scancode::C)
}

/// Remove all pending key events from the global keyboard buffer.
///
/// Shell, demo menu and the individual demos all read from the same buffer.
/// This function should only be called when the ownership of the keyboard input
/// changes, never while the user is actively typing.
pub fn drain_keyboard_buffer() {
    while keyboard_buffer().pop_key_event().is_some() {}
}

/// Drain the keyboard buffer, wait a short moment and drain it again.
///
/// This is used when input ownership changes because of a key combination
/// (e.g. Ctrl+C leaving the demo menu). The release events belonging to that
/// combination arrive slightly later and would otherwise show up as stale input
/// in the component that takes over.
pub fn settle_and_drain() {
    drain_keyboard_buffer();
    pit::wait(DRAIN_SETTLE_MS);
    drain_keyboard_buffer();
}
