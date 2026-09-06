/*
 * Machine control: shutting the system down and rebooting it.
 *
 * This module keeps the low level details of powering off and resetting an x86
 * machine in one place. The shell only calls `shutdown()` and `reboot()` and does
 * not know anything about I/O ports, so no magic port numbers are spread over the
 * command implementations.
 *
 * Both functions never return. If the hardware does not react, they fall back to
 * a halt loop instead of letting the kernel continue in an undefined state.
 *
 * License: GPLv3
 */

use crate::device::cpu;
use crate::device::cpu::IoPort;
use crate::device::keyboard;

/// ACPI PM1a control registers used to request a soft power off.
///
/// The address of this register depends on the emulated chipset, and the firmware
/// would normally report it in the ACPI tables. HeineOS has no ACPI parser, so the
/// well known addresses of the common QEMU machine types are tried in order:
///   - `0x604`  q35 and modern PIIX (the machine type used by this project)
///   - `0xb004` older PIIX4 machines and Bochs
///   - `0x4004` the `virt` and `microvm` machine types
///
/// Writing to an I/O port that no device listens on has no effect on x86, so
/// trying several addresses is harmless.
const ACPI_SHUTDOWN_PORTS: [u16; 3] = [0x604, 0xb004, 0x4004];

/// Value written to the PM1a control register to power the machine off.
///
/// Bit 13 is `SLP_EN`, which starts the transition, and the sleep type in bits
/// 10 to 12 is left at zero, which QEMU interprets as "soft power off" (S5).
const ACPI_SLEEP_COMMAND: u16 = 0x2000;

/// Port of the PCI reset control register, the fallback for rebooting.
const RESET_CONTROL_PORT: u16 = 0xcf9;

/// Value written to the reset control register: request a system reset (bit 1)
/// and trigger it (bit 2), with the hard reset bit (bit 3) set.
const RESET_CONTROL_COMMAND: u8 = 0x0e;

/// Shut the machine down.
///
/// Requests an ACPI soft power off, which makes QEMU terminate. On real hardware
/// without a matching ACPI configuration, the machine is halted instead.
pub fn shutdown() -> ! {
    // From here on, nothing may interrupt the shutdown anymore.
    cpu::disable_int();

    for address in ACPI_SHUTDOWN_PORTS {
        let mut port = IoPort::new(address);

        // SAFETY: Writing to an I/O port is unsafe because it talks to hardware.
        // Here it either powers the machine off or, if no ACPI controller listens
        // on this address, has no effect at all. No memory is accessed.
        unsafe {
            port.outw(ACPI_SLEEP_COMMAND);
        }
    }

    halt_forever()
}

/// Reboot the machine.
///
/// First the classic keyboard controller reset is used, because it is supported
/// by every PC compatible machine. If that does not reset the machine, the PCI
/// reset control register is used as a fallback.
pub fn reboot() -> ! {
    cpu::disable_int();

    keyboard::reset_cpu();

    let mut port = IoPort::new(RESET_CONTROL_PORT);

    // SAFETY: This write resets the machine. Interrupts are already disabled and
    // no kernel data is touched, so nothing can be left in an inconsistent state.
    unsafe {
        port.outb(RESET_CONTROL_COMMAND);
    }

    halt_forever()
}

/// Stop the CPU permanently.
///
/// This is the fallback when neither shutdown nor reset works. Halting is safer
/// than returning to the caller, because the state of the machine is unclear
/// after a failed attempt.
fn halt_forever() -> ! {
    loop {
        cpu::halt();
    }
}
