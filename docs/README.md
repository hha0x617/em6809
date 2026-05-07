# Documentation Index

This is the entry point for em6809's documentation.  Start here to
find the right file for what you need.

## Conventions

- **English is the canonical language.**  The files under `en/` are
  the source of truth; we update them first, then propagate any
  user-visible changes into the matching files under `ja/`.
- **Japanese is a companion.**  Files under `ja/` are translations
  of the corresponding English file and aim for content parity.  If
  you spot a mismatch, the English version wins.
- **Per-directory indexes are intentionally absent.**  Both `en/`
  and `ja/` are flat directories; this top-level page is the single
  navigation point so it cannot drift out of sync with itself.

## User & reference documentation

These topics are bilingual.  Pick the language column you prefer.

| Topic                                | English (canonical)             | 日本語 (companion)             |
| ------------------------------------ | ------------------------------- | ------------------------------ |
| Quick start / GUI usage              | `en/usage.md`                   | `ja/usage.md`                  |
| Console I/O (`--console-*` flags)    | `en/console_io.md`              | `ja/console_io.md`             |
| Tracing & disassembly                | `en/tracing.md`                 | `ja/tracing.md`                |
| Memory dump editing (GUI)            | `en/memory_editing.md`          | `ja/memory_editing.md`         |
| Troubleshooting (incl. WSL2 / Linux) | `en/troubleshooting.md`         | `ja/troubleshooting.md`        |
| Instruction coverage                 | `en/instruction_status.md`      | `ja/instruction_status.md`     |
| Register viewer integration (GUI)    | `en/regview_integration.md`     | `ja/regview_integration.md`    |
| GUI stack notes (eframe / egui)      | `en/gui_stack.md`               | `ja/gui_stack.md`              |
| OS‑9 / NitrOS‑9 guide                | `en/os9_guide.md`               | `ja/os9_guide.md`              |
| OS‑9 device map templates            | `en/os9_device_map_templates.md`| `ja/os9_device_map_templates.md` |

### One-line summaries

- **Usage** — installation prerequisites, build, key CLI flags, recipe
  walk-throughs (Hello World, block storage), and the
  register-viewer enhancement reference.  Most readers should start
  here.
- **Console I/O** — every `--console-*` flag with examples (stdin
  attach, local echo, file mirror, flush policy, escape sequences).
- **Tracing & disassembly** — using the tracer to follow control
  flow, branch targets, and step granularity.
- **Memory dump editing** — editing memory bytes from the GUI's
  memory pane (read / write / search / mode toggles).
- **Troubleshooting** — common pitfalls; the WSL2 / Linux GUI
  workaround for the WSLg ZINK init failure (`LIBGL_ALWAYS_SOFTWARE=1
  GALLIUM_DRIVER=llvmpipe`) lives in this document.
- **Instruction coverage** — current MC6809 opcode coverage, flag
  semantics, and addressing-mode handling.
- **Register viewer integration** — `--features gui` setup, the
  per-instruction snapshot pattern, listing fallback / Hex view,
  Follow PC, self-modifying-code re-disassembly, and context menu.
- **GUI stack notes** — eframe / egui versions, the `glow`-only
  feature configuration on 0.34, and the deferred App::ui migration
  policy.  Read before bumping eframe.
- **OS‑9 / NitrOS‑9 guide** — current status (**not yet bootable**),
  the four-block roadmap (hardware emulation, OS‑9 artifacts, memory
  map, integration), and the intended end-state workflow.
- **OS‑9 device map templates** — recommended I/O page layout,
  interrupt vector hints, and the timer-at-`$FF10` quick reference
  with CLI examples.

## Design notes & internal memos

These live at `docs/` root (not under `en/` or `ja/`) and are
**developer-facing working documents**.  They are not bilingual,
may lag behind the code, and exist primarily for future-us /
contributors deciding whether and how to pick up a feature.

| File                          | What it covers                                                              |
| ----------------------------- | --------------------------------------------------------------------------- |
| `TODO.md`                     | High-level TODOs and future-work checklist                                  |
| `boot_tab_todo.md`            | Plan for adding a "Boot" tab to the Settings dialog                         |
| `dsl_actions_future.md`       | Boot-script DSL extensions under consideration                              |
| `os9_block_stub.md`           | OS‑9 block-I/O driver stub + RBF integration sketch                         |
| `settings_persistence.md`     | Settings persistence design (CLI > saved > defaults order)                  |
| `memory_edit_ui.md`           | Memory-dump edit UI design memo — Phase 1 shipped, Phase 2 (ASCII / paste / undo / range) is TODO |

## How to update these docs

- **Bilingual files**: edit the English version under `en/` first,
  then port the change into the matching file under `ja/`.  Aim for
  content parity (the line counts will diverge by translation
  overhead, but section structure should match).
- **Internal memos at root**: edit in place.  No translation step
  required.
- **This index**: keep the table above in sync with the file system.
  If you add a new bilingual doc, add a row here and create both the
  `en/` and `ja/` files in the same PR.
