/*
 * Frontend for the Peanut-GB emulator.
 * ROMs are loaded from the filesystem, and the Game Boy screen is rendered to the framebuffer.
 *
 * Author: Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-04-01
 * License: GPLv3
 */

use alloc::vec;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_size_t, c_void};
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};
use log::{info, warn};
use crate::device::key::Scancode;
use crate::device::keyboard::keyboard_buffer;
use crate::device::serial::COM3;
use crate::device::{pit, terminal};
use crate::filesystem::tarfs::{filesystem, FsError, SeekMode};
use crate::library::once::Once;
use crate::library::spinlock::Spinlock;

unsafe extern "C" {
    /// Get the size of the `gb_s` structure (implemented in `peanut-gb.c`).
    /// This struct holds the entire state of the emulated Game Boy.
    /// Since we do not have a Rust binding for this, we use a C function to get the size.
    fn gb_size() -> c_int;

    /// Get a pointer to the joypad state in the `gb_s` structure (implemented in `peanut-gb.c`).
    /// The joypad state is a single byte where each bit represents a button state.
    /// If no button is pressed, all bits are set to 1 (0xff).
    /// The buttons are represented by the `JoypadButton` enum.
    fn gb_get_joypad_ptr(gb: *mut c_void) -> *mut u8;

    /// Initialization function for the PeanutGB emulator.
    /// The `gb` parameter must point to block of memory large enough to hold the `gb_s` structure.
    /// The size of this structure can be obtained by calling `gb_size()`.
    /// The `priv_data` parameter can be used to pass additional data to the emulator,
    /// but is currently unused in this implementation.
    /// The other parameters are function pointers and crucial for the emulator to function.
    fn gb_init(gb: *mut c_void,
               gb_rom_read: unsafe extern "C" fn(*mut c_void, u32) -> u8,
               gb_cart_ram_read: unsafe extern "C" fn(*mut c_void, u32) -> u8,
               gb_cart_ram_write: unsafe extern "C" fn(*mut c_void, u32, u8),
               gb_error: unsafe extern "C" fn(*mut c_void, i32, u16),
               priv_data: *const c_void) -> c_int;

    /// Initialize the LCD of the PeanutGB emulator.
    /// This function must be called after the emulator has been initialized.
    /// If this function is not called, the emulator will work, but not render any graphics.
    fn gb_init_lcd(gb: *mut c_void, lcd_draw_line: *const c_void);

    /// Run a single frame of the PeanutGB emulator.
    /// This function must be called in a loop to run the emulator.
    /// To maintain a stable frame rate, the caller should measure the time taken by this function
    /// and sleep for the remaining time to achieve the desired frame rate.
    /// Otherwise, the emulator will run as fast as possible.
    fn gb_run_frame(gb: *mut c_void);

    /// Get the name of the ROM currently loaded in the PeanutGB emulator.
    /// The name is returned as a C string (null-terminated).
    fn gb_get_rom_name(gb: *mut c_void, title_str: *const c_char) -> *const c_char;

    /// Get the RAM size of the currently loaded ROM in the PeanutGB emulator.
    /// The RAM size is written to the given pointer `ram_size`.
    /// A return value of 0 indicates success.
    fn gb_get_save_size_s(gb: *mut c_void, ram_size: *mut c_size_t) -> c_int;
}

/// Bitmask for the joypad buttons. See `gb_get_joypad_ptr` for more details.
#[repr(u8)]
enum JoypadButton {
    A = 0x01,
    B = 0x02,
    Select = 0x04,
    Start = 0x08,
    Right = 0x10,
    Left = 0x20,
    Up = 0x40,
    Down = 0x80,
}

/// Error codes used in `gb_error`.
#[derive(Debug, PartialEq)]
enum GbError {
    UnknownError = 0,
    InvalidOpcode = 1,
    InvalidRead = 2,
    InvalidWrite = 3,
}

impl TryFrom<c_int> for GbError {
    type Error = ();

    fn try_from(value: c_int) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(GbError::UnknownError),
            1 => Ok(GbError::InvalidOpcode),
            2 => Ok(GbError::InvalidRead),
            3 => Ok(GbError::InvalidWrite),
            _ => Err(())
        }
    }
}

/// Error codes used in `gb_init`.
#[derive(Debug, PartialEq)]
enum GbInitError {
    NoError = 0,
    CartridgeUnsupported,
    InvalidChecksum,
    UnknownError = 0xff,
}

impl TryFrom<c_int> for GbInitError {
    type Error = ();

    fn try_from(value: c_int) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(GbInitError::NoError),
            1 => Ok(GbInitError::CartridgeUnsupported),
            2 => Ok(GbInitError::InvalidChecksum),
            3 => Ok(GbInitError::UnknownError),
            _ => Err(())
        }
    }
}

/// The target frame rate for the emulator.
/// The original Game Boy runs at 60 frames per second.
/// Increasing this value will make the emulator run faster,
/// decreasing it will make the emulator run slower.
const TARGET_FRAME_RATE: usize = 60;

