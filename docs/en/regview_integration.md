# Register Viewer Integration (GUI)

This provides a per‑instruction live register window controlled by a Pause/Resume button.

## Cargo setup

Add to Cargo.toml:

```
[features]
# Build GUI register viewer when enabled: `--features gui`
gui = ["eframe"]

[dependencies]
eframe = { version = "0.27", optional = true }
```

Build with GUI:

- `cargo run --release --features gui -- path/to/rom.s19 --format srec --dump-regs`

## Hooking the viewer

1) Include the viewer (e.g., in `src/main.rs`):

```rust
mod regview; // at top
```

2) When `--dump-regs` is present, spawn the window before starting the CPU loop:

```rust
#[cfg(feature = "gui")]
let viewer = regview::Viewer::spawn("MC6809 Registers");
#[cfg(not(feature = "gui"))]
let viewer = regview::Viewer::spawn(""); // no‑op in non‑GUI builds
```

3) After each executed instruction, push a snapshot and honor pause/step:

```rust
// Construct a snapshot from your CPU state
let snap = regview::RegSnapshot {
    a: cpu.a,
    b: cpu.b,
    d: cpu.d(),           // or ((cpu.a as u16) << 8) | (cpu.b as u16)
    x: cpu.x,
    y: cpu.y,
    u: cpu.u,
    s: cpu.s,
    dp: cpu.dp,
    pc: cpu.pc,
    cc: cpu.cc,           // bit layout: E F H I N Z V C
};
viewer.push(snap);

// If paused without a pending step, wait; if a step is pending, it proceeds once
while viewer.should_pause() {
    std::thread::sleep(std::time::Duration::from_millis(10));
}
```

Insert the snippet right after your “execute one instruction” call (e.g., `cpu.step()`), so the window updates on every step.

## Behavior

- The viewer runs in a separate native window via `eframe`/`egui`.
- The Pause/Resume button toggles execution at instruction boundaries.
- In non‑GUI builds (without `--features gui`), the API is a no‑op and the emulator runs normally.

## Listing sources and Hex view

- When a companion `.lst` file exists next to the loaded image, the right side shows that listing.
- If no `.lst` is found, the viewer auto‑generates a hex‑style disassembly of the program span and displays it as a fallback.
- A “Hex view” toggle lets you switch between the `.lst` and the generated hex disassembly at any time (even when `.lst` exists).
- The hex bytes column is padded so mnemonics align consistently.

## Follow PC and highlighting

- “Follow PC” centers the view on the current PC and highlights the matching line.
- The highlight updates in real time while stepping or running.
- The listing also shows a subtle hover highlight for the line under the mouse cursor to improve readability and targeting.

## Self‑modifying code support (event‑driven re‑disassembly)

- The emulator wraps the memory bus with a lightweight write tracker.
- When code writes are detected:
  - Full span mode: re‑disassemble the entire PC‑containing span once and refresh the listing.
  - PC window mode: re‑disassemble only a small window around changed addresses and patch the lines in place.
- This avoids periodic full refresh overhead and scales better for large programs.

## Span mode and window size

- Span Mode controls the cost/latency tradeoff for re‑disassembly:
  - Full span: correct and simple; re‑disassembles the whole PC span when code changes.
  - PC window: faster; re‑disassembles a rolling window (configurable bytes) around modified addresses.
- Window size is adjustable (e.g., 256 bytes by default). Larger windows are more robust to instruction boundary drift; smaller windows are faster.

## Context menu (right‑click on listing lines)

- Right‑click on a line whose first token is a 4‑digit hex address to open the context menu:
  - Run to here: resumes execution and pauses automatically once PC reaches the selected address.
  - Set PC to this address: immediately sets PC to the line address.
  - Cancel: dismisses the menu without action.
- Labels appear in Japanese or English based on locale (see “Environment variables”).
- After choosing Run to here or Set PC, both the register pane and the listing pane refresh immediately so the new PC and highlight are reflected without requiring an extra step.

## Environment variables

Some aspects of the GUI viewer can be customized via environment variables:

- `EM6809_GUI_FONT`: Absolute path to a font file (`.ttf`, `.otf`, or `.ttc`).
  - When set and readable, the font is loaded and placed first in both the proportional and monospace families so CJK (Japanese) glyphs render properly.
  - If not set, the viewer attempts to auto‑discover a reasonable system font (Meiryo/Yu Gothic on Windows, Hiragino on macOS, Noto CJK on Linux) when present.
- `EM6809_LANG`: Force locale for context‑menu labels in the listing pane.
  - `ja` → Japanese labels (e.g., "ここまで実行", "PCの値を現在のアドレスに変更", "キャンセル").
  - `en` → English labels (e.g., "Run to here", "Set PC to this address", "Cancel").
  - If unset, language is auto‑detected from `LC_ALL`/`LC_MESSAGES`/`LANG` or the OS language (Windows), defaulting to English.

