# Memory Dump Pane Editing

This guide explains how to edit memory in the GUI Memory pane.

## Overview

- In Edit mode you can modify the displayed memory one byte at a time (Hex cells).
- Edits are staged (pending) until you click Apply, which writes them to the emulator.
- After Apply, the listing and memory views refresh automatically.

## Workflow

- Enter Edit mode:
  - Click "Edit" in the pane header to enter edit mode.
  - In edit mode, "Apply" and "Cancel" buttons are shown.
- Edit a cell:
  - Click a Hex cell to enable inline editing (TextEdit).
  - Input is interpreted as hexadecimal and staged in the pending buffer.
  - Only bytes that actually change are highlighted (soft yellow) in Hex and ASCII; entering the same value does not highlight.
- Apply / Cancel:
  - Apply: writes all pending edits. Edit mode exits immediately after Apply.
  - Cancel: discards all pending edits and exits edit mode.

## Keyboard (Edit mode)

- Tab / Shift+Tab: move to next / previous cell.
- Enter: move to next cell.
- Left / Right:
  - Move the caret inside the current cell as usual.
  - Only when the caret is already at the left/right edge and you press the same direction again does focus move to the neighboring address (the first press that hits the edge does not move).
- Up / Down: move 16 bytes up / down (same column).
- Home / End: go to line start / end.

Notes:
- Immediately after moving to a new cell the value is selected to make overwriting easy.
- Text input follows egui's normal insert/delete behavior. Pasting is supported.

## Display and Follow PC

- Follow PC:
  - When enabled, the PC is highlighted and the listing auto-scrolls.
  - When switching Lst/Hex, the PC line is scrolled to center on the frame where it can be highlighted.
- Listing refresh:
  - When memory is changed, the disassembly for the affected span is recomputed and the Hex listing is updated.

## Apply timing and protection

- Apply is performed while paused (the UI ensures this).
- Writes to protected regions (e.g., by MMU) may be ignored; in that case the difference disappears on the next refresh.

## Known behavior

- ASCII is for readability; it reflects and highlights changes made in Hex.
- Pending edits are tracked per Hex cell and committed/cleared by Apply/Cancel.

## Troubleshooting

- "Arrow keys do nothing": ensure the cell is focused (click it) before using the keyboard.
- "Left/Right do not move to neighbor": you must press Left/Right again after hitting the cell edge; the first press that reaches the edge keeps the caret inside the cell.
- "Still in edit mode after Apply": by design, the current implementation exits edit mode after Apply.

This feature evolves based on user feedback. Please file issues for ideas and problems.
