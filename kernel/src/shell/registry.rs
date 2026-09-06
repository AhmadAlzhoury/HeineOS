/*
 * The command metadata of the HeineOS shell.
 *
 * This module contains the single table of all commands. It is used by three
 * different parts of the shell:
 *   - `help` prints an overview generated from this table
 *   - `help <command>` prints the detailed help text stored here
 *   - Tab completion searches this table for command names
 *
 * Keeping the metadata in one place means that a new command has to be described
 * exactly once.
 *
 * License: GPLv3
 */

use alloc::string::String;
use alloc::vec::Vec;

/// Description of a single shell command.
pub struct CommandInfo {
    /// The name of the command, as it has to be typed. Used for Tab completion.
    pub name: &'static str,
    /// The category the command belongs to, used to group the `help` output.
    pub category: &'static str,
    /// How the command is used, e.g. `echo <text>`.
    pub usage: &'static str,
    /// A short, single line description for the `help` overview.
    pub description: &'static str,
    /// The detailed help text, printed line by line by `help <command>`.
    pub help: &'static [&'static str],
}

/// All commands supported by the shell, grouped by category.
///
/// The order of this table defines the order of the `help` output, so the
/// entries of one category have to stay next to each other.
pub const COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        name: "help",
        category: "General",
        usage: "help [command]",
        description: "Show available commands",
        help: &[
            "help - Show available commands",
            "",
            "Usage:",
            "  help            List all commands with a short description",
            "  help <command>  Show the detailed help of a single command",
        ],
    },
    CommandInfo {
        name: "clear",
        category: "General",
        usage: "clear",
        description: "Clear the terminal",
        help: &[
            "clear - Clear the terminal",
            "",
            "Usage:",
            "  clear",
            "",
            "The next prompt appears at the top of the screen.",
        ],
    },
    CommandInfo {
        name: "echo",
        category: "General",
        usage: "echo <text>",
        description: "Print text",
        help: &[
            "echo - Print text",
            "",
            "Usage:",
            "  echo <text>",
            "",
            "Everything after the command name is printed unchanged.",
            "Without any argument, an empty line is printed.",
        ],
    },
    CommandInfo {
        name: "about",
        category: "General",
        usage: "about",
        description: "Show information about HeineOS",
        help: &[
            "about - Show information about HeineOS",
            "",
            "Usage:",
            "  about",
        ],
    },
    CommandInfo {
        name: "history",
        category: "General",
        usage: "history",
        description: "Show previously entered commands",
        help: &[
            "history - Show previously entered commands",
            "",
            "Usage:",
            "  history",
            "",
            "The history keeps a limited number of commands. Empty lines and",
            "repetitions of the previous command are not stored.",
            "Use the Up and Down arrow keys to recall an entry at the prompt.",
        ],
    },
    CommandInfo {
        name: "time",
        category: "System",
        usage: "time",
        description: "Show system uptime (alias: uptime)",
        help: &[
            "time - Show the system uptime",
            "",
            "Usage:",
            "  time",
            "  uptime",
            "",
            "The uptime is measured by the PIT since the kernel was started and",
            "is printed as HH:MM:SS.mmm. HeineOS has no real time clock driver,",
            "so this is not the time of day.",
        ],
    },
    CommandInfo {
        name: "meminfo",
        category: "System",
        usage: "meminfo",
        description: "Show kernel heap information",
        help: &[
            "meminfo - Show information about the kernel heap",
            "",
            "Usage:",
            "  meminfo",
            "",
            "The values are read from the linked list allocator and describe the",
            "kernel heap only. HeineOS has no physical memory manager yet.",
        ],
    },
    CommandInfo {
        name: "sysinfo",
        category: "System",
        usage: "sysinfo",
        description: "Show an overview of the system",
        help: &[
            "sysinfo - Show an overview of the system",
            "",
            "Usage:",
            "  sysinfo",
            "",
            "Combines the information of time, meminfo, fbinfo, tid and lspci",
            "into a single summary.",
        ],
    },
    CommandInfo {
        name: "fbinfo",
        category: "System",
        usage: "fbinfo",
        description: "Show framebuffer information",
        help: &[
            "fbinfo - Show information about the framebuffer",
            "",
            "Usage:",
            "  fbinfo",
            "",
            "Shows the resolution reported by the bootloader, the size of a",
            "character cell and the resulting size of the terminal.",
        ],
    },
    CommandInfo {
        name: "sleep",
        category: "System",
        usage: "sleep <ms>",
        description: "Wait for the given number of milliseconds",
        help: &[
            "sleep - Wait for a given amount of time",
            "",
            "Usage:",
            "  sleep <milliseconds>",
            "",
            "Waits using the PIT. While waiting, the shell thread yields the CPU,",
            "so other threads keep running. The maximum duration is 10000 ms.",
        ],
    },
    CommandInfo {
        name: "ps",
        category: "Threads",
        usage: "ps",
        description: "List the threads known to the scheduler",
        help: &[
            "ps - List the threads known to the scheduler",
            "",
            "Usage:",
            "  ps",
            "",
            "Shows the running thread and the threads waiting in the ready queue.",
            "The round robin scheduler of HeineOS has no other thread states.",
            "Reading the list does not change the scheduler state.",
        ],
    },
    CommandInfo {
        name: "tid",
        category: "Threads",
        usage: "tid",
        description: "Show the ID of the current thread",
        help: &[
            "tid - Show the ID of the current thread",
            "",
            "Usage:",
            "  tid",
            "",
            "This is the ID of the thread running the shell.",
        ],
    },
    CommandInfo {
        name: "ls",
        category: "Filesystem",
        usage: "ls [path]",
        description: "List the contents of a directory",
        help: &[
            "ls - List the contents of a directory",
            "",
            "Usage:",
            "  ls          List the root directory",
            "  ls <path>   List the given directory",
            "",
            "Directories are marked with a trailing slash.",
        ],
    },
    CommandInfo {
        name: "cat",
        category: "Filesystem",
        usage: "cat <file>",
        description: "Print the contents of a file",
        help: &[
            "cat - Print the contents of a file",
            "",
            "Usage:",
            "  cat <file>",
            "",
            "Files that are not valid UTF-8 are printed as text anyway, with all",
            "non-printable bytes replaced by a dot. Use hexdump for binary files.",
        ],
    },
    CommandInfo {
        name: "stat",
        category: "Filesystem",
        usage: "stat <path>",
        description: "Show metadata of a file or directory",
        help: &[
            "stat - Show the metadata of a file or directory",
            "",
            "Usage:",
            "  stat <path>",
            "",
            "A tar archive only stores the name, the type and the size of an",
            "entry, so no other metadata can be shown.",
        ],
    },
    CommandInfo {
        name: "hexdump",
        category: "Filesystem",
        usage: "hexdump <file>",
        description: "Show the contents of a file in hexadecimal",
        help: &[
            "hexdump - Show the contents of a file in hexadecimal",
            "",
            "Usage:",
            "  hexdump <file>",
            "",
            "Prints an offset column followed by 16 bytes per line. Only the",
            "beginning of a large file is shown, which is stated in the output.",
        ],
    },
    CommandInfo {
        name: "lspci",
        category: "Hardware",
        usage: "lspci [-v]",
        description: "List the devices on the PCI bus",
        help: &[
            "lspci - List PCI devices",
            "",
            "Usage:",
            "  lspci",
            "  lspci -v",
            "",
            "Options:",
            "  -v    Show detailed information about every device",
            "",
            "The devices are the ones found during the PCI scan at boot time.",
        ],
    },
    CommandInfo {
        name: "beep",
        category: "Hardware",
        usage: "beep <hz> <ms>",
        description: "Play a tone on the PC speaker",
        help: &[
            "beep - Play a tone on the PC speaker",
            "",
            "Usage:",
            "  beep <frequency> <duration>",
            "",
            "The frequency is given in hertz (20 - 20000) and the duration in",
            "milliseconds (1 - 5000). The shell blocks until the tone has ended.",
        ],
    },
    CommandInfo {
        name: "shutdown",
        category: "System control",
        usage: "shutdown",
        description: "Shut down HeineOS (alias: poweroff)",
        help: &[
            "shutdown - Shut down HeineOS",
            "",
            "Usage:",
            "  shutdown",
            "  poweroff",
            "",
            "Requests an ACPI soft power off, which makes the virtual machine",
            "terminate. If the machine does not react, the CPU is halted.",
        ],
    },
    CommandInfo {
        name: "reboot",
        category: "System control",
        usage: "reboot",
        description: "Reboot HeineOS",
        help: &[
            "reboot - Reboot HeineOS",
            "",
            "Usage:",
            "  reboot",
            "",
            "Resets the machine through the keyboard controller, which is the",
            "classic way to reboot a PC. HeineOS starts again from the bootloader.",
        ],
    },
    CommandInfo {
        name: "demo",
        category: "Applications",
        usage: "demo",
        description: "Open the Demo Menu",
        help: &[
            "demo - Open the Demo Menu",
            "",
            "Usage:",
            "  demo",
            "",
            "Controls inside the menu:",
            "  Up/Down   Select an entry",
            "  Enter     Start the selected demo",
            "  Ctrl+C    Return to the shell",
            "",
            "A running demo is left with Esc, which returns to the Demo Menu.",
        ],
    },
];

