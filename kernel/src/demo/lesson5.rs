/*
 * Contains the preemptive multithreading demo.
 *
 * Author: Michael Schoetter, Heinrich Heine University Duesseldorf
 *         Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-15
 * License: GPLv3
 */

use log::info;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crate::device::key::Scancode;
use crate::device::keyboard::keyboard_buffer;
use crate::device::speaker;
use crate::device::speaker::SPEAKER;
use crate::device::terminal::terminal;
use crate::device::pit;
use crate::thread::scheduler::scheduler;
use crate::thread::thread::Thread;

static THREAD_DEMO_RUNNING: AtomicBool = AtomicBool::new(false);
static THREAD_DEMO_WORKERS: AtomicUsize = AtomicUsize::new(0);
static THREAD_DEMO_NEXT_ROW: AtomicUsize = AtomicUsize::new(0);

/// Showcase preemptive multithreading with three counters and a PC speaker melody.
/// The PIT preempts the threads at a fixed interval, while occasional voluntary yields
/// prevent the counter threads from starving each other at the terminal lock.
pub fn thread_demo() {
    terminal().lock().clear();
    println!("Preemptive Thread Demo:");
    println!("");
    println!("Three counters and a PC speaker melody run concurrently.");
    println!("Press 'Esc' to return to the demo menu.");

    let scheduler = scheduler();

    THREAD_DEMO_RUNNING.store(true, Ordering::Release);
    THREAD_DEMO_WORKERS.store(4, Ordering::Release);
    THREAD_DEMO_NEXT_ROW.store(0, Ordering::Release);
    speaker::set_cancelled(false);
    scheduler.ready(Thread::new(thread_entry));
    scheduler.ready(Thread::new(thread_entry));
    scheduler.ready(Thread::new(thread_entry));
    scheduler.ready(Thread::new(melody_entry));

    while THREAD_DEMO_RUNNING.load(Ordering::Acquire) {
        if let Some(event) = keyboard_buffer().pop_key_event() {
            if event.pressed() && event.scancode() == Some(Scancode::Escape) {
                THREAD_DEMO_RUNNING.store(false, Ordering::Release);
                speaker::set_cancelled(true);
            }
        }
        scheduler.yield_cpu();
    }

    while THREAD_DEMO_WORKERS.load(Ordering::Acquire) != 0 {
        scheduler.yield_cpu();
    }
    scheduler.cleanup_terminated_threads();
    speaker::set_cancelled(false);
}

/// Increment and display a counter until it reaches the limit.
fn thread_entry() {
    const COUNTER_LIMIT: usize = 10000;
    const YIELD_INTERVAL: usize = 10;

    let id = scheduler().get_active_tid();
    let row = THREAD_DEMO_NEXT_ROW.fetch_add(1, Ordering::AcqRel) + 7;
    let start_time = pit::system_time();
    let mut counter = 0usize;

    loop {
        if !THREAD_DEMO_RUNNING.load(Ordering::Acquire) {
            THREAD_DEMO_WORKERS.fetch_sub(1, Ordering::AcqRel);
            return;
        }

        {
            let mut terminal = terminal().lock();
            terminal.set_pos(0, row);
            print_terminal!(&mut *terminal, "Thread [{}]: {:>12}", id, counter);
        }

        if counter == COUNTER_LIMIT {
            let elapsed = pit::system_time().wrapping_sub(start_time);
            {
                let mut terminal = terminal().lock();
                terminal.set_pos(0, row);
                print_terminal!(
                    &mut *terminal,
                    "Thread [{}]: {:>12} ({:>6} ms)",
                    id,
                    counter,
                    elapsed,
                );
            }
            info!("Thread {} reached {} after {} ms and exits", id, counter, elapsed);
            THREAD_DEMO_WORKERS.fetch_sub(1, Ordering::AcqRel);
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

    THREAD_DEMO_WORKERS.fetch_sub(1, Ordering::AcqRel);

    {
        let mut terminal = terminal().lock();
        terminal.set_pos(0, 12);
        print_terminal!(&mut *terminal, "Melody: done   ");
    }

    info!("Melody thread exits");
}
