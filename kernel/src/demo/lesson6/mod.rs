use alloc::vec;
use crate::device::terminal::{self, terminal};
use crate::filesystem::tarfs::{filesystem, FsError, SeekMode};
use crate::library::bitmap::Bitmap;

pub mod peanut_gb;

/// Read a text file from the initial ramdisk and print it to the terminal.
pub fn filesystem_demo() {
    const PATH: &str = "/roms/2048-license.txt";

    terminal().lock().clear();
    println!("Filesystem Demo:");
    println!("");
    println!("Reading '{}':\n", PATH);

    let filesystem = filesystem();
    assert!(matches!(filesystem.open("/missing.txt"), Err(FsError::FileNotFound)));

    let handle = filesystem.open(PATH).expect("Failed to open filesystem demo file");
    let size = filesystem.size(handle).expect("Failed to get demo file size");

    let mut prefix = [0; 9];
    assert_eq!(filesystem.read(handle, &mut prefix), Ok(prefix.len()));
    assert_eq!(&prefix, b"Copyright");
    assert_eq!(filesystem.seek(handle, -4, SeekMode::Current), Ok(5));
    assert_eq!(filesystem.seek(handle, 0, SeekMode::End), Ok(size));
    assert!(matches!(filesystem.read(handle, &mut prefix), Err(FsError::EndOfFile)));
    assert_eq!(filesystem.seek(handle, 0, SeekMode::Start), Ok(0));

    let mut data = vec![0; size];
    let bytes_read = filesystem.read(handle, &mut data).expect("Failed to read demo file");
    filesystem.close(handle).expect("Failed to close demo file");
    assert!(matches!(filesystem.size(handle), Err(FsError::InvalidHandle)));

    let text = core::str::from_utf8(&data[..bytes_read])
        .expect("Filesystem demo file contains invalid UTF-8");
    println!("{}", text);
}

/// Load a bitmap from the initial ramdisk and draw it centered on the framebuffer.
pub fn bitmap_demo() {
    const PATH: &str = "/heine.bmp";

    let bitmap = Bitmap::read_from_file(PATH)
        .expect("Failed to read bitmap file")
        .expect("Invalid or unsupported bitmap file");
    assert!(Bitmap::from_bytes(&[]).is_none());
    assert!(Bitmap::from_bytes(b"not a bitmap").is_none());

    let framebuffer = terminal::framebuffer();
    let mut framebuffer = framebuffer.lock();

    // Exercise clipping at the bottom-right framebuffer boundary.
    let clipped_x = framebuffer.width().saturating_sub(bitmap.width() as usize / 2);
    let clipped_y = framebuffer.height().saturating_sub(bitmap.height() as usize / 2);
    framebuffer.draw_bitmap(&bitmap, clipped_x, clipped_y);

    framebuffer.clear();
    let x = framebuffer.width().saturating_sub(bitmap.width() as usize) / 2;
    let y = framebuffer.height().saturating_sub(bitmap.height() as usize) / 2;
    framebuffer.draw_bitmap(&bitmap, x, y);
}