#  Shell Implementation for HeineOS

## Overview

For lesson 7 an interactive shell was implemented for HeineOS. The shell replaces the Demo Menu as
the program that is started after boot and is the main user interface of the system.
Its commands expose the operating-system components that were built in the previous
lessons: the filesystem, the scheduler, the heap allocator, the PIT, the PCI bus, the
PC speaker and the framebuffer.

The shell runs as a normal kernel thread, started in `boot.rs`:

```rust
scheduler().ready(Thread::new(shell::run));
```

## Features

- Interactive prompt with line editing and Backspace
- Command parsing with arguments and aliases
- Bounded command history (32 entries) with Up/Down navigation
- Tab completion of command names
- Terminal scrollback (256 lines) with Ctrl+Up / Ctrl+Down
- Read-only access to filesystem, scheduler, heap, PCI and framebuffer information
- Integration with a Demo Menu
- Shutdown and reboot

## Commands

| Command | Description |
|---|---|
| `help [command]` | List all commands, or show the detailed help of one command |
| `clear` | Clear the terminal |
| `echo <text>` | Print text |
| `about` | Show information about HeineOS |
| `history` | Show the previously entered commands |
| `time` / `uptime` | Show the system uptime as `HH:MM:SS.mmm` |
| `meminfo` | Show kernel heap statistics |
| `sysinfo` | Show a summary of the whole system |
| `fbinfo` | Show framebuffer and terminal size |
| `sleep <ms>` | Wait for the given duration (1–10000 ms) |
| `ps` | List the threads known to the scheduler |
| `tid` | Show the ID of the current thread |
| `ls [path]` | List the contents of a directory |
| `cat <file>` | Print the contents of a file |
| `stat <path>` | Show the metadata of a file or directory |
| `hexdump <file>` | Show the first 512 bytes of a file in hexadecimal |
| `lspci [-v]` | List the devices on the PCI bus |
| `beep <hz> <ms>` | Play a tone on the PC speaker |
| `demo` | Open the Demo Menu |
| `shutdown` / `poweroff` | Shut the machine down |
| `reboot` | Reboot the machine |

Unknown commands and invalid arguments produce a message and never panic. An empty
input line simply shows a new prompt.

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Backspace` | Delete the previous character |
| `Up` / `Down` | Walk through the command history |
| `Tab` | Complete the command name |
| `Ctrl+C` | Cancel the current input line, or leave the Demo Menu |
| `Ctrl+L` | Clear the screen, keeping the line that is being typed |
| `Ctrl+U` | Discard the current input line |
| `Ctrl+Up` / `Ctrl+Down` | Scroll the terminal output by one line |
| `Ctrl+PageUp` / `Ctrl+PageDown` | Scroll the terminal output by one page |

Shortcuts are recognized from the physical key and the Ctrl modifier flags, not from
the reported ASCII code. The keyboard driver has no translation table for control
combinations, so a Ctrl+L event still carries the character `l`. Ctrl combinations are
therefore handled *before* printable characters and before the plain arrow keys.

## Architecture

The shell lives in `kernel/src/shell/`:

| File | Responsibility |
|---|---|
| `mod.rs` | Module interface, exports `run()` |
| `shell.rs` | Shell lifecycle: welcome message, prompt, read/parse/execute loop |
| `input.rs` | Line editing, history navigation, Tab completion, Ctrl shortcuts |
| `parser.rs` | Turns an input line into a `Command` value |
| `registry.rs` | Table describing every command |
| `history.rs` | Bounded list of previously entered command lines |
| `commands/` | Command implementations, grouped by subsystem |

One input line takes this path:

```
keyboard -> input::read_line -> parser::parse -> commands::execute
```

`registry.rs` is the single source of command metadata. The `help` overview,
`help <command>` and Tab completion all read the same table, so a command is
described exactly once.

Commands use the existing kernel abstractions instead of subsystem internals. Where a
subsystem could not provide the required information, a small read-only API was added:

| API | Used by |
|---|---|
| `allocator::global::heap_stats()` | `meminfo`, `sysinfo` |
| `Scheduler::thread_snapshot()` | `ps` |
| `TarFs::list()` and `TarFs::metadata()` | `ls`, `stat`, `cat`, `hexdump` |
| `PciDevice::bus()/device()/function()`, `pci::class_name()` | `lspci` |
| `Terminal::erase()`, `size()`, scrollback and viewport control | line editing, `fbinfo` |
| `machine::shutdown()` and `machine::reboot()` | `shutdown`, `reboot` |

`machine.rs` keeps the low-level power-off and reset code in one place: shutdown
requests an ACPI soft power off, reboot pulses the CPU reset line through the keyboard
controller. Neither function returns; if the hardware does not react, the CPU is halted.

Terminal scrollback belongs to the terminal driver, not to the shell. The driver keeps
the last 256 lines that scrolled off the screen in a fixed ring buffer and renders a
window into it. `view_offset` counts how many lines the window is moved away from the
newest output, so `0` always means "bottom". New output snaps the view back to the
bottom. The shell only detects the key combination and calls the terminal.

## Demo Menu Integration

The `demo` command calls the `demo::menu::run()` synchronously from the shell
thread. Inside the menu, Up/Down and Enter work as before, and Ctrl+C makes `run()`
return, which brings the user back to the prompt. An individual demo is left with
`Esc`, which returns to the Demo menu.

```
Shell --demo--> Demo Menu --Enter--> Demo --Esc--> Demo Menu --Ctrl+C--> Shell
```

Shell, menu and demos read from the same keyboard buffer. When the menu returns, the
buffer is drained twice with a short pause in between, because the release events of
the Ctrl+C combination arrive a few milliseconds after the press event and would
otherwise appear as stale input at the next prompt.

## Build and Run

Build the bootable image:

```bash
cargo make --no-workspace --profile production image
```

Build and run HeineOS in QEMU:

```bash
cargo make --no-workspace --profile production qemu
```

## Known Limitations

- The scrollback stores characters only, so older output is redrawn in the default
  colour and pixels drawn directly by a demo are not recorded.
- `clear` and Ctrl+L discard the scrollback.
- Tab completion covers the first word of a line; file names are not completed.
- `cat` prints at most 64 KiB and `hexdump` at most 512 bytes; both report truncation.
- The filesystem is read-only, so there are no commands that modify files.


## AI Usage

AI-assisted tools were used during the development of this project. **ChatGPT** was used for discussing implementation ideas, planning the shell architecture and refining the documentation. **Cline Code** was used as an AI-assisted programming tool during implementation, refactoring and code cleanup.

AI-generated suggestions and code were reviewed, adapted and integrated into the existing HeineOS codebase. The resulting implementation was built and tested as part of the development process.
