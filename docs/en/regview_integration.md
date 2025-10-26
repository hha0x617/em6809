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

- `cargo run --release --features gui -- --dump-regs --srec path/to/rom.s19 --run`

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

## CLI suggestion

Wire `--dump-regs` to spawn the viewer. You can keep `--trace` independent; both can be enabled.
