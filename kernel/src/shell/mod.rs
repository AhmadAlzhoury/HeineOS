/*
 * An interactive shell, serving as the main user interface of HeineOS.
 *
 * The module is split into these parts:
 *   - `shell`:    the shell lifecycle (welcome message, prompt, read/parse/execute loop)
 *   - `input`:    line editing, history navigation and Tab completion
 *   - `parser`:   turning an input line into a `Command`
 *   - `registry`: the table describing all commands (used by help and completion)
 *   - `history`:  the bounded list of previously entered commands
 *   - `commands`: the implementations of the individual commands
 *
 * License: GPLv3
 */

mod commands;
mod history;
mod input;
mod parser;
mod registry;
mod shell;

pub use self::shell::run;
