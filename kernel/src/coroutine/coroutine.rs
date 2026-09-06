/*
 * Contains functions to create, start, switch and end coroutines.
 *
 * Author: Michael Schoettner, Heinrich Heine University Duesseldorf, 2023-05-15
 *         Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-15
 * License: GPLv3
 */

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::arch::naked_asm;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::consts;
use crate::device::cpu;

/// Atomic counter for coroutine ids.
static COROUTINE_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Generate a new coroutine id by incrementing the counter.
fn next_id() -> usize {
    COROUTINE_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Low-level routine for starting a coroutine.
#[unsafe(naked)]
unsafe extern "C" fn coroutine_start(stack_ptr: usize) {
    naked_asm!(
        "mov rsp, rdi",
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

/// Low-level routine for switching to the next coroutine.
/// `current_stack_ptr` is a pointer to `stack_ptr` of the current coroutine (where the rsp is saved).
/// `next_stack` is the value of `stack_ptr` of the next coroutine (the new rsp value).
#[unsafe(naked)]
unsafe extern "C" fn coroutine_switch(current_stack_ptr: *mut usize, next_stack: usize) {
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

/// Represents a coroutine in the system.
/// It contains the stack, the entry function, and a pointer to the next coroutine.
/// Coroutines must be chained via `set_next()` and form a circular linked list.
/// To start the coroutine, use `start()`. Once started, coroutines cannot be exited
/// and the entry function must not return.
pub struct Coroutine {
    id: usize,
    stack: Vec<u64>,  // Memory for the stack
    stack_ptr: usize, // Pointer on the stack to the saved context
    entry: fn(&mut Coroutine),
    next: *mut Coroutine,
}

impl Coroutine {
    /// Create a new coroutine with the given entry function.
    pub fn new(entry: fn(&mut Coroutine)) -> Box<Coroutine> {
        let mut stack = Vec::<u64>::with_capacity(consts::STACK_SIZE / 8);
        for _ in 0..stack.capacity() {
            stack.push(0);
        }

        let stack_ptr = ptr::from_ref(&stack[stack.capacity() - 1]) as usize;

        let mut coroutine = Box::new(
            Coroutine { id: next_id(), stack, stack_ptr, entry, next: ptr::null_mut() }
        );

        coroutine.prepare_stack();
        coroutine
    }

    /// Start the coroutine.
    /// Once started, coroutines cannot be exited.
    /// May only be called once.
    pub fn start(&mut self) {
        assert!(!self.next.is_null(), "Coroutine {} has no successor", self.id);
        unsafe {
            coroutine_start(self.stack_ptr);
        }
    }

    /// Switch from the current stack to this coroutine while saving the caller context.
    /// A coroutine can later use `switch_to()` to resume the caller.
    pub fn start_resumable(&mut self, caller: &mut Coroutine) {
        assert!(!self.next.is_null(), "Coroutine {} has no successor", self.id);
        unsafe {
            coroutine_switch(&mut caller.stack_ptr, self.stack_ptr);
        }
    }

    /// Switch to the next coroutine.
    pub fn switch(&mut self) {
        assert!(!self.next.is_null(), "Coroutine {} has no successor", self.id);
        unsafe {
            coroutine_switch(&mut self.stack_ptr, (*self.next).stack_ptr);
        }
    }

    /// Switch directly to another coroutine instead of following the configured successor.
    pub fn switch_to(&mut self, next: &mut Coroutine) {
        unsafe {
            coroutine_switch(&mut self.stack_ptr, next.stack_ptr);
        }
    }

    /// Get the id of the coroutine.
    pub fn id(&self) -> usize {
        self.id
    }

    /// Set the next coroutine.
    pub fn set_next(&mut self, next: &mut Coroutine) {
        self.next = ptr::from_mut(next);
    }

    /// Prepare the stack of a newly created coroutine in a way that it can be used
    /// to return to the 'kickoff' function with the coroutine itself as parameter.
    /// The prepared stack is used in 'coroutine_start' to start the first coroutine.
    /// Other coroutines are started by 'coroutine_switch' with the prepared stack.
    fn prepare_stack(&mut self) {
        let kickoff = (Coroutine::kickoff as *const ()) as u64;
        let coroutine = ptr::from_mut(self) as u64;
        let top = self.aligned_return_slot();

        self.stack[top] = kickoff; // Address of 'kickoff' -> Used as return address
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
        self.stack[top - 14] = coroutine; // rdi -> First parameter for 'kickoff'
        self.stack[top - 15] = 0; // rbp
        self.stack[top - 16] = 0x2; // rflags (IE = 0); interrupts disabled

        // The context above is restored by 16 pops, so the coroutine starts with
        // `rsp` pointing at the saved rflags.
        let stack_ptr = ptr::from_ref(&self.stack[top - 16]) as usize;
        self.stack_ptr = stack_ptr;
    }

    /// Index of the stack slot that holds the return address into `kickoff`.
    ///
    /// See `Thread::aligned_return_slot()` for the reasoning: the x86-64 System V ABI
    /// requires that `kickoff` is entered with `rsp % 16 == 8`, and since `kickoff` is
    /// reached through `ret`, the slot holding its address must be 16-byte aligned.
    /// The kernel heap only guarantees 8-byte alignment for the stack buffer, so the
    /// topmost slot is skipped when it does not satisfy that.
    fn aligned_return_slot(&self) -> usize {
        let slot = self.stack.len() - 2;
        let address = ptr::from_ref(&self.stack[slot]) as usize;

        if address % 16 == 0 { slot } else { slot - 1 }
    }

    /// Called indirectly by using the prepared stack in 'coroutine_start' and 'coroutine_switch'.
    fn kickoff(&mut self) {
        // Interrupts are disabled during coroutine start, so we need to re-enable them here
        cpu::enable_int();
        (self.entry)(self);

        panic!("Coroutine {} finished!", self.id());
    }
}
