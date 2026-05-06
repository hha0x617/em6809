# Troubleshooting

Common pitfalls and how to fix them.

## Output is choppy/slow

- Increase flush interval/buffering: `--console-flush-every 128`
- Mirror large logs to a file: `--console-out <FILE>`

## Input not reaching the guest

- Add `--console-stdin`
- If you want local echo, add `--console-echo`
- For OS‑9, confirm device init order (use tracing)

## Start address is not as expected

- `--pc` has highest priority; otherwise: Reset vector > SREC entry (if invalid) > first loaded range start

## Verify branch/PC flow

- Use tracing to see instructions/registers/targets

## Encoding/input issues

- Use a UTF‑8 terminal and a compatible font
- On Windows, note Ctrl+C exits the emulator (raw stdin)

## macOS GUI

- Run from Terminal.app or iTerm2. The GUI runs on the main thread.
- If Japanese labels/text render as tofu (missing glyphs), set `EM6809_GUI_FONT` to a Japanese-capable font (e.g. `/System/Library/Fonts/Hiragino Sans W6.ttc`). When unset, the app auto-discovers common system fonts.
- HiDPI (Retina) scaling is handled by eframe’s defaults. Adjust macOS display scaling if needed.
- If the window fails to open or closes immediately, check stdout/stderr logs and try a clean rebuild: `cargo clean && cargo run --release`.

## WSL2 / Linux GUI

WSL2 + WSLg (Wayland) is the typical environment for running em6809's GUI on Linux from a Windows host.  Most setups work out of the box on Windows 11.  This section walks through prerequisites, environment variables, fallback paths, and the most common failure modes.

### Prerequisite packages (Ubuntu / Debian)

- **Wayland + GL minimum** — `libxkbcommon0`, `libwayland-client0`, `libwayland-cursor0`, `libwayland-egl1`, `libegl1-mesa`, `libgles2`
- **X11 (XWayland) fallback** — `libx11-6`, `libxi6`, `libxcursor1`, `libxrandr2`, `libxinerama1`
- **Diagnostic tools (optional)** — `mesa-utils` (provides `glxinfo -B` for GL verification)
- **Bulk install**:
  ```bash
  sudo apt update
  sudo apt install -y \
    libxkbcommon0 libwayland-client0 libwayland-cursor0 libwayland-egl1 \
    libegl1-mesa libgles2 \
    libx11-6 libxi6 libxcursor1 libxrandr2 libxinerama1 \
    mesa-utils
  ```

### Build with the GUI feature

```bash
cargo build --release --features gui
```

### Runtime environment variables (WSLg / Wayland, recommended)

- `export XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir` — let the process see WSLg's Wayland socket.
- `export WINIT_UNIX_BACKEND=wayland` — pin winit to its Wayland backend.
- `export WGPU_BACKEND=gl` — pin wgpu to OpenGL.
- Example invocation:
  ```bash
  XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir \
  WINIT_UNIX_BACKEND=wayland \
  WGPU_BACKEND=gl \
  cargo run --release --features gui -- <image> --paused --dump-regs
  ```

### X11 (XWayland) fallback

If Wayland is unhappy, fall back to X11:

- `export WINIT_UNIX_BACKEND=x11`
- `export DISPLAY=:0` (usually already set under WSLg)
- Example:
  ```bash
  WINIT_UNIX_BACKEND=x11 WGPU_BACKEND=gl \
  cargo run --release --features gui -- <image> --paused --dump-regs
  ```

### Legacy WSL without WSLg (reference)

On older WSL setups that lack WSLg, run an X server on the Windows side (VcXsrv, X410, etc.) and point WSL at it:

```bash
export DISPLAY=$(awk '/nameserver/{print $2; exit}' /etc/resolv.conf):0
export WINIT_UNIX_BACKEND=x11
# Optional, depending on the X server:
export LIBGL_ALWAYS_INDIRECT=1
```

### Verification

- `echo $WAYLAND_DISPLAY` — should be non-empty (e.g. `wayland-0`).
- `echo $DISPLAY` — set, for the XWayland fallback path.
- `glxinfo -B` — shows the GL implementation in use (requires `mesa-utils`).

### General notes

- If `XDG_RUNTIME_DIR` is unset under Wayland, GUI startup can fail.  Under WSLg this is normally `/mnt/wslg/runtime-dir`.
- Combinations of `WINIT_UNIX_BACKEND=wayland|x11` × `WGPU_BACKEND=gl|vulkan` sometimes resolve issues that any single setting alone does not.
- For multi-viewport debugging, run with `RUST_LOG=egui_winit=debug` to surface egui/winit details (e.g. `ViewportCommand::InnerSize ignored by winit`) — useful when investigating Linux-side rendering anomalies in the native Settings panel.

### ZINK init failure (GUI fails to launch or exits shortly after start)

The WSLg ZINK / Mesa OpenGL pipeline can fail to initialize after Windows GPU driver updates, sleep/resume, or Mesa package upgrades.  This is a WSLg-side instability, not an em6809-specific bug.

- **Symptoms** (printed to stderr):
  ```
  libEGL warning: failed to get driver name for fd -1
  libEGL warning: MESA-LOADER: failed to retrieve device information
  MESA: error: ZINK: failed to choose pdev
  libEGL warning: egl: failed to create dri2 screen
  Io error: Broken pipe (os error 32)
  [regview] GUI error: winit EventLoopError: Exit Failure: 1 — running headless
  ```
  The app exits or falls back to headless mode without showing a window.

- **Background**: WSL2 typically has no `/dev/dri/`.  WSLg routes OpenGL calls through ZINK (a Vulkan-backed OpenGL implementation) → D3D12 → Windows GPU driver.  If any link in that chain isn't ready, Mesa can't enumerate a Vulkan physical device and GL context creation fails.

- **Workaround: bypass ZINK and force Mesa's llvmpipe (CPU) rasterizer**
  ```bash
  export LIBGL_ALWAYS_SOFTWARE=1
  export GALLIUM_DRIVER=llvmpipe
  cargo run --release
  ```

- **Persistence (recommended)**: append to `~/.bashrc` so new shells pick it up automatically:
  ```bash
  cat >> ~/.bashrc <<'EOF'
  export LIBGL_ALWAYS_SOFTWARE=1
  export GALLIUM_DRIVER=llvmpipe
  EOF
  source ~/.bashrc
  ```

- **Trade-off**: GPU acceleration is disabled.  For em6809 (egui-based, light on rendering) the perceived performance difference is negligible.

- **Re-enable GPU rendering temporarily**: `unset LIBGL_ALWAYS_SOFTWARE GALLIUM_DRIVER` and rerun.

- **If the workaround alone doesn't help**:
  - From PowerShell: `wsl --shutdown`, wait a few seconds, then reopen the WSL terminal.
  - Reinstall Mesa: `sudo apt install --reinstall -y libgl1 libegl1 libglx-mesa0 libgl1-mesa-dri`
  - Verify with `glxinfo -B` (from `mesa-utils`); you should see `direct rendering: Yes` and the `llvmpipe` device.
