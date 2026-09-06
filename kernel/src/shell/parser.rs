/*
 * Parser for the HeineOS shell.
 *
 * The parser splits an input line into a command name and its arguments and maps
 * the name to a `Command` variant. It works entirely on borrowed string slices,
 * so parsing does not allocate any memory on the heap.
 *
 * The parser deliberately does not validate the arguments. Whether an argument
 * is a valid number or a known option is decided by the command implementation,
 * which can also print a useful error message for it.
 *
 * License: GPLv3
 */

/// A parsed shell command.
///
/// The lifetime parameter ties the borrowed arguments to the input line, which
/// avoids copying them onto the heap.
#[derive(Debug)]
pub enum Command<'a> {
    /// Show the list of available commands, or the help of a single command.
    /// The argument is empty for the overview.
    Help(&'a str),
    /// Clear the terminal.
    Clear,
    /// Print the given text.
    Echo(&'a str),
    /// Show information about HeineOS.
    About,
    /// Show the stored command history.
    History,
    /// Show the system uptime.
    Time,
    /// Show information about the kernel heap.
    MemInfo,
    /// Show a summary of the whole system.
    SysInfo,
    /// Show information about the framebuffer and the terminal.
    FbInfo,
    /// Wait for the given number of milliseconds.
    Sleep(&'a str),
    /// List the threads known to the scheduler.
    Ps,
    /// Show the ID of the current thread.
    Tid,
    /// List the contents of a directory. The argument may be empty.
    Ls(&'a str),
    /// Print the contents of a file.
    Cat(&'a str),
    /// Show the metadata of a file or directory.
    Stat(&'a str),
    /// Show the contents of a file in hexadecimal.
    Hexdump(&'a str),
    /// List the devices on the PCI bus. The argument holds the options.
    LsPci(&'a str),
    /// Play a tone on the PC speaker. The argument holds frequency and duration.
    Beep(&'a str),
    /// Shut the machine down.
    Shutdown,
    /// Reboot the machine.
    Reboot,
    /// Open the existing demo menu.
    Demo,
    /// The command name is not known. Contains the name that was entered.
    Unknown(&'a str),
    /// The input line was empty or contained only whitespace.
    Empty,
}

/// Parse a single input line into a `Command`.
///
/// Leading and trailing whitespace is removed and any number of spaces between
/// the command name and its arguments is tolerated. An empty line results in
/// `Command::Empty` instead of an error.
pub fn parse(line: &str) -> Command<'_> {
    let line = line.trim();
    if line.is_empty() {
        return Command::Empty;
    }

    let (name, arguments) = split_command(line);

    match name {
        "help" => Command::Help(arguments),
        "clear" => Command::Clear,
        "echo" => Command::Echo(arguments),
        "about" => Command::About,
        "history" => Command::History,
        // `uptime` is an alias for `time`, since `time` shows the system uptime.
        "time" | "uptime" => Command::Time,
        "meminfo" => Command::MemInfo,
        "sysinfo" => Command::SysInfo,
        "fbinfo" => Command::FbInfo,
        "sleep" => Command::Sleep(arguments),
        "ps" => Command::Ps,
        "tid" => Command::Tid,
        "ls" => Command::Ls(arguments),
        "cat" => Command::Cat(arguments),
        "stat" => Command::Stat(arguments),
        "hexdump" => Command::Hexdump(arguments),
        "lspci" => Command::LsPci(arguments),
        "beep" => Command::Beep(arguments),
        // `poweroff` is an alias for `shutdown`, as on most Unix systems.
        "shutdown" | "poweroff" => Command::Shutdown,
        "reboot" => Command::Reboot,
        "demo" => Command::Demo,
        _ => Command::Unknown(name),
    }
}

/// Split a trimmed line into the command name and the remaining argument text.
///
/// The argument text keeps its internal spacing, but leading whitespace is
/// removed, so `echo   hello  world` yields the arguments `hello  world`.
fn split_command(line: &str) -> (&str, &str) {
    match line.split_once(char::is_whitespace) {
        Some((name, arguments)) => (name, arguments.trim_start()),
        None => (line, ""),
    }
}
