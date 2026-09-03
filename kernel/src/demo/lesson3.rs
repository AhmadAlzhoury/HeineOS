/*
 * Contains demos for interrupt-driven input.
 *
 * Author: Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-14
 * License: GPLv3
 */

use crate::device::key::Scancode;
use crate::device::keyboard::keyboard_buffer;
use crate::device::terminal::terminal;

/// Display key events produced by the interrupt-driven keyboard driver.
/// The demo returns when the Escape key is pressed.
pub fn keyboard_interrupt_demo() {
    terminal().lock();

    println!("");
    println!("");
    println!("");
    println!("Lesson 3: Interrupt-driven Keyboard Demo");
    println!("========================================");
    println!("");
    println!("Press keys on your keyboard. Press 'Esc' to exit the demo.");
    println!("");

    loop {
        let key = keyboard_buffer().poll_key_event();
        println!("{:?}", key);

        if key.pressed() && key.scancode() == Some(Scancode::Escape) {
            break;
        }
    }

    println!("");
    println!("Keyboard interrupt demo finished.");
}