/// Description of a keyboard shortcut of the interactive line editor.
pub struct ShortcutInfo {
    /// The keys that have to be pressed, e.g. `Ctrl+L`.
    pub keys: &'static str,
    /// What the shortcut does.
    pub description: &'static str,
}

/// The keyboard shortcuts of the shell, listed at the end of the `help` output.
///
/// They are kept here next to the commands, so that the shortcuts are described
/// in exactly one place as well.
pub const SHORTCUTS: &[ShortcutInfo] = &[
    ShortcutInfo {
        keys: "Backspace",
        description: "Delete the previous character",
    },
    ShortcutInfo {
        keys: "Up/Down",
        description: "Walk through the command history",
    },
    ShortcutInfo {
        keys: "Tab",
        description: "Complete the command name",
    },
    ShortcutInfo {
        keys: "Ctrl+C",
        description: "Cancel the current input line",
    },
    ShortcutInfo {
        keys: "Ctrl+L",
        description: "Clear the screen, keep the input",
    },
    ShortcutInfo {
        keys: "Ctrl+U",
        description: "Clear the current input line",
    },
    ShortcutInfo {
        keys: "Ctrl+Up/Down",
        description: "Scroll through older output",
    },
    ShortcutInfo {
        keys: "Ctrl+PgUp/PgDn",
        description: "Scroll by a whole page",
    },
];

