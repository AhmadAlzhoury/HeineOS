/*
 * Contains demos for text output and keyboard input.
 *
 * Author: Michael Schoetter, Heinrich Heine University Duesseldorf
 *         Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-14
 * License: GPLv3
 */

use crate::device::keyboard::keyboard_buffer;
use crate::device::key::Scancode;
use crate::device::terminal::terminal;

/// A simple text demo, displaying formatted numbers.
pub fn text_demo() {
    terminal().lock().clear();

    println!("");
    println!("Text Demo:");
    println!("");
    println!("  | dec | hex | bin   |");
    println!("  |-----|-----|-------|");

    for number in 0..17 {
        println!(
            "  |  {:>2} |  {:2x} | {:5b} | ",
            number,
            number,
            number
        );
    }
    println!("");
}

/// A simple keyboard demo, displaying the events of key presses and releases.
pub fn keyboard_demo() {
    terminal().lock().clear();

    println!("");
    println!("Keyboard Demo:");
    println!("Press keys on your keyboard. Press 'Esc' to exit the demo.");
    println!("");

    loop {
        let key = keyboard_buffer().poll_key_event();
        println!("{:?}", key);
        if key.pressed() && key.scancode() == Some(Scancode::Escape) {
            break;
        }
    }
}