/*
 * The command implementations of the HeineOS shell.
 *
 * The commands are grouped by the subsystem they use:
 *   - `general`:    commands of the shell itself (help, clear, echo, about, history, demo)
 *   - `system`:     time, uptime, memory, framebuffer and the system overview
 *   - `threads`:    information about the scheduler
 *   - `filesystem`: read-only access to the TarFs filesystem
 *   - `hardware`:   PCI bus and PC speaker
 *
 * This file only dispatches a parsed `Command` to the matching function.
 *
 * License: GPLv3
 */

mod filesystem;
mod general;
mod hardware;
mod system;
mod threads;

use crate::shell::history::History;
use crate::shell::parser::Command;

/// Execute a parsed command.
///
/// The history is passed in because the `history` command prints it. All other
/// commands ignore it, and none of them may modify it.
pub fn execute(command: Command, history: &History) {
    match command {
        Command::Help(name) => general::help(name),
        Command::Clear => general::clear(),
        Command::Echo(text) => general::echo(text),
        Command::About => general::about(),
        Command::History => general::history(history),
        Command::Demo => general::demo(),
        Command::Time => system::time(),
        Command::MemInfo => system::meminfo(),
        Command::SysInfo => system::sysinfo(),
        Command::FbInfo => system::fbinfo(),
        Command::Sleep(argument) => system::sleep(argument),
        Command::Shutdown => system::shutdown(),
        Command::Reboot => system::reboot(),
        Command::Ps => threads::ps(),
        Command::Tid => threads::tid(),
        Command::Ls(path) => filesystem::ls(path),
        Command::Cat(path) => filesystem::cat(path),
        Command::Stat(path) => filesystem::stat(path),
        Command::Hexdump(path) => filesystem::hexdump(path),
        Command::LsPci(options) => hardware::lspci(options),
        Command::Beep(arguments) => hardware::beep(arguments),
        Command::Unknown(name) => general::unknown(name),
        // An empty line is not an error, the shell just shows a new prompt.
        Command::Empty => {}
    }
}
