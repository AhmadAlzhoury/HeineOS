/*
 * Read-only filesystem commands: ls, cat, stat and hexdump.
 *
 * All commands use the existing TarFs abstraction. The shell never touches the
 * tar archive itself.
 *
 * License: GPLv3
 */

use crate::filesystem::tarfs::{FileHandle, FileType, FsError, Metadata, TarFs, filesystem};
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

/// Maximum number of bytes printed by `cat`.
const MAX_CAT_BYTES: usize = 64 * 1024;
/// Maximum number of bytes printed by `hexdump`.
const MAX_HEXDUMP_BYTES: usize = 512;
/// Number of bytes shown per line of the `hexdump` output.
const BYTES_PER_LINE: usize = 16;
/// Column width used to align the entries of the `ls` output.
const NAME_WIDTH: usize = 24;

/// List the entries of a directory. Without an argument, the root is listed.
pub fn ls(argument: &str) {
    let path = match argument.trim() {
        "" => "/",
        path => path,
    };

    let Ok(entries) = filesystem().list(path) else {
        println!("Cannot list '{}': no such directory", path);
        return;
    };

    if entries.is_empty() {
        println!("The directory '{}' is empty.", path);
        return;
    }

    println!("");
    for entry in &entries {
        match entry.file_type {
            FileType::Directory => {
                // A trailing slash marks a directory, like in a tar archive.
                let name = format!("{}/", entry.name);
                println!("  {:width$} <dir>", name, width = NAME_WIDTH);
            }
            FileType::File => {
                println!(
                    "  {:width$} {} bytes",
                    entry.name,
                    entry.size,
                    width = NAME_WIDTH
                )
            }
        }
    }
    println!("");
}

/// Print the contents of a file.
pub fn cat(argument: &str) {
    let Some(path) = require_path(argument, "cat <file>") else {
        return;
    };

    let Some(metadata) = regular_file(path) else {
        return;
    };

    let Some(data) = read_file(path, MAX_CAT_BYTES) else {
        return;
    };

    // A tar archive stores arbitrary bytes, so the contents are only printed as
    // text if they really are valid UTF-8.
    match core::str::from_utf8(&data) {
        Ok(text) => print!("{}", text),
        Err(_) => print_sanitized(&data),
    }

    // Make sure the next prompt starts on its own line.
    if !data.is_empty() && !data.ends_with(b"\n") {
        println!("");
    }

    print_truncation_note(data.len(), metadata.size);
}

/// Show the metadata of a file or directory.
pub fn stat(argument: &str) {
    let Some(path) = require_path(argument, "stat <path>") else {
        return;
    };

    let Ok(metadata) = filesystem().metadata(path) else {
        println!("File not found: {}", path);
        return;
    };

    println!("");
    println!("Path: {}", path);
    match metadata.file_type {
        FileType::File => {
            println!("Type: file");
            println!("Size: {} bytes", metadata.size);
        }
        // A tar archive does not store a size for directories.
        FileType::Directory => println!("Type: directory"),
    }
    println!("");
}

/// Print the beginning of a file in hexadecimal.
pub fn hexdump(argument: &str) {
    let Some(path) = require_path(argument, "hexdump <file>") else {
        return;
    };

    let Some(metadata) = regular_file(path) else {
        return;
    };

    let Some(data) = read_file(path, MAX_HEXDUMP_BYTES) else {
        return;
    };

    if data.is_empty() {
        println!("The file '{}' is empty.", path);
        return;
    }

    println!("");
    for (index, chunk) in data.chunks(BYTES_PER_LINE).enumerate() {
        print_hex_line(index * BYTES_PER_LINE, chunk);
    }
    println!("");

    print_truncation_note(data.len(), metadata.size);
}

/// Print a single line of the hexadecimal dump: offset, bytes and characters.
fn print_hex_line(offset: usize, bytes: &[u8]) {
    print!("{:08x} ", offset);

    for byte in bytes {
        print!(" {:02x}", byte);
    }

    // Pad a short last line, so the character column stays aligned.
    for _ in bytes.len()..BYTES_PER_LINE {
        print!("   ");
    }

    print!("  |");
    for byte in bytes {
        print!("{}", printable(*byte));
    }
    println!("|");
}

/// Print bytes that are not valid UTF-8 as text, replacing everything that
/// cannot be shown on the terminal.
fn print_sanitized(data: &[u8]) {
    println!("(binary file, non-printable bytes are shown as '.')");

    for &byte in data {
        match byte {
            b'\n' => println!(""),
            b'\t' => print!(" "),
            _ => print!("{}", printable(byte)),
        }
    }
}

/// Map a byte to a character that can be displayed, or to a dot.
fn printable(byte: u8) -> char {
    match byte {
        0x20..=0x7e => byte as char,
        _ => '.',
    }
}

/// Tell the user if only the beginning of a file was shown.
fn print_truncation_note(shown: usize, total: usize) {
    if total > shown {
        println!("-- truncated: showing {} of {} bytes --", shown, total);
    }
}

/// Get the path argument of a command, or print how the command is used.
fn require_path<'a>(argument: &'a str, usage: &str) -> Option<&'a str> {
    let path = argument.trim();
    if path.is_empty() {
        println!("Usage: {}", usage);
        return None;
    }

    Some(path)
}

/// Check that the given path names a readable file and return its metadata.
///
/// Directories and missing files are reported with their own message, so the
/// commands themselves do not have to deal with these cases.
fn regular_file(path: &str) -> Option<Metadata> {
    match filesystem().metadata(path) {
        Ok(metadata) if metadata.file_type == FileType::File => Some(metadata),
        Ok(_) => {
            println!("'{}' is a directory.", path);
            None
        }
        Err(_) => {
            println!("File not found: {}", path);
            None
        }
    }
}

/// Read at most `limit` bytes from the beginning of a file.
///
/// The file handle is closed in every case, also when reading fails.
fn read_file(path: &str, limit: usize) -> Option<Vec<u8>> {
    let filesystem = filesystem();

    let Ok(handle) = filesystem.open(path) else {
        println!("Cannot open file: {}", path);
        return None;
    };

    let data = read_bytes(filesystem, handle, limit);
    let _ = filesystem.close(handle);

    if data.is_none() {
        println!("Cannot read file: {}", path);
    }

    data
}

/// Read the contents of an open file into a buffer.
///
/// The buffer is only as large as the part of the file that is actually read,
/// so a large file cannot exhaust the kernel heap.
fn read_bytes(filesystem: &TarFs, handle: FileHandle, limit: usize) -> Option<Vec<u8>> {
    let size = filesystem.size(handle).ok()?;
    let mut data = vec![0u8; size.min(limit)];
    let mut read = 0;

    while read < data.len() {
        match filesystem.read(handle, &mut data[read..]) {
            Ok(0) => break,
            Ok(count) => read += count,
            // Reaching the end of the file is not an error here: an empty file
            // simply produces no data at all.
            Err(FsError::EndOfFile) => break,
            Err(_) => return None,
        }
    }

    data.truncate(read);
    Some(data)
}
