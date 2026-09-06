/*
 * System commands: uptime, memory, framebuffer, sleep and the system overview.
 *
 * License: GPLv3
 */

use crate::allocator::global;
use crate::device::framebuffer;
use crate::device::pit;
use crate::device::terminal;
use crate::machine;
use super::{hardware, threads};

/// Longest duration accepted by `sleep`.
/// The shell cannot read any input while sleeping, so the wait is limited.
const MAX_SLEEP_MS: usize = 10000;

/// Size of the framebuffer and of the terminal that is drawn on it.
///
/// The values are collected in one struct, so that `fbinfo` and `sysinfo` can
/// use the same code to read them.
pub struct FramebufferInfo {
    /// Width of the framebuffer in pixels.
    pub width: usize,
    /// Height of the framebuffer in pixels.
    pub height: usize,
    /// Number of text columns that fit into the framebuffer.
    pub columns: usize,
    /// Number of text rows that fit into the framebuffer.
    pub rows: usize,
}

/// Print the system uptime as reported by the PIT.
pub fn time() {
    let (hours, minutes, seconds, milliseconds) = split_uptime(pit::system_time());
    println!(
        "System uptime: {:02}:{:02}:{:02}.{:03}",
        hours, minutes, seconds, milliseconds
    );
}

/// Split a duration in milliseconds into hours, minutes, seconds and milliseconds.
/// Uses integer arithmetic only, since the kernel does not rely on floating point math.
fn split_uptime(total_milliseconds: usize) -> (usize, usize, usize, usize) {
    let milliseconds = total_milliseconds % 1000;
    let total_seconds = total_milliseconds / 1000;

    (
        total_seconds / 3600,
        (total_seconds / 60) % 60,
        total_seconds % 60,
        milliseconds,
    )
}

/// Print aggregate information about the kernel heap.
pub fn meminfo() {
    let stats = global::heap_stats();

    println!("");
    println!("Kernel heap information:");
    println!("  Total: {} bytes", stats.total);
    println!("  Used:  {} bytes", stats.used);
    println!("  Free:  {} bytes", stats.free);
    println!(
        "  Free blocks: {} (largest: {} bytes)",
        stats.free_blocks, stats.largest_free_block
    );
    println!("");
    println!("These values describe the kernel heap only.");
    println!("HeineOS has no physical memory manager yet.");
    println!("");
}

/// Print the size of the framebuffer and of the terminal drawn on it.
pub fn fbinfo() {
    let info = framebuffer_info();

    println!("");
    println!("Framebuffer information");
    println!("-----------------------");
    println!("  Resolution:       {}x{}", info.width, info.height);
    println!("  Character width:  {}", framebuffer::CHAR_WIDTH);
    println!("  Character height: {}", framebuffer::CHAR_HEIGHT);
    println!("  Columns:          {}", info.columns);
    println!("  Rows:             {}", info.rows);
    println!("");
}

/// Read the size of the framebuffer and of the terminal.
///
/// Both locks are taken one after another and released immediately, because
/// printing locks the terminal again and the spinlock is not reentrant.
pub fn framebuffer_info() -> FramebufferInfo {
    let (width, height) = {
        let display = terminal::framebuffer().lock();
        (display.width(), display.height())
    };

    let (columns, rows) = terminal::terminal().lock().size();

    FramebufferInfo { width, height, columns, rows }
}

/// Print a summary of the whole system.
///
/// All values are read through the same helpers that the individual commands
/// use, so there is no second implementation of any of these calculations.
pub fn sysinfo() {
    let (hours, minutes, seconds, milliseconds) = split_uptime(pit::system_time());
    let stats = global::heap_stats();
    let display = framebuffer_info();

    println!("");
    println!("HeineOS System Information");
    println!("--------------------------");
    println!(
        "  Uptime:       {:02}:{:02}:{:02}.{:03}",
        hours, minutes, seconds, milliseconds
    );
    println!("  Current TID:  {}", threads::current_tid());
    println!("  Heap total:   {} bytes", stats.total);
    println!("  Heap used:    {} bytes", stats.used);
    println!("  Heap free:    {} bytes", stats.free);
    println!("  Framebuffer:  {}x{}", display.width, display.height);
    println!("  Terminal:     {}x{} characters", display.columns, display.rows);
    println!("  PCI devices:  {}", hardware::device_count());
    println!("  Filesystem:   TarFs (read-only)");
    println!("");
}

/// Wait for the given number of milliseconds.
///
/// The duration is validated before waiting, so invalid input only produces an
/// error message instead of blocking the shell or panicking.
pub fn sleep(argument: &str) {
    let argument = argument.trim();
    if argument.is_empty() {
        print_sleep_usage();
        return;
    }

    let Ok(duration) = argument.parse::<usize>() else {
        println!("Invalid duration: '{}'", argument);
        print_sleep_usage();
        return;
    };

    if duration == 0 || duration > MAX_SLEEP_MS {
        println!("The duration must be between 1 and {} ms.", MAX_SLEEP_MS);
        return;
    }

    println!("Sleeping for {} ms...", duration);
    // `pit::wait()` yields the CPU, so other threads keep running while waiting.
    pit::wait(duration);
    println!("Done.");
}

/// Print how the `sleep` command is used.
fn print_sleep_usage() {
    println!("Usage: sleep <milliseconds>");
}

/// Shut the machine down.
///
/// The actual power off is done by the machine module, so this command contains
/// no hardware specific code. It never returns.
pub fn shutdown() -> ! {
    println!("Shutting down HeineOS...");
    machine::shutdown()
}

/// Reboot the machine.
///
/// Like `shutdown`, the reset itself is left to the machine module.
pub fn reboot() -> ! {
    println!("Rebooting HeineOS...");
    machine::reboot()
}
