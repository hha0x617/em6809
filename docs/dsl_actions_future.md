# DSL Action Expansion (Design Note)

This note outlines proposed additions to the boot script DSL to simplify interrupt behavior tuning and broader device control.

## Goals
- Abstract common IRQ/FIRQ handling patterns.
- Offer clear, high‑level controls while keeping existing low‑level actions.

## Proposed Actions
- `irq_policy <irq|firq> <device>`: route a device’s interrupts to IRQ or FIRQ.
- `irq_priority <device> <low|normal|high>`: hint priority within the emulator’s event loop.
- `irq_mask <on|off>`: keep current action; allow `irq_mask firq on|off` (granular).
- `con_flow <none|xonxoff|rtscts>`: console flow control abstraction.
- `dev_map <name> 0xADDR`: named mapping for common devices (e.g., `dev_map console 0xFF00`).

## Semantics
- New actions compile down to existing primitives (`con_ctrl`, watermark, mask ops) to avoid breaking changes.
- No change to file format; actions are optional and backward‑compatible.

## Validation
- Unit tests: routing correctness, mask transitions, and regression tests for existing scripts.
- Templates: update emitters to include new actions with comments.

## Migration
- Existing scripts remain valid. New actions can progressively replace verbose sequences where helpful.
