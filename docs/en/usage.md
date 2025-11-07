# Usage (GUI)

This document summarizes how to run the emulator as a GUI app and the most relevant options. See companion docs for detailed console I/O, tracing/disassembly concepts, GUI memory editing, and OS‑9 workflows.

## Platform Prerequisites

The emulator builds and runs as a native GUI app (eframe/winit). Install the following per platform.

- Windows 10/11
  - Rust (rustup): https://rustup.rs/
  - Visual Studio Build Tools (C++): https://visualstudio.microsoft.com/visual-cpp-build-tools/
  - Optional: a TTF/OTF/TTC font for Japanese UI labels; set `EM6809_GUI_FONT` to its path.
- macOS 12+
  - Rust (rustup): `curl https://sh.rustup.rs -sSf | sh`
  - Xcode Command Line Tools: `xcode-select --install`
  - Optional font for Japanese: set `EM6809_GUI_FONT`.
- Linux / WSL2 + WSLg
  - Rust (rustup)
  - Build/runtime packages (Debian/Ubuntu):
    - `sudo apt update && sudo apt install -y build-essential pkg-config libxkbcommon0 libxkbcommon-dev libwayland-client0 libwayland-cursor0 libwayland-dev libx11-xcb1 libxcb1 libxcb-render0 libxcb-shape0 libxcb-xfixes0 libegl1 libgles2`
  - WSL2: Use Windows 11 WSLg or another Wayland/X11 server. WSLg provides Wayland/EGL out of the box.

Notes
- On Windows we create the event loop on a background thread; on some Unix setups the GUI must own the main thread.

## Basics

- Build: `cargo build --release`
- Run and open GUI (paused by default):
  - `target/release/em6809 path/to/rom.s19 --format srec`
- Start from an explicit PC:
  - `target/release/em6809 path/to/rom.s19 --format srec --pc 0xE000`
- Show help: `target/release/em6809 --help`

PC initialization priority:

1. `--pc` if provided
2. Reset vector
3. S‑Record entry (if reset vector is invalid)
4. First loaded memory range start

## Key options (GUI-oriented)

- `--format bin|srec`: Input format
- `--base <ADDR>`: Base address for BIN
- `--pc <ADDR>`: Force start address

I/O (see console_io.md for details)

- `--console-stdin`: Attach stdin to the guest
- `--console-echo`: Enable local echo
- `--console-in <TEXT>`: Send escaped text (`\\r`, `\\n`, `\\t`, `\\\\`)
- `--console-out <FILE>`: Mirror output to file
- `--console-tee-stderr`: Tee output to stderr as well
- `--console-flush-every {off|byte|<N>}`: Flush interval (default 1 = every byte, max 128)
- `--console-flush-newline`: Also flush on newline

## Quick recipes

- Run Hello World (GUI):
  - `target/release/em6809 samples/hello/hello.s19 --format srec`
- Attach stdin with echo:
  - `... --console-stdin --console-echo`

For deeper I/O control see console_io.md; for debugging/tracing see tracing.md; for GUI memory editing see memory_editing.md; for OS‑9 integration see os9_guide.md.

## Environment variables

The following variables affect the GUI register viewer (when building with `--features gui`). See `docs/en/regview_integration.md` for details.

- `EM6809_GUI_FONT`: Path to a TTF/OTF/TTC font to prefer (helps render Japanese glyphs).
- `EM6809_LANG`: Force listing context‑menu labels to Japanese (`ja`) or English (`en`); auto‑detected when unset.

Linux client-side decorations (CSD):

- `EM6809_CSD`: When set to `1` or `true` on Linux, enables a custom client‑side decorated header (icon/title, Help→About, Minimize/Maximize/Close). Example: `EM6809_CSD=1 cargo run --release -- samples/echo/echo.s19 --format srec`.

Additional (About dialog icon on Linux):

- `EM6809_ABOUT_ICON`: Absolute path to a PNG image used for the About dialog icon on Linux.
  - If set and readable, the PNG is loaded and resized to 32×32.
  - If unset, the app looks for `assets/app_icon_32.png`, then `assets/icon32.png`, then `assets/icon.png`.
  - If no PNG is found, a vector fallback is drawn (rounded rectangle with a lowercase "e").

## GUI menus

- Help → About em6809…: shows version and license. On Windows and macOS, the platform’s window/app icon is used. On Linux, the icon is loaded via `EM6809_ABOUT_ICON`/assets as above, with a vector fallback when missing.

## GPIO and Timer options (CLI)

- `--gpio-bits N`: Number of GPIO LEDs (default 8)
- `--gpio-on-color 0xRRGGBB`: LED ON color (e.g., `0xFFD000`)
- `--timer 0xADDR`: Attach periodic timer at base (e.g., `0xFF10`)
- `--timer-reload 0xNNNN`: Timer period in instruction ticks
- `--timer-rate HZ`: Target rate in Hz; combined with `--timer-ips` to derive reload
- `--timer-ips N`: Estimated instructions/sec for deriving reload from `--timer-rate`
- `--timer-irq`: Enable timer IRQ; `--timer-firq`: route to FIRQ instead
- `--timer-start`: Start timer immediately
- `--run`: Start unpaused