/// The number of milliseconds per frame at the target frame rate.
const MS_PER_FRAME: usize = 1000 / TARGET_FRAME_RATE;

/// The original Game Boy screen resolution (160x144 pixels).
const GB_SCREEN_RES: (usize, usize) = (160, 144);

/// Scale used to render the Game Boy screen.
const SCREEN_SCALE: usize = 2;

/// The color palette used for rendering.
/// The Game Boy supports 4 shades of gray, represented as 32-bit ARGB colors in this array.
static PALETTE: &[u32] = &[
    0xe0f8d0, // White
    0x88c070, // Light Gray
    0x346856, // Dark Gray
    0x081820, // Black
];

/// The ROM file to be played by the emulator.
static ROM: Once<Vec<u8>> = Once::new();

/// Cartridge RAM, sized according to the loaded ROM's header.
static CART_RAM: Spinlock<Vec<u8>> = Spinlock::new(Vec::new());
static CART_RAM_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Save data imported from the initrd and exported through COM3.
const SAVE_PATH: &str = "/roms/gameboy.sav";

/// Read a byte from the ROM file at the offset specified by `addr`.
/// This is a callback function for the PeanutGB emulator.
unsafe extern "C" fn gb_rom_read(_gb: *mut c_void, addr: u32) -> u8 {
    ROM.get()
        .and_then(|rom| rom.get(addr as usize))
        .copied()
        .unwrap_or(0xff)
}

/// Read a byte from the save RAM at the offset specified by `addr`.
/// This is a callback function for the PeanutGB emulator.
///
/// This is mostly needed for save game support and part of an optional assignment.
unsafe extern "C" fn gb_cart_ram_read(_gb: *mut c_void, addr: u32) -> u8 {
    CART_RAM.lock().get(addr as usize).copied().unwrap_or(0xff)
}

/// Write a byte to the save RAM at the offset specified by `addr`.
/// This is a callback function for the PeanutGB emulator.
///
/// This is mostly needed for save game support and part of an optional assignment.
unsafe extern "C" fn gb_cart_ram_write(_gb: *mut c_void, addr: u32, val: u8) {
    if let Some(byte) = CART_RAM.lock().get_mut(addr as usize) {
        *byte = val;
    }
}

/// Draw a line of pixels from the Game Boy screen to the framebuffer.
/// The buffer pointed to by `pixels` contains the pixel data for the line.
/// Each pixel is represented by a single byte, whose first two bits represent the color index.
/// The other bits are used for Game Boy Color emulation, but are ignored in this implementation.
unsafe extern "C" fn lcd_draw_line(_gb: *mut c_void, pixels: *const u8, line: u8) {
    let line = line as usize;
    if pixels.is_null() || line >= GB_SCREEN_RES.1 {
        return;
    }

    let pixels = unsafe { core::slice::from_raw_parts(pixels, GB_SCREEN_RES.0) };
    let mut framebuffer = terminal::framebuffer().lock();
    let screen_width = GB_SCREEN_RES.0 * SCREEN_SCALE;
    let screen_height = GB_SCREEN_RES.1 * SCREEN_SCALE;
    if screen_width > framebuffer.width() || screen_height > framebuffer.height() {
        return;
    }

    let start_x = (framebuffer.width() - screen_width) / 2;
    let start_y = (framebuffer.height() - screen_height) / 2;

    for (source_x, pixel) in pixels.iter().enumerate() {
        let color = PALETTE[(pixel & 0x03) as usize];
        let target_x = start_x + source_x * SCREEN_SCALE;
        let target_y = start_y + line * SCREEN_SCALE;

        for y_offset in 0..SCREEN_SCALE {
            for x_offset in 0..SCREEN_SCALE {
                unsafe {
                    framebuffer.draw_pixel_unchecked(
                        target_x + x_offset,
                        target_y + y_offset,
                        color,
                    );
                }
            }
        }
    }
}

/// Handle emulation errors.
/// This is a callback function for the PeanutGB emulator.
unsafe extern "C" fn gb_error(_gb: *mut c_void, error: c_int, addr: u16) {
    let error = GbError::try_from(error).unwrap_or(GbError::UnknownError);
    panic!("PeanutGB error [{:?}] at address [0x{:0>4x}]!", error, addr);
}

