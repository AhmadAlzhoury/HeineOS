/*
 * Bitmap Image Loader for 24-bit and 32-bit uncompressed BMP files.
 *
 * Author: Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-14
 * License: GPLv3
 */

use alloc::vec;
use alloc::vec::Vec;
use crate::filesystem::tarfs::{filesystem, FsError};

/// Represents a bitmap image loaded from a BMP file.
/// The pixel data is stored as a vector of 32-bit color values in ARGB format.
/// The color format matches the framebuffer color format,
/// so the bitmap data can be copied directly to the framebuffer.
pub struct Bitmap {
    header: BitmapFileHeader,
    pixel_data: Vec<u32>,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
/// The BMP file header structure (see: https://en.wikipedia.org/wiki/BMP_file_format)
struct BitmapFileHeader {
    /// Signature must be 'BM' (0x42, 0x4D).
    signature: [u8; 2],
    /// The size of the whole BMP file in bytes.
    file_size: u32,
    /// Reserved (unused).
    reserved: u32,
    /// The offset to the start of the pixel data.
    data_offset: u32,
    /// The BMP info header.
    info_header: BitmapInfoHeader,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
/// The BMP info header structure (see: https://en.wikipedia.org/wiki/BMP_file_format)
struct BitmapInfoHeader {
    /// The size of this header (should be 40 bytes for BITMAPINFOHEADER).
    header_size: u32,
    /// The width of the bitmap in pixels.
    width: i32,
    /// The height of the bitmap in pixels.
    height: i32,
    /// The number of color planes (must be 1).
    color_planes: u16,
    /// The number of bits per pixel (only 24 is supported).
    bits_per_pixel: u16,
    /// The compression method used (Only the uncompressed formats `None` and `BitFields` are supported).
    compression: Compression,
    /// The size of the raw bitmap data.
    image_size: u32,
    /// The horizontal resolution (pixels per meter, unused by HeineOS).
    x_pixels_per_meter: i32,
    /// The vertical resolution (pixels per meter, unused by HeineOS).
    y_pixels_per_meter: i32,
    /// The number of colors in the color palette (unused by HeineOS).
    colors_used: u32,
    /// The number of important colors (unused by HeineOS).
    important_colors: u32,
}

#[repr(u32)]
#[derive(Copy, Clone, PartialEq)]
enum Compression {
    None = 0,
    BitFields = 3,
}

/// Create a 32-bit ARGB color value from individual red, green, blue, and alpha components.
fn color(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

impl Bitmap {
    /// Read a bitmap image from a file at the given path.
    /// Returns `Ok(Some(Bitmap))` if the file was read and parsed successfully.
    /// Returns `Ok(None)` if the file is not a valid or supported BMP image.
    /// Returns `Err(FsError)` if there was an error reading the file.
    pub fn read_from_file(path: &str) -> Result<Option<Bitmap>, FsError> {
        let filesystem = filesystem();
        let file = filesystem.open(path)?;
        let size = filesystem.size(file)?;

        let mut bmp_data = vec![0u8; size];
        let result = filesystem.read(file, &mut bmp_data);
        filesystem.close(file)?;
        result?;

        Ok(Bitmap::from_bytes(&bmp_data))
    }

    /// Parse a bitmap image from the given byte slice.
    /// Returns `Some(Bitmap)` if the data represents a valid and supported BMP image.
    /// Returns `None` if the data is not a valid or supported BMP image.
    pub fn from_bytes(data: &[u8]) -> Option<Bitmap> {
        const FILE_HEADER_SIZE: usize = 14;
        const INFO_HEADER_SIZE: usize = 40;
        const BYTES_PER_PIXEL: usize = 3;

        fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
            Some(u16::from_le_bytes(data.get(offset..offset + 2)?.try_into().ok()?))
        }

        fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
            Some(u32::from_le_bytes(data.get(offset..offset + 4)?.try_into().ok()?))
        }

        fn i32_at(data: &[u8], offset: usize) -> Option<i32> {
            Some(i32::from_le_bytes(data.get(offset..offset + 4)?.try_into().ok()?))
        }

        if data.len() < FILE_HEADER_SIZE + INFO_HEADER_SIZE || data.get(..2)? != b"BM" {
            return None;
        }

        let file_size = u32_at(data, 2)?;
        let reserved = u32_at(data, 6)?;
        let data_offset = u32_at(data, 10)?;
        let header_size = u32_at(data, 14)?;
        let width = i32_at(data, 18)?;
        let signed_height = i32_at(data, 22)?;
        let color_planes = u16_at(data, 26)?;
        let bits_per_pixel = u16_at(data, 28)?;
        let compression = u32_at(data, 30)?;
        let image_size = u32_at(data, 34)?;
        let x_pixels_per_meter = i32_at(data, 38)?;
        let y_pixels_per_meter = i32_at(data, 42)?;
        let colors_used = u32_at(data, 46)?;
        let important_colors = u32_at(data, 50)?;

        if header_size < INFO_HEADER_SIZE as u32
            || width <= 0
            || signed_height == 0
            || color_planes != 1
            || bits_per_pixel != 24
            || compression != Compression::None as u32
        {
            return None;
        }

        let width = usize::try_from(width).ok()?;
        let height = usize::try_from(signed_height.checked_abs()?).ok()?;
        let row_data_size = width.checked_mul(BYTES_PER_PIXEL)?;
        let row_stride = row_data_size.checked_add(3)? & !3;
        let bitmap_data_size = row_stride.checked_mul(height)?;
        let data_start = usize::try_from(data_offset).ok()?;
        let data_end = data_start.checked_add(bitmap_data_size)?;

        if data_end > data.len()
            || usize::try_from(file_size).ok()? > data.len()
            || usize::try_from(file_size).ok()? < data_end
        {
            return None;
        }

        let pixel_count = width.checked_mul(height)?;
        let mut pixel_data = vec![0; pixel_count];

        for target_y in 0..height {
            let source_y = if signed_height > 0 {
                height - 1 - target_y
            } else {
                target_y
            };
            let source_row = data_start.checked_add(source_y.checked_mul(row_stride)?)?;
            let target_row = target_y.checked_mul(width)?;

            for x in 0..width {
                let source = source_row.checked_add(x.checked_mul(BYTES_PER_PIXEL)?)?;
                let blue = *data.get(source)?;
                let green = *data.get(source + 1)?;
                let red = *data.get(source + 2)?;
                pixel_data[target_row + x] = color(red, green, blue, 0);
            }
        }

        let header = BitmapFileHeader {
            signature: *b"BM",
            file_size,
            reserved,
            data_offset,
            info_header: BitmapInfoHeader {
                header_size,
                width: width as i32,
                height: height as i32,
                color_planes,
                bits_per_pixel,
                compression: Compression::None,
                image_size,
                x_pixels_per_meter,
                y_pixels_per_meter,
                colors_used,
                important_colors,
            },
        };

        Some(Bitmap { header, pixel_data })
    }

    /// Get the width of the bitmap in pixels.
    pub fn width(&self) -> u32 {
        self.header.info_header.width as u32
    }

    /// Get the height of the bitmap in pixels.
    pub fn height(&self) -> u32 {
        self.header.info_header.height as u32
    }

    /// Get a reference to the pixel data of the bitmap.
    /// The data is in 32-bit ARGB format, matching the framebuffer color format.
    pub fn pixel_data(&self) -> &[u32] {
        &self.pixel_data
    }
}