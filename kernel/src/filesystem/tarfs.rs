/*
 * TarFs - A simple read-only filesystem for accessing files in a tar archive.
 *
 * Author: Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-14
 * License: GPLv3
 */

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
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

/// The type of an entry in the filesystem.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FileType {
    /// A regular file, which can be opened and read.
    File,
    /// A directory, which can be listed.
    Directory,
}

/// A single entry of a directory listing.
///
/// This type exists so that the contents of the archive can be enumerated without
/// exposing the underlying tar archive.
pub struct DirectoryEntry {
    /// The name of the entry, without the path of its parent directory.
    pub name: String,
    /// The size of the entry in bytes. Directories are reported with a size of zero.
    pub size: usize,
    /// Whether the entry is a file or a directory.
    pub file_type: FileType,
}

/// Metadata of a single file or directory.
#[derive(Copy, Clone, Debug)]
pub struct Metadata {
    /// The size of the file in bytes. Directories are reported with a size of zero.
    pub size: usize,
    /// Whether the entry is a file or a directory.
    pub file_type: FileType,
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

    /// List the entries that are located directly below the given path.
    ///
    /// A tar archive is a flat list of full path names, it has no real directory
    /// structure. The directory tree is therefore derived from those names: every
    /// archive entry starting with the path of the listed directory contributes either
    /// a file (no further slash in the remaining name) or a subdirectory (the part in
    /// front of the next slash). This works regardless of whether the archive contains
    /// explicit directory entries or not.
    ///
    /// The listing is sorted by name. `FsError::FileNotFound` is returned if no entry
    /// belongs to the given path. The root directory always exists.
    pub fn list(&self, path: &str) -> Result<Vec<DirectoryEntry>, FsError> {
        let prefix = Self::directory_prefix(path);
        let mut entries: Vec<DirectoryEntry> = Vec::new();
        // The root directory is always present, even if the archive is empty.
        let mut directory_exists = prefix.is_empty();

        for entry in self.archive.entries() {
            let filename = entry.filename();
            let Ok(raw_name) = filename.as_str() else {
                continue;
            };

            // Directory entries in a tar archive end with a slash. The flag has to
            // be read before the slash is removed, because it is the only thing
            // that distinguishes an empty directory from a file.
            let is_directory_entry = raw_name.ends_with('/');
            let name = raw_name.trim_end_matches('/');

            // The archive may contain an entry for the listed directory itself.
            // It proves that the directory exists, but is not part of its contents.
            if !prefix.is_empty() && name == prefix.trim_end_matches('/') {
                directory_exists = true;
                continue;
            }

            let Some(remainder) = name.strip_prefix(prefix.as_str()) else {
                continue;
            };

            if remainder.is_empty() {
                continue;
            }

            directory_exists = true;

            match remainder.split_once('/') {
                // The entry is located deeper in the tree, so its first path segment
                // is a subdirectory of the listed directory.
                Some((subdirectory, _)) => Self::push_directory(&mut entries, subdirectory),
                // The entry is a directory directly inside the listed directory.
                None if is_directory_entry => Self::push_directory(&mut entries, remainder),
                // The entry is a file directly inside the listed directory.
                None => entries.push(DirectoryEntry {
                    name: String::from(remainder),
                    size: entry.size(),
                    file_type: FileType::File,
                }),
            }
        }

        if !directory_exists {
            return Err(FsError::FileNotFound);
        }

        entries.sort_by(|first, second| first.name.cmp(&second.name));
        Ok(entries)
    }

    /// Get the metadata of a single file or directory.
    ///
    /// Only the information actually stored in a tar archive is reported: the size of
    /// a file and whether the path denotes a file or a directory.
    pub fn metadata(&self, path: &str) -> Result<Metadata, FsError> {
        let normalized = Self::normalize(path);

        for entry in self.archive.entries() {
            let filename = entry.filename();
            let Ok(name) = filename.as_str() else {
                continue;
            };

            if name.trim_end_matches('/') != normalized {
                continue;
            }

            return if name.ends_with('/') {
                Ok(Metadata { size: 0, file_type: FileType::Directory })
            } else {
                Ok(Metadata { size: entry.size(), file_type: FileType::File })
            };
        }

        // The path does not name an archive entry. It can still be a directory that is
        // only implied by the names of the entries below it.
        if self.directory_has_entries(normalized) {
            Ok(Metadata { size: 0, file_type: FileType::Directory })
        } else {
            Err(FsError::FileNotFound)
        }
    }

    /// Check whether any archive entry is located below the given normalized path.
    fn directory_has_entries(&self, normalized: &str) -> bool {
        if normalized.is_empty() {
            // The root directory always exists.
            return true;
        }

        let prefix = format!("{}/", normalized);
        self.archive.entries().any(|entry| {
            entry
                .filename()
                .as_str()
                .is_ok_and(|name| name.starts_with(prefix.as_str()))
        })
    }

    /// Add a subdirectory to a listing, unless it has already been added.
    /// A directory is usually implied by several archive entries and must appear only once.
    fn push_directory(entries: &mut Vec<DirectoryEntry>, name: &str) {
        let already_listed = entries
            .iter()
            .any(|entry| entry.file_type == FileType::Directory && entry.name == name);

        if already_listed {
            return;
        }

        entries.push(DirectoryEntry {
            name: String::from(name),
            size: 0,
            file_type: FileType::Directory,
        });
    }

    /// Turn a path into the form used inside a tar archive: no leading and no trailing slash.
    fn normalize(path: &str) -> &str {
        path.trim().trim_start_matches('/').trim_end_matches('/')
    }

    /// Build the prefix that all entries of the given directory share.
    /// The root directory has an empty prefix, every other directory ends with a slash.
    fn directory_prefix(path: &str) -> String {
        let normalized = Self::normalize(path);

        if normalized.is_empty() {
            String::new()
        } else {
            format!("{}/", normalized)
        }
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