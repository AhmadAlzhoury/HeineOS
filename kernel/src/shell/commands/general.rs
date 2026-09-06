/*
 * Commands of the shell itself: help, clear, echo, about, history and demo.
 *
 * License: GPLv3
 */

use crate::demo::menu;
use crate::device::terminal::terminal;
use crate::library::input;
use crate::shell::history::History;
use crate::shell::registry;

/// Column width used to align the descriptions in the `help` overview.
const USAGE_WIDTH: usize = 18;

/// Show the available commands, or the detailed help of a single command.
///
/// An empty argument prints the overview. Everything after the first word of the
/// argument is ignored, so `help ls extra` behaves like `help ls`.
pub fn help(argument: &str) {
    match argument.split_whitespace().next() {
        Some(name) => print_command_help(name),
        None => print_overview(),
    }
}

/// Print all commands, grouped by category.
///
/// The output is generated from the command table, so it can never get out of
/// sync with the commands the shell actually supports.
fn print_overview() {
    let mut current_category = "";

    println!("");
    println!("Available commands:");

    for command in registry::COMMANDS {
        if command.category != current_category {
            current_category = command.category;
            println!("");
            println!("{}:", current_category);
        }

        println!(
            "  {:width$} {}",
            command.usage,
            command.description,
            width = USAGE_WIDTH
        );
    }

    print_shortcuts();

    println!("");
    println!("Type 'help <command>' for more information about a single command.");
    println!("");
}

/// Print the keyboard shortcuts of the line editor.
fn print_shortcuts() {
    println!("");
    println!("Keyboard shortcuts:");

    for shortcut in registry::SHORTCUTS {
        println!(
            "  {:width$} {}",
            shortcut.keys,
            shortcut.description,
            width = USAGE_WIDTH
        );
    }
}

/// Print the detailed help text of a single command.
fn print_command_help(name: &str) {
    let Some(command) = registry::find(name) else {
        println!("No help available for '{}'.", name);
        return;
    };

    println!("");
    for line in command.help {
        println!("{}", line);
    }
    println!("");
}

/// Clear the terminal using the existing terminal driver.
pub fn clear() {
    terminal().lock().clear();
}

/// Print the given text. An empty argument results in an empty line.
pub fn echo(text: &str) {
    println!("{}", text);
}

/// Print a short description of HeineOS.
pub fn about() {
    println!("");
    println!("HeineOS");
    println!("Educational x86-64 operating system written in Rust.");
    println!("Shell developed for Lesson 7 - Operating System Development.");
    println!("");
}

/// Print the stored command history, oldest entry first.
///
/// The shell stores a line before executing it, so this command appears as the
/// last entry of its own output.
pub fn history(history: &History) {
    if history.len() == 0 {
        println!("The history is empty.");
        return;
    }

    println!("");
    for (index, entry) in history.iter().enumerate() {
        println!("  {:3}  {}", index + 1, entry);
    }
    println!("");
}

/// Open the existing demo menu and return to the shell when it exits.
///
/// The menu runs synchronously in the shell thread and returns when the user
/// presses Ctrl+C. Afterwards the keyboard buffer is drained, so that the key
/// events of that combination do not end up in the next shell input line.
pub fn demo() {
    menu::run();

    input::settle_and_drain();
    terminal().lock().clear();
}

/// Report an unknown command without aborting the shell.
pub fn unknown(name: &str) {
    println!("Unknown command: '{}'", name);
    println!("Type 'help' to see available commands.");
}
