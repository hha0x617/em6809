# em6809

[![Build and Test](https://github.com/hha0x617/em6809/actions/workflows/build.yml/badge.svg)](https://github.com/hha0x617/em6809/actions/workflows/build.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE-APACHE)

*This application was generated with the assistance of AI tools: Codex CLI as part of the vibe coding workflow.*

## Overview

em6809 is a software emulator for the MC6809. It provides a GUI with register/memory views, rich console I/O controls, instruction tracing with resolved branch targets, and a disassembler. It can be used to boot and validate OS‑9/NitrOS‑9 setups.

## Install

- Build with Rust (stable):
  - `cargo build --release`

## Quick Start (GUI-only)

- Run and open GUI (paused by default):
  - `target/release/em6809 samples/hello/hello.s19 --format srec --console 0xFF00`
- Show help:
  - `target/release/em6809 --help`
- Start from an explicit PC:
  - `target/release/em6809 samples/hello/hello.s19 --format srec --console 0xFF00 --pc 0xE000`

## Common Use Cases

- Attach stdin with local echo: `... samples/echo/echo.s19 --console 0xFF00 --console-stdin --console-echo`
  (Disasm/Memory/Trace are available in the GUI window)
- VT100 console demo: `... samples/vt100/vt100_demo.s19 --console 0xFF00`

## Documentation

- Usage: `docs/en/usage.md`
- Console I/O: `docs/en/console_io.md`
- Tracing/Disassembly: `docs/en/tracing.md`
- OS‑9/NitrOS‑9 guide: `docs/en/os9_guide.md`
- Troubleshooting: `docs/en/troubleshooting.md`
- Instruction coverage: `docs/en/instruction_status.md`
- Register viewer integration (GUI): `docs/en/regview_integration.md`
- Environment variables (GUI): see `docs/en/regview_integration.md#environment-variables`

### Recent GUI viewer enhancements

- Listing fallback when `.lst` is missing (auto‑generated hex disassembly).
- Hex view toggle and Follow PC highlight with auto‑scroll.
- Event‑driven re‑disassembly for self‑modifying code (Full span / PC window modes).
- Right‑click context menu on listing lines: Run to here, Set PC to this address, Cancel.
- Hover highlight on listing lines; panes refresh immediately after context actions.

## Links

- Repository: https://github.com/hhagiwara/mc6809emu
- Issues: https://github.com/hhagiwara/mc6809emu/issues
- Releases: https://github.com/hhagiwara/mc6809emu/releases
- Docs index: `docs/README.md`
- English docs: `docs/en/`
- Japanese docs: `docs/ja/`
- Samples (traces, boot scripts): `samples/`

## References

- Motorola 6809 (overview): https://en.wikipedia.org/wiki/Motorola_6809
- MC6809 documentation archive (Bitsavers): http://bitsavers.org/components/motorola/6809/
- S‑Record format: https://en.wikipedia.org/wiki/SREC_(file_format)
- NitrOS‑9 project: https://sourceforge.net/projects/nitros9/
- 6809 community resources: http://www.6809.org.uk/
- LWTOOLS: https://www.lwtools.ca/

## Known Issues

### Linux on Wayland (Ubuntu Desktop, Mutter compositor)

The GUI uses [eframe / egui](https://github.com/emilk/egui) multi-viewport
to render Breakpoints / Call Stack / Console as separate top-level
windows.  On Wayland (notably Mutter on Ubuntu Desktop 24.04 LTS,
verified inside a Parallels VM on macOS host), the application can
**freeze when the Console window is fully occluded behind the main
window** — for example, when the user clicks a menu in the main window
or drags the main window over the Console.

The window manager raises a *"em6809 is not responding"* dialog after a
few seconds, and `Ctrl+C` may not terminate the process; closing the
main window's `[X]` button is the reliable exit path.

This is an upstream limitation of `eframe` + `winit` on Wayland.  We
verified the freeze across eframe 0.29 / 0.30 / 0.31 / 0.32 — the bug
is not fixed in any released version we tested.  This repo includes
partial mitigations (a 50 ms heartbeat repaint on the parent context,
a 33 ms Console repaint ticker, and `show_viewport_immediate` for the
Console viewport on Linux) that prevent the startup deadlock when
Console opens occluded, but they do not eliminate the freeze when
Console becomes occluded later via user interaction.

Workarounds:

* Use Linux on **X11** (start the GNOME session as "Ubuntu on Xorg").
* Or keep the Console window at least partially visible — do not park
  it directly behind the main window.
* Or run the build on Windows / macOS, where the multi-viewport stack
  is unaffected.

### Project direction

For the time being, multi-platform development on this repository is
**paused**.  Active development is moving to a new Windows-focused
project, **emfe** (a plugin-based emulator framework), which lets the
core emulator backend (this `em6809`) be used as a plugin while the
front-end UI is rebuilt on a Windows-native stack (WinUI 3 / WPF) that
sidesteps the Wayland multi-viewport class of issues entirely.

This repository continues to build and run on Windows / macOS / Linux
(native X11), and remains the canonical Rust implementation of the
MC6809 core that emfe loads.  Pull requests for the CPU core, debugger
back-end, and platform-agnostic code are welcome; new GUI features
specific to the eframe front-end are unlikely to be developed further
here.

## Contributing and Policies

- Development/Operations: `AGENTS.md`
- Code of Conduct/Security: `CODE_OF_CONDUCT.md` / `SECURITY.md` (if present)

## License

MIT OR Apache‑2.0 (dual license). See `LICENSE-MIT` and `LICENSE-APACHE`.
