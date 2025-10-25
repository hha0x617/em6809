# OS-9 Device Map Templates (MC6809)

This document provides suggested I/O mappings and interrupt vector notes for OS‑9–style setups on MC6809. Adjust addresses to your target image and board configuration.

## I/O Page Suggestions
- Console (UART/ACIA): base `0xFF00`
  - Control/Status: `0xFF00`
  - Data RX/TX:     `0xFF01`
  - Optional aux:   `0xFF02..0xFF03`
- Timer: `0xFF10`
- Disk/Block I/O: `0xFF20`
- Free/Reserved: `0xFF30..0xFFEF`

Notes:
- Keep the I/O page contiguous at `0xFF00..0xFFFF` for simplicity.
- Use the boot script to set console IRQ/FIRQ behavior and watermarks.

## Interrupt Vectors (Reference)
MC6809 vectors reside in the top of memory. Typical usage (adjust per ROM):
- FIRQ: fast IRQ handler (e.g., high‑rate console RX)
- IRQ: standard interrupt handler (e.g., timer)
- NMI: non‑maskable for critical faults
- RESET: system entry

Recommendations:
- Write‑protect vector page after initialization (e.g., via `attr`/`prot` in the boot script).
- Route console RX to FIRQ when low latency is preferred; otherwise use IRQ.

## Boot Script Hints
- Use `on_step`/`on_pc` to apply MMU maps and enable device IRQs in phases.
- Apply `con_ctrl`, `con_rx_wm`, and optional `con_irq_hold` to tune console behavior.
- Switch from `mode system` to `mode user` only after kernel pages are mapped.
