# Memory Dump Edit UI — Design Memo

This document captures the original UI / implementation plan for
the "edit memory values from the dump pane" feature.  It is a
design memo, not a user reference.

## Status (2026-05)

- **Phase 1 has shipped.**  The Edit toggle, inline Hex-cell editing,
  and Apply / Cancel flow described under "Implementation phases"
  below are all implemented.  The user-facing reference for what
  shipped lives at `docs/en/memory_editing.md` (and its Japanese
  companion `docs/ja/memory_editing.md`) — readers who want to *use*
  the feature should go there, not here.
- **Phase 2 is still TODO.**  ASCII-side editing, paste (Hex / text),
  the multi-step undo stack, and range bulk operations have not
  shipped.  This document is kept around mainly for them.
- The text below preserves the original design rationale; status
  callouts are added inline where the as-shipped behaviour diverges
  from the proposal.

## Scope and goals

- **Goal**: let the user edit memory one byte at a time from the GUI
  memory dump pane, with the change reflected in the emulator's
  memory.
- **Scope**: writes into the 64 KiB logical memory space (transparently
  via `Memory` or `Mc6829`).  MMU protection is honoured; we do not
  force writes through.

## User experience (UX)

- **Edit-mode toggle.**  Add an "Edit" toggle to the memory pane
  header.  When ON, the editing UI activates.
- **Edit unit.**  One byte at a time.
  - **Hex cell**: a small TextEdit accepting a 2-digit hex value
    (Enter commits, Esc cancels).
  - **ASCII cell**: a single-character input (non-printables shown
    as `.`).  *(Phase 2.)*
- **Bulk operations** (introduced gradually):
  - **Paste** (Hex / text): modal input → staged starting at a
    given address.  *(Phase 2.)*
  - **Fill**: apply a single value across a selected range.
    *(Phase 2; optional.)*
- **Apply flow**:
  - Stage (pending) → Apply → bus `write8`.  Cancel discards all.
  - "Live apply" toggle (optional) commits each cell on confirm.
    Default OFF.  *(Phase 2; not shipped.)*
- **Feedback**:
  - Pending edits highlighted in soft yellow; cells that failed
    (e.g., write-protected) shown in red, with a tooltip explaining
    why.
- **Navigation**: arrows / Tab move between cells.  Home / End jump
  to the line start / end.  Shift extends the selection (when range
  selection lands).
- **Follow PC**: when "Follow PC" is on, the cell currently being
  edited must not get scrolled out of view; the auto-scroll is
  suppressed while editing.

## State and data flow (regview)

- New fields on `RegApp`:
  - `mem_edit_mode: bool` — Edit ON/OFF.
  - `mem_live_apply: bool` — commit-on-confirm.  *(Phase 2.)*
  - `mem_pending: HashMap<u16, u8>` — unapplied diff.
  - `mem_selection: Option<u16>` — currently selected address.
  - `mem_error: Option<String>` — most recent error string.
  - `mem_undo: Vec<Vec<(u16, u8)>>` — per-batch undo stack.
    *(Phase 2.)*
- Rendering:
  - Most cells render as Labels.  Only the selected cell swaps in a
    TextEdit (Hex 2-char) / single-char ASCII edit.
  - Pending bytes get a coloured background on both Hex and ASCII
    sides.

## Command extensions (regview → main)

Add to `enum Command`:

- `MemWrite(Vec<(u16, u8)>)` — batch write triggered by Apply.
- `MemWriteLive(u16, u8)` — single-cell live apply.  *(Phase 2.)*
- `MemPaste(Vec<u8>, u16)` — paste a payload starting at the
  given address.  *(Phase 2.)*

No return value is required; the UI infers success from the next
read.  Returning an error count later is an option if we want it.

## Worker side (apply only while paused)

- Handle the new commands in the paused-state command path.
- Disable Apply / Live while running (UI greys the buttons out).
- The bus may be `IoBus<Memory|Mc6829>` or
  `WriteTrack(IoBus<...>)`; reuse the existing downcast pattern to
  reach the underlying `write8`.
- After applying, the existing memory-view update logic refreshes
  the window.

## Protection / failure handling (MMU)

- Honour `Mc6829`'s WPROT / RPROT / NX bits.
  - WPROT hit: real memory is unchanged → the diff disappears on
    the next refresh.
  - The UI marks unapplied (failed) bytes in red after Apply, with a
    tooltip stating "page is write-protected".
- Addresses outside the 64 KiB space are out of scope.  Edits to
  addresses outside the visible window are still kept by address
  and applied normally.

## Performance considerations

- Only the selected cell uses a TextEdit; everything else stays as a
  Label so the per-frame cost stays low.
- Paste is capped (suggested: 4 KiB).
- Apply runs as a sequence of `write8` calls; that's fine because
  `WriteTrack` already de-duplicates.

## Key bindings (Hex editing)

- Allowed input: `0–9`, `A–F`.  Backspace cancels the cell, Enter
  commits, Esc cancels.  Arrow keys move to adjacent cells (when
  range selection lands).
- ASCII editing: a single printable character.  Backspace cancels.
  *(Phase 2.)*

## Implementation phases

- **Phase 1 (minimal — shipped)**:
  - Edit toggle, inline Hex-cell editing, Apply / Cancel
    (paused only).
  - Live apply (optional) and single-step undo land later.
- **Phase 2 (extension — TODO)**:
  - ASCII-side editing, paste (Hex / text), undo stack, range bulk
    operations.

## Touched code (overview)

- `src/regview.rs`:
  - Memory-edit state on `RegApp`.
  - Memory pane: cell selection, inline TextEdit, pending-diff
    handling, Apply / Cancel / live-apply buttons.
  - `Command` extensions for memory ops.
- `src/main.rs`:
  - Paused-state command handler routes `MemWrite` & co. to
    `Bus::write8`.
  - The existing memory-window update path refreshes after Apply.

## Guarding policy

- Apply only while paused (UI greys the buttons out otherwise).
- Honour MMU protection (no forced writes).
- Reuse the same Edit / Apply / Cancel vocabulary and colour cues as
  the register-edit UI for consistency.

***

The design slots naturally into the existing Viewer / Command / Bus
boundaries and is incrementally deployable.  Phase 1 has shipped;
Phase 2 will extend ASCII editing, paste, undo, and range
operations once we revisit the feature.
