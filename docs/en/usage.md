# Usage (CLI Overview)

This document summarizes mc6809emu’s key CLI options with practical examples. See companion docs for detailed console I/O, tracing/disassembly, and OS‑9 workflows.

## Basics

- Build: `cargo build --release`
- Run an S‑Record until Ctrl+C:
  - `target/release/mc6809emu --run --srec path/to/rom.s19`
- Start from an explicit PC:
  - `target/release/mc6809emu --run --srec path/to/rom.s19 --pc 0xE000`
- Show help: `target/release/mc6809emu --help`

PC initialization priority:

1. `--pc` if provided
2. Reset vector
3. S‑Record entry (if reset vector is invalid)
4. First loaded memory range start

## Key options

- `--run`: Continuous run (Ctrl+C to stop)
- `--srec <FILE>`: Load S‑Record (repeatable)
- `--pc <ADDR>`: Force start address
- `--trace`: Log instructions, registers, and resolved branch targets
- `--step`: Step mode
- `--disasm <RANGE>`: Disassemble (e.g., `0xE000:0xE100`)
- `--hex <RANGE>`: Hex dump (e.g., `0xE000:0xE040`)
- `--dump-regs`: When built with `--features gui`, opens a live register window with Pause/Resume

I/O (see console_io.md for details)

- `--console-stdin`: Attach stdin to the guest
- `--console-echo {ON|OFF}`: Local echo control
- `--console-in <TEXT>`: Send escaped text (`\\r`, `\\n`, `\\t`, `\\\\`)
- `--console-out <FILE>`: Mirror output to file
- `--console-tee-stderr`: Tee output to stderr as well
- `--console-flush-every {off|byte|<N>}`: Flush interval

## Quick recipes

- Run Hello World:
  - `target/release/mc6809emu --run --srec samples/hello_world.s19`
- Disassemble only:
  - `target/release/mc6809emu --disasm 0xE000:0xE100`
- Run with trace:
  - `target/release/mc6809emu --run --srec path/to/rom.s19 --trace`
- Attach stdin with echo:
  - `... --console-stdin --console-echo ON`

For deeper I/O control see console_io.md; for debugging/tracing see tracing.md; for OS‑9 integration see os9_guide.md.

## Environment variables

The following variables affect the GUI register viewer (when building with `--features gui`). See `docs/en/regview_integration.md` for details.

- `EM6809_GUI_FONT`: Path to a TTF/OTF/TTC font to prefer (helps render Japanese glyphs).
- `EM6809_LANG`: Force listing context‑menu labels to Japanese (`ja`) or English (`en`); auto‑detected when unset.
