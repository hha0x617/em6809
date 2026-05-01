# Console I/O Guide

Options to control the emulator’s console input/output with practical recipes. On Windows/Linux/macOS, stdin is set to raw (immediate) mode; output is buffered internally for UTF‑8.

## Register layout

The console device is a Motorola MC6850 ACIA-compatible UART (base = `--console 0xADDR`):

| Offset | Read | Write |
| --- | --- | --- |
| `+0` | Status Register (SR) — bit 0 = RDRF, bit 1 = TDRE, bit 7 = IRQ | Control Register (CR) |
| `+1` | Receive Data Register | Transmit Data Register |

Compatible out of the box with Hha Forth / Hha Lisp / NetBSD MVME147 boot ROM and similar real-hardware-targeting code. The old custom layout (`Simple`: data@+0, status@+1) has been removed.

## Main options

- `--console-stdin`: Attach host stdin (keyboard) to the guest
- `--console-echo`: Enable local echo (default OFF)
- `--console-in <TEXT>`: Send text immediately (escapes: `\\r`, `\\n`, `\\t`, `\\\\`)
- `--console-out <FILE>`: Mirror console output to a file
- `--console-tee-stderr`: Also tee output to stderr
- `--console-flush-every {off|byte|<N>}`: Flush strategy (default: 1 = every byte, max 128)
- `--console-flush-newline`: Also flush on newline (line-oriented output)
- `--console-rx-watermark <N>`: Assert IRQ when the RX buffer holds at least N bytes. `0` means IRQ on the first byte received.
- `--console-irq-hold <N>`: Keep the IRQ line asserted for N cycles after raising it. Use a nonzero value to avoid missing short IRQ pulses; `0` drops IRQ immediately after the event.

## Recipes

- VT100 console demo (GUI window):
  - `... samples/vt100/vt100_demo.s19 --console 0xFF00`
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
