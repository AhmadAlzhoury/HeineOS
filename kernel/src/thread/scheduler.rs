/*
 * A basic round-robin scheduler for cooperative threads.
 * Priorities are not supported.
 *
 * Author: Michael Schoettner, Heinrich Heine University Duesseldorf, 2023-05-15
 *         Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-15
 * License: GPLv3
 */

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt::Display;
use core::{fmt, ptr};
use core::sync::atomic::{AtomicBool, Ordering};
use crate::allocator;
use crate::device::cpu;
use crate::library::once::Once;
use crate::library::queue::LinkedQueue;
use crate::library::spinlock::Spinlock;
use crate::thread::idle_thread::idle_thread;
use crate::thread::thread::Thread;

/// Upper limit for the number of threads reported by `Scheduler::thread_snapshot()`.
/// The snapshot allocates its memory before locking the scheduler, so its size has to
/// be known in advance.
const MAX_SNAPSHOT_THREADS: usize = 64;

/// Global scheduler instance
static SCHEDULER: Once<Scheduler> = Once::new();
static SCHEDULING_SUSPENDED: AtomicBool = AtomicBool::new(false);

/// Global access to the scheduler.
pub fn scheduler() -> &'static Scheduler {
    SCHEDULER.init(Scheduler::new)
}

/// Yield the CPU if the global scheduler has already been initialized.
/// This avoids initializing the scheduler from low-level code such as a spinlock.
pub fn yield_cpu_if_initialized() {
    if !cpu::is_int_enabled() {
        return;
    }

    if let Some(scheduler) = SCHEDULER.get() {
        scheduler.yield_cpu();
    }
}

/// Temporarily prevent thread context switches while preserving hardware interrupts.
pub fn suspend_scheduling() {
    SCHEDULING_SUSPENDED.store(true, Ordering::Release);
}

/// Re-enable thread context switches after `suspend_scheduling()`.
pub fn resume_scheduling() {
    SCHEDULING_SUSPENDED.store(false, Ordering::Release);
}

/// Unlock the scheduler state.
/// This function is called from assembly code.
/// Usually, the mutex would be unlocked automatically when going out of scope.
/// However, since we switch to a different thread in `yield_cpu()` and `exit()`,
/// the scope is not left and the mutex remains locked.
/// As a workaround, we provide this function to unlock the scheduler manually.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unlock_scheduler() {
    unsafe {
        scheduler().state.force_unlock();
    }
}

/// The state of the scheduler.
/// It contains the active thread, the ready queue with all other threads,
/// and a queue of terminated threads waiting for cleanup.
/// The state is contained in its own struct so that it can be locked via a mutex.
struct SchedulerState {
    initialized: bool,
    active_thread: Option<Box<Thread>>,
    ready_queue: LinkedQueue<Box<Thread>>,
    terminated_threads: LinkedQueue<Box<Thread>>,
}

/// Represents the scheduler.
/// It is round-robin-based and uses a queue to manage the threads.
pub struct Scheduler {
    state: Spinlock<SchedulerState>,
}

/// The state a thread can be in, as far as the scheduler can tell.
///
/// The round-robin scheduler only distinguishes between the one thread that currently
/// owns the CPU and the threads waiting in the ready queue. There are no other states,
/// so none are reported.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ThreadState {
    /// The thread is currently running on the CPU.
    Running,
    /// The thread is waiting in the ready queue.
    Ready,
}

impl ThreadState {
    /// Get the name of the state for display purposes.
    pub fn as_str(&self) -> &'static str {
        match self {
            ThreadState::Running => "running",
            ThreadState::Ready => "ready",
        }
    }
}

/// Read-only information about a single thread.
///
/// This struct exists so that the scheduler can be inspected (e.g. by the `ps` shell
/// command) without handing out references to its internal queues.
#[derive(Copy, Clone, Debug)]
pub struct ThreadInfo {
    /// The ID of the thread.
    pub id: usize,
    /// The state the thread was in when the snapshot was taken.
    pub state: ThreadState,
}

impl Scheduler {
    /// Create a new scheduler instance with an empty ready queue
    /// and an idle thread as the active thread.
    pub fn new() -> Self {
        let state = SchedulerState {
            initialized: false,
            active_thread: Some(Thread::new(idle_thread)),
            ready_queue: LinkedQueue::new(),
            terminated_threads: LinkedQueue::new(),
        };

        Scheduler { state: Spinlock::new(state) }
    }