/// Look up a command by its exact name.
pub fn find(name: &str) -> Option<&'static CommandInfo> {
    COMMANDS.iter().find(|command| command.name == name)
}

/// Collect all commands whose name starts with the given prefix.
///
/// The result is returned as a vector instead of an iterator, so that the
/// borrow of the prefix ends when this function returns. This keeps the calling
/// code in the line editor free of borrowing problems.
pub fn completions(prefix: &str) -> Vec<&'static CommandInfo> {
    COMMANDS
        .iter()
        .filter(|command| command.name.starts_with(prefix))
        .collect()
}

/// Build the longest prefix that all given commands have in common.
///
/// Tab completion uses this to complete as far as possible when several
/// commands match, e.g. `st` and `sy` both start with `s`.
pub fn longest_common_prefix(commands: &[&'static CommandInfo]) -> String {
    let Some((first, rest)) = commands.split_first() else {
        return String::new();
    };

    let mut length = first.name.len();
    for command in rest {
        length = common_prefix_length(first.name, command.name).min(length);
    }

    String::from(&first.name[..length])
}

/// Get the number of leading bytes that two command names have in common.
/// Command names are plain ASCII, so counting bytes is safe here.
fn common_prefix_length(first: &str, second: &str) -> usize {
    first
        .bytes()
        .zip(second.bytes())
        .take_while(|(left, right)| left == right)
        .count()
}