/// Play the given ROM file using the Peanut-GB emulator.
pub fn play(rom_path: &str) {
    let filesystem = filesystem();
    let file = filesystem.open(rom_path).expect("Failed to open Game Boy ROM");
    let rom_size = filesystem.size(file).expect("Failed to get Game Boy ROM size");
    let mut rom = vec![0; rom_size];
    filesystem.read(file, &mut rom).expect("Failed to read Game Boy ROM");
    filesystem.close(file).expect("Failed to close Game Boy ROM");
    ROM.init(|| rom);

    let gb_size = usize::try_from(unsafe { gb_size() }).expect("Invalid Peanut-GB state size");
    let mut gb = vec![0u8; gb_size];
    let gb_ptr = gb.as_mut_ptr().cast::<c_void>();

    let init_result = unsafe {
        gb_init(
            gb_ptr,
            gb_rom_read,
            gb_cart_ram_read,
            gb_cart_ram_write,
            gb_error,
            ptr::null(),
        )
    };
    let init_error = GbInitError::try_from(init_result).unwrap_or(GbInitError::UnknownError);
    if init_error != GbInitError::NoError {
        panic!("Failed to initialize Peanut-GB (Error: {:?})", init_error);
    }

    let mut save_size = 0usize;
    let save_size_result = unsafe { gb_get_save_size_s(gb_ptr, &mut save_size) };
    assert_eq!(save_size_result, 0, "Invalid cartridge RAM size");
    initialize_cart_ram(save_size);

    let mut title = [0 as c_char; 17];
    unsafe {
        gb_get_rom_name(gb_ptr, title.as_mut_ptr());
        gb_init_lcd(gb_ptr, lcd_draw_line as *const c_void);
    }
    let title_len = title.iter().position(|&character| character == 0).unwrap_or(title.len());
    let title_bytes = unsafe { core::slice::from_raw_parts(title.as_ptr().cast::<u8>(), title_len) };
    info!("Starting Game Boy ROM '{}'", core::str::from_utf8(title_bytes).unwrap_or("unknown"));

    let joypad = unsafe { gb_get_joypad_ptr(gb_ptr) };
    assert!(!joypad.is_null(), "Peanut-GB returned a null joypad pointer");
    terminal::framebuffer().lock().clear();

    loop {
        let mut exit = false;
        while let Some(event) = keyboard_buffer().pop_key_event() {
            if event.pressed() && event.scancode() == Some(Scancode::Escape) {
                exit = true;
                continue;
            }

            let Some(button) = event.scancode().and_then(joypad_button) else {
                continue;
            };

            unsafe {
                if event.pressed() {
                    *joypad &= !(button as u8);
                } else {
                    *joypad |= button as u8;
                }
            }
        }
        if exit {
            break;
        }

        let frame_start = pit::system_time();
        unsafe { gb_run_frame(gb_ptr); }
        let elapsed = pit::system_time().wrapping_sub(frame_start);
        if elapsed < MS_PER_FRAME {
            pit::wait(MS_PER_FRAME - elapsed);
        }
    }

    export_save_data();
}

/// Initialize cartridge RAM and preload save data from the initrd when available.
fn initialize_cart_ram(save_size: usize) {
    let filesystem = filesystem();
    let mut cart_ram = CART_RAM.lock();

    if CART_RAM_INITIALIZED.load(Ordering::Acquire) && cart_ram.len() == save_size {
        info!("Reusing {} bytes of cartridge RAM from the current session", save_size);
        return;
    }

    cart_ram.clear();
    cart_ram.resize(save_size, 0);
    CART_RAM_INITIALIZED.store(true, Ordering::Release);

    if save_size == 0 {
        info!("The loaded ROM does not use cartridge RAM");
        return;
    }

    let file = match filesystem.open(SAVE_PATH) {
        Ok(file) => file,
        Err(FsError::FileNotFound) => {
            info!("No save file found; starting with {} bytes of empty cartridge RAM", save_size);
            return;
        }
        Err(error) => panic!("Failed to open save file: {:?}", error),
    };

    let file_size = filesystem.size(file).expect("Failed to get save file size");
    if file_size == 0 || file_size % save_size != 0 {
        filesystem.close(file).expect("Failed to close invalid save file");
        warn!(
            "Ignoring save file with invalid size: expected a nonzero multiple of {}, got {}",
            save_size, file_size
        );
        return;
    }

    let snapshot_offset = file_size - save_size;
    filesystem
        .seek(file, snapshot_offset as isize, SeekMode::Start)
        .expect("Failed to seek to latest save snapshot");
    let bytes_read = filesystem.read(file, &mut cart_ram).expect("Failed to read save file");
    filesystem.close(file).expect("Failed to close save file");
    assert_eq!(bytes_read, save_size, "Failed to read complete save file");
    info!(
        "Loaded latest {}-byte cartridge RAM snapshot from '{}' ({} snapshot(s))",
        save_size,
        SAVE_PATH,
        file_size / save_size
    );
}

/// Export cartridge RAM as raw bytes through the third serial port.
fn export_save_data() {
    let cart_ram = CART_RAM.lock();
    let mut serial = COM3.lock();
    serial.init();
    for &byte in cart_ram.iter() {
        serial.write_raw_byte(byte);
    }
    info!("Exported {} bytes of cartridge RAM through COM3", cart_ram.len());
}

/// Map keyboard scancodes to Game Boy joypad buttons.
fn joypad_button(scancode: Scancode) -> Option<JoypadButton> {
    match scancode {
        Scancode::X => Some(JoypadButton::A),
        Scancode::Y => Some(JoypadButton::B),
        Scancode::Space => Some(JoypadButton::Select),
        Scancode::Enter => Some(JoypadButton::Start),
        Scancode::Right => Some(JoypadButton::Right),
        Scancode::Left => Some(JoypadButton::Left),
        Scancode::Up => Some(JoypadButton::Up),
        Scancode::Down => Some(JoypadButton::Down),
        _ => None,
    }
}