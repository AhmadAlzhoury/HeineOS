/*
 * A driver for the programmable interval timer (PIT).
 *
 * Author: Michael Schoettner, Heinrich Heine University Duesseldorf, 2023-06-15
 *         Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-15
 * License: GPLv3
 */

use alloc::boxed::Box;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::device::cpu::IoPort;
use crate::device::framebuffer;
use crate::device::pic::{Irq, PIC};
use crate::device::terminal;
use crate::interrupt::dispatcher::{IntVectors, InterruptVector};
use crate::interrupt::isr::ISR;
use crate::library::once::Once;
use crate::thread::scheduler::scheduler;

/// Get the current system time in milliseconds.
pub fn system_time() -> usize {
    SYSTEM_TIME.load(Ordering::Relaxed)
}

/// Wait for a specified number of milliseconds using the system time.
pub fn wait(ms: usize) {
    let start_time = system_time();
    while system_time().wrapping_sub(start_time) < ms {
        core::hint::spin_loop();
    }
}

#[repr(u16)]
/// I/O port addresses for the PIT.
enum PitRegister {
    Control = 0x43,
    Data = 0x40,
}

/// Frequency of the timer in Hz
const TIMER_FREQUENCY: usize = 1193182;

/// Nanoseconds that pass per timer tick
const NANOSECONDS_PER_TICK: usize = 1_000_000_000 / TIMER_FREQUENCY;

/// The interval at which the timer should generate interrupts (1 ms).
const TIMER_INTERRUPT_INTERVAL_MS: usize = 1;

/// Global timer instance
static TIMER: Once<Timer> = Once::new();

/// System time in milliseconds.
/// This variable is updated by the timer interrupt service routine.
static SYSTEM_TIME: AtomicUsize = AtomicUsize::new(0);

/// Characters used for the spinner animation.
static SPINNER_CHARS: &[char] = &['|', '/', '-', '\\'];

/// Register the timer interrupt handler.
pub fn plugin() {
    TIMER.init(|| {
        let mut timer = Timer::new();
        timer.set_interrupt_interval(TIMER_INTERRUPT_INTERVAL_MS);
        timer
    });

    IntVectors::register(
        InterruptVector::Pit,
        Box::new(TimerISR {
            interval_ms: TIMER_INTERRUPT_INTERVAL_MS,
        }),
    );
    PIC.lock().allow(Irq::Timer);
}

/// Represents the programmable interval timer.
struct Timer {
    control_port: IoPort,
    data_port0: IoPort,
}

/// The timer interrupt service routine.
struct TimerISR {
    interval_ms: usize,
}

impl TimerISR {
    /// Draw the current system time and spinner in the upper-right corner.
    /// If the framebuffer is already locked, the display update is skipped.
    fn draw_time_display(system_time: usize) {
        const SPINNER_INTERVAL_MS: usize = 250;
        const TIME_DIGITS: usize = 10;
        const STATUS_CHARS: usize = 21;

        let Some(mut framebuffer) = terminal::framebuffer().try_lock() else {
            return;
        };

        let status_width = STATUS_CHARS * framebuffer::CHAR_WIDTH;
        if status_width > framebuffer.width() {
            return;
        }

        let spinner_index = (system_time / SPINNER_INTERVAL_MS - 1) % SPINNER_CHARS.len();
        let mut x = framebuffer.width() - status_width;

        framebuffer.draw_str(
            "Time: ",
            x,
            0,
            framebuffer::WHITE,
            framebuffer::BLUE,
        );
        x += 6 * framebuffer::CHAR_WIDTH;

        let mut divisor = 1_000_000_000usize;
        for _ in 0..TIME_DIGITS {
            let digit = (system_time / divisor) % 10;
            framebuffer.draw_char(
                (b'0' + digit as u8) as char,
                x,
                0,
                framebuffer::WHITE,
                framebuffer::BLUE,
            );
            x += framebuffer::CHAR_WIDTH;
            divisor /= 10;
        }

        framebuffer.draw_str(
            " ms ",
            x,
            0,
            framebuffer::WHITE,
            framebuffer::BLUE,
        );
        x += 4 * framebuffer::CHAR_WIDTH;

        framebuffer.draw_char(
            SPINNER_CHARS[spinner_index],
            x,
            0,
            framebuffer::YELLOW,
            framebuffer::BLUE,
        );
    }
}

impl ISR for TimerISR {
    /// Handle the timer interrupt.
    /// This function updates the system time and redraws the time display every 250 ms.
    fn trigger(&self) {
        const SPINNER_INTERVAL_MS: usize = 250;
        const SCHEDULER_INTERVAL_MS: usize = 10;

        let system_time = SYSTEM_TIME.fetch_add(self.interval_ms, Ordering::Relaxed) + self.interval_ms;
        if system_time % SPINNER_INTERVAL_MS == 0 {
            Self::draw_time_display(system_time);
        }

        if system_time % SCHEDULER_INTERVAL_MS == 0 {
            // The interrupt dispatcher holds INT_VECTORS while invoking this ISR.
            // A context switch may leave this handler suspended, so release that
            // lock before switching to a different thread.
            unsafe {
                crate::interrupt::dispatcher::unlock_int_vectors();
            }
            scheduler().yield_cpu();
        }
    }
}

impl Timer {
    /// Create a new Timer instance.
    pub const fn new() -> Timer {
        Timer {
            control_port: IoPort::new(PitRegister::Control as u16),
            data_port0: IoPort::new(PitRegister::Data as u16),
        }
    }

    /// Set the timer interrupt interval in milliseconds.
    pub fn set_interrupt_interval(&mut self, interval_ms: usize) {
        let counter = ((interval_ms * 1_000_000) / NANOSECONDS_PER_TICK)
            .clamp(1, u16::MAX as usize) as u16;

        unsafe {
            // Counter 0, low byte followed by high byte, mode 3, binary counter.
            self.control_port.outb(0x36);
            self.data_port0.outb((counter & 0xff) as u8);
            self.data_port0.outb((counter >> 8) as u8);
        }
    }
}
