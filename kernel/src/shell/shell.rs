/*
 * The lifecycle of the HeineOS shell.
 *
 * This file only drives the main loop:
 *   welcome message -> prompt -> read line -> parse -> execute -> next prompt
 *
 * Line editing lives in `input.rs`, parsing in `parser.rs` and the command
 * implementations in `commands/`.
 *
 * License: GPLv3
 */

use crate::device::terminal::terminal;
use crate::shell::commands;
use crate::shell::history::History;
use crate::shell::input::{self, PROMPT, ReadLineResult};
use crate::shell::parser;

/// Run the interactive shell.
///
/// This function never returns. It is started as the main interactive kernel
/// thread in `boot.rs` and keeps reading and executing commands.
pub fn run() {
    print_welcome();

    // The history belongs to the shell loop, so no global state is required.
    let mut history = History::new();

    loop {
        print!("{}", PROMPT);

        match input::read_line(&history) {
            ReadLineResult::Line(line) => {
                // The line is stored before it runs, so that the `history`
                // command shows itself as its own last entry.
                history.push(&line);
                execute_line(&line, &history);
            }
            // Ctrl+C cancels the current line. The shell itself keeps running
            // and simply shows a fresh prompt.
            ReadLineResult::Interrupted => {}
        }
    }
}

/// Print the welcome banner on a freshly cleared screen.
fn print_welcome() {
    terminal().lock().clear();

    println!("HeineOS Shell");
    println!("Type 'help' to see available commands.");
    println!("");
}

/// Parse a single input line and execute the resulting command.
fn execute_line(line: &str, history: &History) {
    let command = parser::parse(line);
    commands::execute(command, history);
}