    /// Get the ID of the currently active thread.
    pub fn get_active_tid(&self) -> usize {
        let state = self.state.lock();

        state.active_thread.as_ref().unwrap().id()
    }

    /// Take a read-only snapshot of the threads known to the scheduler.
    ///
    /// The snapshot contains the active thread followed by the threads in the ready
    /// queue. It only reads the scheduler state, so calling it does not influence
    /// scheduling in any way.
    ///
    /// The result vector is allocated *before* the scheduler state is locked. Allocating
    /// while holding that lock could deadlock: `yield_cpu()` refuses to switch threads
    /// while the allocator is locked, so the thread owning the allocator could never
    /// be scheduled again.
    pub fn thread_snapshot(&self) -> Vec<ThreadInfo> {
        let mut threads = Vec::with_capacity(MAX_SNAPSHOT_THREADS);

        let state = self.state.lock();

        if let Some(active_thread) = state.active_thread.as_ref() {
            threads.push(ThreadInfo { id: active_thread.id(), state: ThreadState::Running });
        }

        for thread in state.ready_queue.iter() {
            if threads.len() >= MAX_SNAPSHOT_THREADS {
                break;
            }

            threads.push(ThreadInfo { id: thread.id(), state: ThreadState::Ready });
        }

        threads
    }

    /// Start the scheduler.
    /// This function must only be called once.
    pub fn schedule(&self) {
        let mut state = self.state.lock();

        state.initialized = true;

        // The active thread is never None, since we must at least have the idle thread.
        state.active_thread.as_mut().unwrap().start();
    }

    /// Register a new thread in the ready queue.
    pub fn ready(&self, thread: Box<Thread>) {
        let mut state = self.state.lock();

        state.ready_queue.enqueue(thread);
    }

    /// Terminate the current (calling) thread and switch to the next one.
    pub fn exit(&self) {
        let mut state = self.state.lock();

        // The active thread is never None, since we must at least have the idle thread.
        let mut current = state.active_thread.take().unwrap();
        let current_ptr = ptr::from_mut(current.as_mut());
        // The idle thread never exits, so there must be at least one thread in the queue.
        let next = state.ready_queue.dequeue().unwrap();

        // Keep ownership of the current thread until the idle thread can safely
        // free its resources after switching to a different stack.
        state.terminated_threads.enqueue(current);
        state.active_thread = Some(next);
        unsafe {
            // Switch to the next thread.
            // `terminated_threads` contains the old thread we want to exit,
            // while `state.active_thread` contains the next one.
            Thread::switch(current_ptr, state.active_thread.as_mut().unwrap().as_mut());
        }
    }

    /// Free the resources of all terminated threads.
    ///
    /// Each thread is removed while the scheduler state is locked, but dropped
    /// only after releasing that lock because dropping its stack invokes the
    /// global allocator.
    pub fn cleanup_terminated_threads(&self) {
        loop {
            let terminated_thread = {
                let mut state = self.state.lock();
                state.terminated_threads.dequeue()
            };

            let Some(terminated_thread) = terminated_thread else {
                return;
            };

            drop(terminated_thread);
        }
    }

    /// Yield the CPU and switch to the next thread in the ready queue.
    pub fn yield_cpu(&self) {
        if SCHEDULING_SUSPENDED.load(Ordering::Acquire) {
            return;
        }

        if allocator::global::is_allocator_locked() {
            return;
        }

        let Some(mut state) = self.state.try_lock() else {
            return;
        };

        if !state.initialized {
            return;
        }

        let Some(next) = state.ready_queue.dequeue() else {
            return;
        };

        let mut current = state.active_thread.take().unwrap();
        let current_ptr = ptr::from_mut(current.as_mut());

        state.ready_queue.enqueue(current);
        state.active_thread = Some(next);
        unsafe {
            Thread::switch(current_ptr, state.active_thread.as_mut().unwrap().as_mut());
        }
    }

    /// Kill the thread with the given ID by removing it from the ready queue.
    pub fn kill(&self, to_kill_id: usize) {
        let mut state = self.state.lock();
        state.ready_queue.remove(|thread| thread.id() == to_kill_id);
    }
}

impl Display for Scheduler {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let state = self.state.lock();
        let active = state.active_thread.as_ref().unwrap();

        write!(f, "active: {}, ready: {}", active, state.ready_queue)
    }
}