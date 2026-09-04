/*
 * Contains the preemptive multithreading demo.
 *
 * Author: Michael Schoetter, Heinrich Heine University Duesseldorf
 *         Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-15
 * License: GPLv3
 */

use log::info;
use crate::device::speaker;
use crate::device::speaker::SPEAKER;
use crate::device::terminal::terminal;
use crate::device::pit;
use crate::thread::scheduler::scheduler;
use crate::thread::thread::Thread;

/// Showcase preemptive multithreading with three counters and a PC speaker melody.
/// The PIT preempts the threads at a fixed interval, while occasional voluntary yields
/// prevent the counter threads from starving each other at the terminal lock.
pub fn thread_demo() {
    terminal().lock().clear();
    println!("Preemptive Thread Demo:");
    println!("");
    println!("Three counters and a PC speaker melody run concurrently.");

    let scheduler = scheduler();

    // The scheduler creates idle thread T0. The counters are T1-T3 and
    // the PC speaker melody runs in T4.
    scheduler.ready(Thread::new(thread_entry));
    scheduler.ready(Thread::new(thread_entry));
    scheduler.ready(Thread::new(thread_entry));
    scheduler.ready(Thread::new(melody_entry));
    scheduler.schedule();
}

/// Increment and display a counter until it reaches the limit.
fn thread_entry() {
    const COUNTER_LIMIT: usize = 1000;
    const YIELD_INTERVAL: usize = 10;

    let id = scheduler().get_active_tid();
    let start_time = pit::system_time();
    let mut counter = 0usize;

    loop {
        {
            let mut terminal = terminal().lock();
            terminal.set_pos(0, id + 7);
            print_terminal!(&mut *terminal, "Thread [{}]: {:>12}", id, counter);
        }

        if counter == COUNTER_LIMIT {
            let elapsed = pit::system_time().wrapping_sub(start_time);
            {
                let mut terminal = terminal().lock();
                terminal.set_pos(0, id + 7);
                print_terminal!(
                    &mut *terminal,
                    "Thread [{}]: {:>12} ({:>6} ms)",
                    id,
                    counter,
                    elapsed,
                );
            }
            info!("Thread {} reached {} after {} ms and exits", id, counter, elapsed);
            return;
        }

        counter = counter.wrapping_add(1);
        if counter % YIELD_INTERVAL == 0 {
            scheduler().yield_cpu();
        }
    }
}

/// Play a short melody while the counter threads are running.
fn melody_entry() {
    {
        let mut terminal = terminal().lock();
        terminal.set_pos(0, 12);
        print_terminal!(&mut *terminal, "Melody: playing");
    }

    let mut speaker = SPEAKER.lock();
    for frequency in [
        speaker::C1,
        speaker::D1,
        speaker::E1,
        speaker::F1,
        speaker::G1,
        speaker::A1,
        speaker::B1,
        speaker::C2,
    ] {
        speaker.play(frequency, 250);
    }
    drop(speaker);

    {
        let mut terminal = terminal().lock();
        terminal.set_pos(0, 12);
        print_terminal!(&mut *terminal, "Melody: done   ");
    }

    info!("Melody thread exits");
}
