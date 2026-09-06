/*
 * Hardware commands: the PCI bus and the PC speaker.
 *
 * Both commands use the existing drivers. The shell does not access the PCI
 * configuration space or the speaker ports itself.
 *
 * License: GPLv3
 */

use crate::device::pci::{self, pci_bus, PciDevice};
use crate::device::speaker::{self, SPEAKER};

/// Number of base address registers of a PCI device.
const BAR_COUNT: u8 = 6;

/// Lowest frequency accepted by `beep`, in hertz.
const MIN_FREQUENCY: usize = 20;
/// Highest frequency accepted by `beep`, in hertz.
const MAX_FREQUENCY: usize = 20000;
/// Longest tone accepted by `beep`, in milliseconds.
const MAX_BEEP_MS: usize = 5000;

/// List the devices that were found during the PCI scan at boot time.
pub fn lspci(options: &str) {
    match options.trim() {
        "" => list_devices(),
        "-v" => list_devices_verbose(),
        _ => print_lspci_usage(),
    }
}

/// Get the number of devices on the PCI bus.
/// Also used by `sysinfo`, so the bus is queried in only one place.
pub fn device_count() -> usize {
    pci_bus().iter().count()
}

/// Print one line per PCI device with its address and its identification.
fn list_devices() {
    println!("");
    println!("  BUS  DEV  FUNC  VENDOR  DEVICE  CLASS  SUBCLASS");

    for device in pci_bus().iter() {
        println!(
            "  {:02x}   {:02x}   {:02x}    {:04x}    {:04x}    {:02x}     {:02x}",
            device.bus(),
            device.device(),
            device.function(),
            device.read_vendor_id(),
            device.read_device_id(),
            device.read_class(),
            device.read_subclass()
        );
    }

    println!("");
    println!("{} device(s).", device_count());
    println!("");
}

/// Print a detailed block of information for every PCI device.
fn list_devices_verbose() {
    for device in pci_bus().iter() {
        println!("");
        print_device_details(device);
    }

    println!("");
}

/// Print the configuration space values of a single device.
fn print_device_details(device: &PciDevice) {
    let class = device.read_class();

    println!(
        "Device {:04x}:{:04x} (bus {:02x}, device {:02x}, function {:02x})",
        device.read_vendor_id(),
        device.read_device_id(),
        device.bus(),
        device.device(),
        device.function()
    );
    println!("  Class:      {:02x} ({})", class, pci::class_name(class));
    println!("  Subclass:   {:02x}", device.read_subclass());
    println!("  Revision:   {:02x}", device.read_revision());
    println!("  Interrupt:  {}", device.read_interrupt_line());

    // Only the base address registers that are actually used are shown.
    for index in 0..BAR_COUNT {
        let address = device.read_bar(index);
        if address != 0 {
            println!("  BAR{}:       0x{:08x}", index, address);
        }
    }
}

/// Print how the `lspci` command is used.
fn print_lspci_usage() {
    println!("Usage:");
    println!("  lspci");
    println!("  lspci -v");
}

/// Play a tone on the PC speaker.
///
/// Both arguments are validated before the speaker is used, so invalid input
/// only produces an error message.
pub fn beep(arguments: &str) {
    let mut tokens = arguments.split_whitespace();
    let (Some(frequency), Some(duration)) = (tokens.next(), tokens.next()) else {
        print_beep_usage();
        return;
    };

    if tokens.next().is_some() {
        print_beep_usage();
        return;
    }

    let Some(frequency) = parse_number(frequency, "frequency", MIN_FREQUENCY, MAX_FREQUENCY) else {
        return;
    };

    let Some(duration) = parse_number(duration, "duration", 1, MAX_BEEP_MS) else {
        return;
    };

    println!("Playing {} Hz for {} ms...", frequency, duration);

    // The demos use this flag to stop a melody early. It is reset here, because
    // a demo that was left with Esc may have left it set.
    speaker::set_cancelled(false);
    SPEAKER.lock().play(frequency, duration);

    println!("Done.");
}

/// Parse and range check one of the numeric arguments of `beep`.
fn parse_number(text: &str, name: &str, minimum: usize, maximum: usize) -> Option<usize> {
    let Ok(value) = text.parse::<usize>() else {
        println!("Invalid {}: '{}'", name, text);
        return None;
    };

    if value < minimum || value > maximum {
        println!("The {} must be between {} and {}.", name, minimum, maximum);
        return None;
    }

    Some(value)
}

/// Print how the `beep` command is used.
fn print_beep_usage() {
    println!("Usage: beep <frequency> <duration_ms>");
}
