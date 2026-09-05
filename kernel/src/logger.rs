/*
 * A simple logger implementation that writes log messages to the serial port (COM1).
 * The log messages include a timestamp, log level, source file name, and line number.
 *
 * Author: Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-07
 * License: GPLv3
 */

use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};
use log::{Metadata, Record};
use crate::device::pit;
use crate::device::{serial, terminal};

/// A simple logger implementing the `log::Log` trait, writing to the serial port (COM1).
pub struct Logger {
    terminal_logging: AtomicBool,
}

impl Logger {
    /// Create a new logger.
    pub const fn new() -> Logger {
        Logger {
            terminal_logging: AtomicBool::new(false),
        }
    }

    /// Enable or disable mirroring log messages to the terminal.
    pub fn enable_terminal_logging(&self, enabled: bool) {
        self.terminal_logging.store(enabled, Ordering::Release);
    }
}

impl log::Log for Logger {
    /// Check if the logger is enabled for the given metadata.
    /// This simple implementation always returns true.
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    /// Print a log record to the serial port.
    fn log(&self, record: &Record) {
        let file = record.file().unwrap_or("unknown");
        let line = record.line().unwrap_or(0);
        let milliseconds = pit::system_time();
        let seconds = milliseconds / 1000;
        let milliseconds = milliseconds % 1000;
        let mut com1 = serial::COM1.lock();

        let _ = writeln!(
            &mut *com1,
            "[{}.{:03}] [{}] [{}@{}] : {}",
            seconds,
            milliseconds,
            level_abbreviation(record.level()),
            file,
            line,
            record.args()
        );

        if self.terminal_logging.load(Ordering::Acquire) {
            if let Some(mut terminal) = terminal::terminal().try_lock() {
                let _ = writeln!(
                    &mut *terminal,
                    "[{}.{:03}] [{}] [{}@{}] : {}",
                    seconds,
                    milliseconds,
                    level_abbreviation(record.level()),
                    file,
                    line,
                    record.args()
                );
            }
        }
    }

    /// Flush the logger.
    /// Since all messages are written immediately, this is a no-op.
    fn flush(&self) {}
}

/// Convert a log level abbreviation to a `log::Level`.
/// Supported abbreviations are:
/// - "TRC" -> Trace
/// - "DBG" -> Debug
/// - "INF" -> Info
/// - "WRN" -> Warn
/// - "ERR" -> Error
/// Returns `None` for unrecognized abbreviations.
pub fn level_from_abbreviation(abbr: &str) -> Option<log::Level> {
    match abbr {
        "TRC" | "trc" => Some(log::Level::Trace),
        "DBG" | "dbg" => Some(log::Level::Debug),
        "INF" | "inf" => Some(log::Level::Info),
        "WRN" | "wrn" => Some(log::Level::Warn),
        "ERR" | "err" => Some(log::Level::Error),
        _ => None,
    }
}

/// Get the three-letter abbreviation for a given log level.
fn level_abbreviation(level: log::Level) -> &'static str {
    match level {
        log::Level::Trace => "TRC",
        log::Level::Debug => "DBG",
        log::Level::Info => "INF",
        log::Level::Warn => "WRN",
        log::Level::Error => "ERR",
    }
}
