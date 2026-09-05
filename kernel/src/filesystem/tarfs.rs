/*
 * TarFs - A simple read-only filesystem for accessing files in a tar archive.
 *
 * Author: Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-14
 * License: GPLv3
 */

use alloc::collections::BTreeMap;
use core::cmp::min;
use core::sync::atomic::{AtomicUsize, Ordering};
use tar_no_std::{ArchiveEntry, TarArchiveRef};
use crate::library::once::Once;
use crate::library::spinlock::Spinlock;

/// Global TarFs filesystem instance.
/// This instance is initialized once during kernel startup with a tar archive reference.
/// After initialization, it can be accessed via the `filesystem()` function.
static FILESYSTEM: Once<TarFs> = Once::new();

/// Initialize the global TarFs filesystem with the given tar archive reference.
/// This function should be called once during kernel startup.
/// After calling this function, the filesystem can be accessed via the `filesystem()` function.
pub fn init_filesystem(archive: TarArchiveRef<'static>) {
    FILESYSTEM.init(|| TarFs::new(archive));
}

/// Get a reference to the global TarFs filesystem instance.
/// This function panics if the filesystem has not been initialized yet.
pub fn filesystem() -> &'static TarFs {
    FILESYSTEM.get().expect("Filesystem not initialized")
}

/// A simple read-only filesystem that provides access to files stored in a tar archive.
/// It allows opening files by path, reading data, seeking within files, and getting file sizes.
pub struct TarFs {
    archive: TarArchiveRef<'static>,
    open_handles: Spinlock<BTreeMap<FileHandle, OpenFile>>,
    next_handle: AtomicUsize,
}

#[derive(Copy, Clone, PartialOrd, PartialEq, Ord, Eq, Debug)]
/// A handle representing an open file in the TarFs filesystem.
/// The filesystem returns such handles when files are opened, and they are used to access the opened files for reading.
/// This works similarly to file descriptors in Unix-like operating systems.
pub struct FileHandle(usize);

#[derive(Debug, PartialEq, Eq)]
/// Possible errors that can occur when interacting with the TarFs filesystem.
pub enum FsError {
    /// The specified file was not found in the archive.
    FileNotFound,
    /// The provided file handle is invalid (e.g. not opened or already closed).
    InvalidHandle,
    /// The file has reached the end and no more data can be read.
    EndOfFile
}

/// Modes for seeking within a file, specifying the reference point for the offset.
pub enum SeekMode {
    /// Seek from the beginning of the file.
    Start,
    /// Seek from the current position in the file.
    Current,
    /// Seek from the end of the file.
    End
}

/// An open file in the TarFs filesystem, containing the archive entry and the current read position.
struct OpenFile {
    /// The entry in the tar archive representing the file.
    /// This contains the file data and metadata.
    data: ArchiveEntry<'static>,
    /// The current read position within the file data.
    /// This is updated as data is read from the file and can be modified via seeking.
    position: usize,
}

impl TarFs {
    /// Create a new TarFs filesystem instance with the given tar archive reference.
    pub const fn new(archive: TarArchiveRef<'static>) -> Self {
        TarFs {
            archive,
            open_handles: Spinlock::new(BTreeMap::new()),
            next_handle:
            AtomicUsize::new(0)
        }
    }

    /// Generate the next unique file handle ID.
    fn next_handle_id(&self) -> usize {
        self.next_handle.fetch_add(1, Ordering::SeqCst)
    }

    /// Open a file by its path in the TarFs filesystem.
    /// If the file is found, a `FileHandle` is returned for accessing the opened file.
    /// If the file is not found, an `FsError::FileNotFound` error is returned.
    pub fn open(&self, mut path: &str) -> Result<FileHandle, FsError> {
        // Paths in tar archives do not start with a leading slash, so we remove it if present.
        path = path.trim_start_matches('/');

        // Find the entry in the archive matching the given path.
        let entry = self
            .archive
            .entries()
            .find(|entry| entry.filename().as_str().is_ok_and(|filename| filename == path))
            .ok_or(FsError::FileNotFound)?;

        let mut open_handles = self.open_handles.lock();
        let handle = loop {
            let handle = FileHandle(self.next_handle_id());
            if !open_handles.contains_key(&handle) {
                break handle;
            }
        };

        open_handles.insert(handle, OpenFile { data: entry, position: 0 });
        Ok(handle)
    }

    /// Read data from an opened file into the provided buffer.
    /// The function reads up to `buffer.len()` bytes from the file starting at the current position.
    /// If the end of the file is reached, only the available bytes are read.
    /// The actual number of bytes read is returned.
    /// If the file is already at the end at the start of the read operation, an `FsError::EndOfFile` error is returned.
    /// If the provided file handle is invalid, an `FsError::InvalidHandle` error is returned.
    pub fn read(&self, handle: FileHandle, buffer: &mut [u8]) -> Result<usize, FsError> {
        let mut open_handles = self.open_handles.lock();
        let open_file = open_handles.get_mut(&handle).ok_or(FsError::InvalidHandle)?;
        let data = open_file.data.data();

        if open_file.position >= data.len() {
            return Err(FsError::EndOfFile);
        }

        let read_size = min(buffer.len(), data.len() - open_file.position);
        let end = open_file.position + read_size;
        buffer[..read_size].copy_from_slice(&data[open_file.position..end]);
        open_file.position = end;

        Ok(read_size)
    }

    /// Seek to a new position within an opened file.
    /// The new position is calculated based on the provided offset and seek mode.
    /// Negative offsets are allowed. The resulting position is clamped to the valid range of the file (0 to file size).
    /// The function returns the new position within the file after seeking.
    /// If the provided file handle is invalid, an `FsError::InvalidHandle` error is returned.
    pub fn seek(&self, handle: FileHandle, offset: isize, mode: SeekMode) -> Result<usize, FsError> {
        let mut open_handles = self.open_handles.lock();
        let open_file = open_handles.get_mut(&handle).ok_or(FsError::InvalidHandle)?;
        let size = open_file.data.size();
        let base = match mode {
            SeekMode::Start => 0,
            SeekMode::Current => open_file.position,
            SeekMode::End => size,
        };

        open_file.position = base.saturating_add_signed(offset).min(size);
        Ok(open_file.position)
    }

    /// Get the size of an opened file in bytes.
    /// If the provided file handle is invalid, an `FsError::InvalidHandle` error is returned.
    pub fn size(&self, handle: FileHandle) -> Result<usize, FsError> {
        let open_handles = self.open_handles.lock();
        let open_file = open_handles.get(&handle).ok_or(FsError::InvalidHandle)?;
        Ok(open_file.data.size())
    }

    /// Close an open file and invalidate its handle.
    /// If the provided file handle is invalid, an `FsError::InvalidHandle` error is returned.
    pub fn close(&self, handle: FileHandle) -> Result<(), FsError> {
        let mut open_handles = self.open_handles.lock();
        open_handles
            .remove(&handle)
            .map(|_| ())
            .ok_or(FsError::InvalidHandle)
    }
}