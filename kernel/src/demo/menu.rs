/*
 * Interactive menu for launching all HeineOS demos without rebooting.
 *
 * License: GPLv3
 */

use crate::demo::{lesson1, lesson2, lesson3, lesson4, lesson5, lesson6, lesson7};
use crate::device::framebuffer::{self, BLACK, GRAY, GREEN};
use crate::device::key::Scancode;
use crate::device::keyboard::keyboard_buffer;
use crate::device::terminal::{self, terminal};
use crate::library::bitmap::Bitmap;
use crate::thread::scheduler::scheduler;

const IMAGE_PATH: &str = "/heine.bmp";
const MENU_TOP: usize = 2;
const MENU_ENTRIES: &[&str] = &[
    "Text Demo                 ",
    "Keyboard Demo             ",
    "Interrupt Keyboard Demo   ",
    "Heap Demo                 ",
    "PC Speaker Demo           ",
    "Coroutine Demo            ",
    "Cooperative Thread Demo   ",
    "Preemptive Thread Demo    ",
    "Filesystem Demo           ",
    "Bitmap Demo               ",
    "Peanut-GB Demo            ",
    "PCI Bus Scan              ",
    "RTL8139 Demo              ",
];

/// Display the demo menu forever and launch the selected demo on Enter.
pub fn run() {
    let image = Bitmap::read_from_file(IMAGE_PATH)
        .expect("Failed to read menu bitmap")
        .expect("Invalid or unsupported menu bitmap");
    let mut selected = 0;

    loop {
        drain_keyboard_buffer();
        draw(&image, selected);

        loop {
            let event = keyboard_buffer().poll_key_event();
            if !event.pressed() {
                continue;
            }

            match event.scancode() {
                Some(Scancode::Up) => {
                    selected = selected.checked_sub(1).unwrap_or(MENU_ENTRIES.len() - 1);
                    draw(&image, selected);
                }
                Some(Scancode::Down) => {
                    selected = (selected + 1) % MENU_ENTRIES.len();
                    draw(&image, selected);
                }
                Some(Scancode::Enter) => break,
                _ => {}
            }
        }

        launch(selected);
    }
}

/// Wait until Escape is pressed.
pub fn wait_for_escape() {
    loop {
        let event = keyboard_buffer().poll_key_event();
        if event.pressed() && event.scancode() == Some(Scancode::Escape) {
            return;
        }
    }
}

/// Remove pending key events before changing ownership of keyboard input.
pub fn drain_keyboard_buffer() {
    while keyboard_buffer().pop_key_event().is_some() {}
}

fn draw(image: &Bitmap, selected: usize) {
    terminal().lock().clear();

    let framebuffer = terminal::framebuffer();
    let mut framebuffer = framebuffer.lock();
    framebuffer.draw_str("Demo Menu:", 0, 0, GREEN, BLACK);
    framebuffer.draw_str("Use Up/Down and Enter. Esc returns to this menu.", 0, framebuffer::CHAR_HEIGHT, GREEN, BLACK);

    for (index, entry) in MENU_ENTRIES.iter().enumerate() {
        let background = if index == selected { GRAY } else { BLACK };
        framebuffer.draw_str(entry, 0, (MENU_TOP + index) * framebuffer::CHAR_HEIGHT, GREEN, background);
    }

    let image_x = framebuffer.width().saturating_sub(image.width() as usize) / 2;
    let menu_bottom = (MENU_TOP + MENU_ENTRIES.len() + 1) * framebuffer::CHAR_HEIGHT;
    let centered_y = framebuffer.height().saturating_sub(image.height() as usize) / 2;
    let image_y = centered_y.max(menu_bottom);
    framebuffer.draw_bitmap(image, image_x, image_y);
}

fn launch(selected: usize) {
    match selected {
        0 => {
            lesson1::text_demo();
            wait_for_escape();
        }
        1 => lesson1::keyboard_demo(),
        2 => lesson3::keyboard_interrupt_demo(),
        3 => lesson2::heap_demo(),
        4 => lesson2::speaker_demo(),
        5 => lesson4::coroutine_demo(),
        6 => lesson4::thread_demo(),
        7 => lesson5::thread_demo(),
        8 => {
            lesson6::filesystem_demo();
            wait_for_escape();
        }
        9 => {
            lesson6::bitmap_demo();
            wait_for_escape();
        }
        10 => lesson6::peanut_gb::play("/roms/2048.gb"),
        11 => lesson7::print_pci_devices(),
        12 => lesson7::rtl8139_demo(),
        _ => unreachable!(),
    }

    // Give stopped worker threads a chance to finish before redrawing the menu.
    scheduler().yield_cpu();
}