# Console I/O Guide

Options to control the emulator’s console input/output with practical recipes. On Windows/Linux/macOS, stdin is set to raw (immediate) mode; output is buffered internally for UTF‑8.

## Main options

- `--console-stdin`: Attach host stdin (keyboard) to the guest
- `--console-echo`: Enable local echo (default OFF)
- `--console-in <TEXT>`: Send text immediately (escapes: `\\r`, `\\n`, `\\t`, `\\\\`)
- `--console-out <FILE>`: Mirror console output to a file
- `--console-tee-stderr`: Also tee output to stderr
- `--console-flush-every {off|byte|<N>}`: Flush strategy (default: per newline)

## Recipes

- Send one command line:
  - `... --console-in "HELP\\r\\n"`
- Interactive input with local echo:
  - `... --console-stdin --console-echo`
- Save output while displaying:
  - `... --console-out session.log --console-tee-stderr`
- If output feels choppy/slow, increase buffering:
  - `... --console-flush-every 128`
- For tight timing checks, flush every byte:
  - `... --console-flush-every byte`

## Troubleshooting

- No input? Ensure `--console-stdin` and check `--console-echo` if needed
- Garbled characters? Use a UTF‑8 terminal and verify font
- Interrupt: Ctrl+C exits the emulator
