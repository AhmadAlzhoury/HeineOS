/*
 * The command history of the HeineOS shell.
 *
 * The history stores the most recently entered command lines, so they can be
 * recalled with the Up and Down arrow keys or listed with the `history` command.
 *
 * License: GPLv3
 */

use alloc::collections::VecDeque;
use alloc::string::String;

/// Maximum number of command lines kept in the history.
///
/// The history is bounded on purpose: the shell runs for the whole uptime of the
/// system, so an unbounded history would slowly consume the kernel heap.
const HISTORY_CAPACITY: usize = 32;

/// A bounded list of the command lines that were entered before.
///
/// Entries are stored from oldest to newest. When the capacity is reached, the
/// oldest entry is dropped.
pub struct History {
    entries: VecDeque<String>,
}

impl History {
    /// Create an empty history.
    pub fn new() -> Self {
        History { entries: VecDeque::new() }
    }

    /// Add a command line to the history.
    ///
    /// Empty lines are not stored, and a line that is identical to the previous
    /// one does not create a second entry. If the history is full, the oldest
    /// entry is removed.
    pub fn push(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() || self.newest() == Some(line) {
            return;
        }

        if self.entries.len() == HISTORY_CAPACITY {
            self.entries.pop_front();
        }

        self.entries.push_back(String::from(line));
    }

    /// Get the number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Get an entry counted from the newest one, which has the index 0.
    ///
    /// This is the order in which the Up arrow key walks through the history.
    /// `None` is returned if the index is out of range.
    pub fn get(&self, index_from_newest: usize) -> Option<&str> {
        let length = self.entries.len();
        let index = length.checked_sub(index_from_newest + 1)?;

        self.entries.get(index).map(String::as_str)
    }

    /// Iterate over the entries from the oldest to the newest one.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(String::as_str)
    }

    /// Get the most recently stored entry.
    fn newest(&self) -> Option<&str> {
        self.entries.back().map(String::as_str)
    }
}
