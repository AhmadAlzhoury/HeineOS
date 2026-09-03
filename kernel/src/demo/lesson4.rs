/*
 * Contains demos for coroutines and threads.
 *
 * Author: Michael Schoetter, Heinrich Heine University Duesseldorf
 *         Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-15
 * License: GPLv3
 */
use log::info;
use crate::coroutine::coroutine::Coroutine;
use crate::device::terminal::terminal;
use crate::thread::scheduler::scheduler;
use crate::thread::thread::Thread;

/// A demo function showcasing coroutines.
/// It starts three coroutines, each incrementing a counter and printing it to the terminal in an endless loop.
/// The coroutines switch to the next coroutine after each print.
pub fn coroutine_demo() {
    terminal().lock().clear();
    println!("Coroutine Demo:");
    println!("");
    println!("This demo connot be exited. Please reboot the system to get back to the menu.");

    let mut first = Coroutine::new(coroutine_loop);
    let mut second = Coroutine::new(coroutine_loop);
    let mut third = Coroutine::new(coroutine_loop);

    first.set_next(&mut second);
    second.set_next(&mut third);
    third.set_next(&mut first);

    first.start();
}

/// The function executed by each coroutine in the coroutine demo.
/// It increments a counter and prints it to the terminal in an endless loop,
/// switching to the next coroutine after each print.
fn coroutine_loop(coroutine: &mut Coroutine) {
    let mut counter = 0usize;

    loop {
        {
            let mut terminal = terminal().lock();
            terminal.set_pos(0, coroutine.id() + 8);
            print_terminal!(&mut *terminal, "Coroutine {}: {:>12}", coroutine.id(), counter);
        }

        counter = counter.wrapping_add(1);
        coroutine.switch();
    }
}

/// A demo function showcasing threads.
/// It starts three threads, each incrementing a counter and printing it to the terminal in an endless loop.
/// The threads yield the CPU to the next thread after each print.
/// The first thread also kills the other two threads after a certain number of iterations and finally exits itself, ending the demo.
pub fn thread_demo() {

    terminal().lock().clear();
    println!("Thread Demo:");
    println!("");
    println!("This demo connot be exited. Please reboot the system to get back to the menu.");

    let scheduler = scheduler();

    // The scheduler creates idle thread T0. The basic thread is therefore T1,
    // followed by the three counter threads T2, T3 and T4.
    scheduler.ready(Thread::new(thread_entry));
    scheduler.ready(Thread::new(thread_entry));
    scheduler.ready(Thread::new(thread_entry));
    scheduler.schedule();
}

/// The function executed by each thread in the thread demo.
/// It increments a counter and prints it to the terminal in an endless loop,
/// yielding the CPU to the next thread after each print.
fn thread_entry() {
    const COORDINATOR_ID: usize = 1;
    const FIRST_WORKER_ID: usize = 2;
    const SECOND_WORKER_ID: usize = 3;

    let id = scheduler().get_active_tid();
    let mut counter = 0usize;

    loop {
        {
            let mut terminal = terminal().lock();
            terminal.set_pos(0, id + 7);
            print_terminal!(&mut *terminal, "Thread [{}]: {:>12}", id, counter);
            if id == SECOND_WORKER_ID {
                terminal.set_pos(0, id+8);
            }
        }

        if id == COORDINATOR_ID && counter == 200 {
            scheduler().kill(FIRST_WORKER_ID);
            info!("Thread {} killed thread {}", id, FIRST_WORKER_ID);
        }

        if id == COORDINATOR_ID && counter == 100 {
            scheduler().kill(SECOND_WORKER_ID);
            info!("Thread {} killed thread {}", id, FIRST_WORKER_ID);
        }

        if id == COORDINATOR_ID && counter == 299 {
            info!("Thread {} exits", id);
            scheduler().exit();
            return;
        }

        counter = counter.wrapping_add(1);
        scheduler().yield_cpu();
    }
}