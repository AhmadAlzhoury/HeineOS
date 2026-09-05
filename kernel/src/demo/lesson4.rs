/*
 * Contains demos for coroutines and threads.
 *
 * Author: Michael Schoetter, Heinrich Heine University Duesseldorf
 *         Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-15
 * License: GPLv3
 */
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};
use crate::coroutine::coroutine::Coroutine;
use crate::device::key::Scancode;
use crate::device::keyboard::keyboard_buffer;
use crate::device::terminal::terminal;
use crate::thread::scheduler::{resume_scheduling, scheduler, suspend_scheduling};
use crate::thread::thread::Thread;

static COROUTINE_CALLER: AtomicPtr<Coroutine> = AtomicPtr::new(ptr::null_mut());
static COROUTINE_FIRST_ID: AtomicUsize = AtomicUsize::new(0);
static THREAD_DEMO_RUNNING: AtomicBool = AtomicBool::new(false);
static THREAD_DEMO_WORKERS: AtomicUsize = AtomicUsize::new(0);
static THREAD_DEMO_NEXT_ROW: AtomicUsize = AtomicUsize::new(0);

/// A demo function showcasing coroutines.
/// It starts three coroutines, each incrementing a counter and printing it to the terminal in an endless loop.
/// The coroutines switch to the next coroutine after each print.
pub fn coroutine_demo() {
    terminal().lock().clear();
    println!("Coroutine Demo:");
    println!("");
    println!("Press 'Esc' to return to the demo menu.");

    let mut caller = Coroutine::new(coroutine_caller_placeholder);
    let mut first = Coroutine::new(coroutine_loop);
    let mut second = Coroutine::new(coroutine_loop);
    let mut third = Coroutine::new(coroutine_loop);

    first.set_next(&mut second);
    second.set_next(&mut third);
    third.set_next(&mut first);

    COROUTINE_FIRST_ID.store(first.id(), Ordering::Release);
    COROUTINE_CALLER.store(ptr::from_mut(caller.as_mut()), Ordering::Release);
    suspend_scheduling();
    first.start_resumable(&mut caller);
    resume_scheduling();
    COROUTINE_CALLER.store(ptr::null_mut(), Ordering::Release);
}

fn coroutine_caller_placeholder(_coroutine: &mut Coroutine) {
    unreachable!();
}

/// The function executed by each coroutine in the coroutine demo.
/// It increments a counter and prints it to the terminal in an endless loop,
/// switching to the next coroutine after each print.
fn coroutine_loop(coroutine: &mut Coroutine) {
    let mut counter = 0usize;

    loop {
        while let Some(event) = keyboard_buffer().pop_key_event() {
            if event.pressed() && event.scancode() == Some(Scancode::Escape) {
                let caller = COROUTINE_CALLER.load(Ordering::Acquire);
                assert!(!caller.is_null(), "Missing coroutine caller context");
                unsafe { coroutine.switch_to(&mut *caller); }
            }
        }

        {
            let mut terminal = terminal().lock();
            let row = coroutine.id() - COROUTINE_FIRST_ID.load(Ordering::Acquire) + 8;
            terminal.set_pos(0, row);
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
    println!("Press 'Esc' to return to the demo menu.");

    let scheduler = scheduler();
    THREAD_DEMO_RUNNING.store(true, Ordering::Release);
    THREAD_DEMO_WORKERS.store(3, Ordering::Release);
    THREAD_DEMO_NEXT_ROW.store(0, Ordering::Release);
    scheduler.ready(Thread::new(thread_entry));
    scheduler.ready(Thread::new(thread_entry));
    scheduler.ready(Thread::new(thread_entry));

    while THREAD_DEMO_RUNNING.load(Ordering::Acquire) {
        if let Some(event) = keyboard_buffer().pop_key_event() {
            if event.pressed() && event.scancode() == Some(Scancode::Escape) {
                THREAD_DEMO_RUNNING.store(false, Ordering::Release);
            }
        }
        scheduler.yield_cpu();
    }

    while THREAD_DEMO_WORKERS.load(Ordering::Acquire) != 0 {
        scheduler.yield_cpu();
    }
    scheduler.cleanup_terminated_threads();
}

/// The function executed by each thread in the thread demo.
/// It increments a counter and prints it to the terminal in an endless loop,
/// yielding the CPU to the next thread after each print.
fn thread_entry() {
    let id = scheduler().get_active_tid();
    let row = THREAD_DEMO_NEXT_ROW.fetch_add(1, Ordering::AcqRel) + 7;
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

        counter = counter.wrapping_add(1);
        scheduler().yield_cpu();
    }
}
