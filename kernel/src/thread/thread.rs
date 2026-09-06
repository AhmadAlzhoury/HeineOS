/*
 * Contains functions to create, start, switch and end threads.
 *
 * Author: Michael Schoettner, Heinrich Heine University Duesseldorf, 2023-05-15
 *         Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-15
 * License: GPLv3
 */

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::{fmt, ptr};
use core::arch::naked_asm;
use core::fmt::Display;
use core::sync::atomic::AtomicUsize;
use crate::consts::STACK_SIZE;
use crate::device::cpu;
use crate::thread::scheduler::scheduler;

static THREAD_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn next_id() -> usize {
    THREAD_ID_COUNTER.fetch_add(1, core::sync::atomic::Ordering::SeqCst)
}

/// Low-level routine for starting a thread.
#[unsafe(naked)]
unsafe extern "C" fn thread_start(stack_ptr: usize) {
    naked_asm!(
        "mov rsp, rdi",
        "mov r12, rsp",
        "and rsp, -16",
        "call unlock_scheduler",
        "mov rsp, r12",
        "popfq",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "ret"
    )
}

/// Low-level routine for switching to the next thread.
/// `current_stack_ptr` is a pointer to `stack_ptr` of the next coroutine (where the rsp is saved).
/// `next_stack` is the value of `stack_ptr` of the next thread (the new rsp value).
#[unsafe(naked)]
unsafe extern "C" fn thread_switch(current_stack_ptr: *mut usize, next_stack: usize) {
    naked_asm!(
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "pushfq",
        "mov [rdi], rsp",
        "mov rsp, rsi",
        "mov r12, rsp",
        "and rsp, -16",
        "call unlock_scheduler",
        "mov rsp, r12",
        "popfq",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "ret"
    )
}

/// Represents a thread in the system.
/// It contains the stack and the entry function.
/// Threads must be registered in the scheduler and are run automatically once the scheduler is started.
#[repr(C)]
pub struct Thread {
    id: usize,
    stack: Vec<u64>,  // Memory for the stack
    stack_ptr: usize, // Pointer on the stack to the saved context
    entry: fn(),
}

impl Thread {
    /// Create a new thread with the given entry function.
    pub fn new(entry: fn()) -> Box<Thread> {
        // Allocate memory for the stack and initialize it to zero
        let mut stack = Vec::<u64>::with_capacity(STACK_SIZE / 8);
        for _ in 0..stack.capacity() {
            stack.push(0);
        }

        // Set the stack pointer to the top of the stack
        let stack_ptr = ptr::from_ref(&stack[stack.capacity() - 1]) as usize;

        // Create a new thread object
        let mut thread = Box::new(
            Thread { id: next_id(), stack, stack_ptr, entry }
        );

        // Prepare the stack for the thread so it can be started via `thread_start()`
        thread.prepare_stack();
        thread
    }

    /// Start the thread.
    /// This function is only once by the scheduler.
    /// The scheduler does further thread switching via `switch()`.
    pub fn start(&mut self) {
        unsafe {
            thread_start(self.stack_ptr);
        }
    }

    /// Switch from the `current` thread to the `next` thread.
    /// This function is called by the scheduler to switch between threads.
    pub unsafe fn switch(current: *mut Thread, next: *mut Thread) {
        unsafe {
            thread_switch(&mut (*current).stack_ptr, (*next).stack_ptr);
        }
    }

    /// Get the ID of the thread.
    pub fn id(&self) -> usize {
        self.id
    }

    /// Prepare the stack of a newly created thread in a way that it can be used
    /// to return to the 'kickoff' function with the thread itself as parameter.
    /// The prepared stack is used in 'thread_start' to start the first thread.
    /// Other threads are started by 'thread_switch' with the prepared stack.
    fn prepare_stack(&mut self) {
        let kickoff = (Thread::kickoff as *const ()) as u64;
        let thread = ptr::from_mut(self) as u64;
        let top = self.aligned_return_slot();

        self.stack[top] = kickoff; // Address of 'kickoff'
        self.stack[top - 1] = 0; // r8
        self.stack[top - 2] = 0; // r9
        self.stack[top - 3] = 0; // r10
        self.stack[top - 4] = 0; // r11
        self.stack[top - 5] = 0; // r12
        self.stack[top - 6] = 0; // r13
        self.stack[top - 7] = 0; // r14
        self.stack[top - 8] = 0; // r15
        self.stack[top - 9] = 0; // rax
        self.stack[top - 10] = 0; // rbx
        self.stack[top - 11] = 0; // rcx
        self.stack[top - 12] = 0; // rdx
        self.stack[top - 13] = 0; // rsi
        self.stack[top - 14] = thread; // rdi -> First parameter for 'kickoff'
        self.stack[top - 15] = 0; // rbp
        self.stack[top - 16] = 0x2; // rflags (IE = 0); interrupts disabled

        // The context above is restored by 16 pops, so the thread starts with
        // `rsp` pointing at the saved rflags.
        let stack_ptr = ptr::from_ref(&self.stack[top - 16]) as usize;
        self.stack_ptr = stack_ptr;
    }

    /// Index of the stack slot that holds the return address into `kickoff`.
    ///
    /// The x86-64 System V ABI requires `rsp` to be 16-byte aligned before a `call`,
    /// so a function finds `rsp % 16 == 8` on entry. `thread_start` and `thread_switch`
    /// enter `kickoff` with `ret`, which pops the return address, so the slot holding
    /// that address must itself lie on a 16-byte boundary.
    ///
    /// The stack is a `Vec<u64>` and the kernel heap only guarantees 8-byte alignment,
    /// so the top of the stack is not necessarily 16-byte aligned. When it is not, the
    /// topmost usable slot is skipped and the next one down is used instead.
    ///
    /// This is not a cosmetic detail: code that spills SSE registers with aligned moves
    /// (`movaps`) raises a general protection fault on a misaligned stack. The kernel
    /// itself is compiled without SSE, but the C code of the Peanut-GB demo is not.
    fn aligned_return_slot(&self) -> usize {
        let slot = self.stack.len() - 2;
        let address = ptr::from_ref(&self.stack[slot]) as usize;

        if address % 16 == 0 { slot } else { slot - 1 }
    }

    /// Called indirectly by using the prepared stack in 'thread_start' and 'thread_switch'.
    fn kickoff(&self) {
        // Interrupts are disabled during thread start, so we need to re-enable them here
        cpu::enable_int();
        ((*self).entry)();

        scheduler().exit();
    }
}

impl Display for Thread {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "T{}", self.id)
    }
}