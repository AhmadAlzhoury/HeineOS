/*
 * Interactive menu for launching all HeineOS demos without rebooting.
 *
 * License: GPLv3
 */

use crate::demo::{lesson1, lesson2, lesson4, lesson5, lesson6, lesson7};
use crate::device::framebuffer::{self, BLACK, GRAY, GREEN};
use crate::device::key::Scancode;
use crate::device::keyboard::keyboard_buffer;
use crate::device::terminal::{self, terminal};
use crate::library::bitmap::Bitmap;
use crate::library::input::{drain_keyboard_buffer, is_ctrl_c};
use crate::thread::scheduler::scheduler;

const IMAGE_PATH: &str = "/heine.bmp";
/// First terminal row used for the menu entries. The rows above hold the title
/// and the control hints.
const MENU_TOP: usize = 5;
/// The control hints shown below the menu title.
const MENU_HINTS: &[&str] = &[
    "Use Up/Down and Enter.",
    "Esc exits a running demo.",
    "Ctrl+C returns to the shell.",
];
const MENU_ENTRIES: &[&str] = &[
    "Text Demo                 ",
    "Keyboard Demo             ",
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

/// What the user requested while the menu was displayed.
enum MenuAction {
    /// Start the currently selected demo.
    Launch,
    /// Leave the menu and return to the caller (the shell).
    Exit,
}

/// Display the demo menu and launch the selected demo on Enter.
///
/// The function returns to its caller when the user presses Ctrl+C, which is how
/// the shell regains control of the terminal and the keyboard.
pub fn run() {
    let image = Bitmap::read_from_file(IMAGE_PATH)
        .expect("Failed to read menu bitmap")
        .expect("Invalid or unsupported menu bitmap");
    let mut selected = 0;

    loop {
        drain_keyboard_buffer();
        draw(&image, selected);

        match read_menu_action(&image, &mut selected) {
            MenuAction::Launch => launch(selected),
            MenuAction::Exit => return,
        }
    }
}

/// Handle key events until a demo should be launched or the menu should be left.
///
/// Up and Down change the selection and redraw the menu immediately, so this
/// function also needs access to the menu bitmap.
fn read_menu_action(image: &Bitmap, selected: &mut usize) -> MenuAction {
    loop {
        let event = keyboard_buffer().poll_key_event();
        if !event.pressed() {
            continue;
        }

        // Checked before the scancode match, because Ctrl+C also reports the
        // scancode of the 'C' key.
        if is_ctrl_c(&event) {
            return MenuAction::Exit;
        }

        match event.scancode() {
            Some(Scancode::Up) => {
                *selected = selected.checked_sub(1).unwrap_or(MENU_ENTRIES.len() - 1);
                draw(image, *selected);
            }
            Some(Scancode::Down) => {
                *selected = (*selected + 1) % MENU_ENTRIES.len();
                draw(image, *selected);
            }
            Some(Scancode::Enter) => return MenuAction::Launch,
            _ => {}
        }
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

fn draw(image: &Bitmap, selected: usize) {
    terminal().lock().clear();

    let framebuffer = terminal::framebuffer();
    let mut framebuffer = framebuffer.lock();
    framebuffer.draw_str("Demo Menu:", 0, 0, GREEN, BLACK);
    for (index, hint) in MENU_HINTS.iter().enumerate() {
        framebuffer.draw_str(hint, 0, (index + 1) * framebuffer::CHAR_HEIGHT, GREEN, BLACK);
    }

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
        2 => lesson2::heap_demo(),
        3 => lesson2::speaker_demo(),
        4 => lesson4::coroutine_demo(),
        5 => lesson4::thread_demo(),
        6 => lesson5::thread_demo(),
        7 => {
            lesson6::filesystem_demo();
            wait_for_escape();
        }
        8 => {
            lesson6::bitmap_demo();
            wait_for_escape();
        }
        9 => lesson6::peanut_gb::play("/roms/2048.gb"),
        10 => lesson7::print_pci_devices(),
        11 => lesson7::rtl8139_demo(),
        _ => unreachable!(),
    }

    // Give stopped worker threads a chance to finish before redrawing the menu.
    scheduler().yield_cpu();
}