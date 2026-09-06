/*
 * Commands that show information about the scheduler.
 *
 * License: GPLv3
 */

use crate::thread::scheduler::scheduler;

/// List the threads known to the scheduler.
///
/// The list is taken as a snapshot, so the scheduler state is only read and
/// never modified. Because the scheduler keeps running while the list is
/// printed, the output describes the situation at the moment of the snapshot.
pub fn ps() {
    let threads = scheduler().thread_snapshot();

    println!("");
    println!("  TID   STATE");
    for thread in &threads {
        let state = if thread.is_idle { "idle" } else { thread.state.as_str() };
        println!("  {:<5} {}", thread.id, state);
    }
    println!("");
    println!("{} thread(s).", threads.len());
    println!("");
}

/// Print the ID of the thread that runs the shell.
pub fn tid() {
    println!("Current thread ID: {}", current_tid());
}

/// Get the ID of the currently running thread.
/// Also used by `sysinfo`, so the scheduler is queried in only one place.
pub fn current_tid() -> usize {
    scheduler().get_active_tid()
}
