# Boot Settings Tab (GUI) — TODO / Design Memo

This document captures the background, open issues, proposed solutions,
and policy questions for adding a "Boot" (boot-script) tab to the
Settings dialog.

## Why this is not yet implemented

- Conceptual separation: Settings handles "runtime configuration"
  (devices, display, etc.), whereas boot scripts automate the
  "startup sequence" — different concerns we have so far avoided
  mixing.
- CLI-driven today: only the `--boot-script` / `--emit-boot-template`
  entry points exist.  The GUI command system (`src/regview.rs`)
  has no load / apply / stop path, and no i18n keys are defined.
- DSL is still evolving: `docs/dsl_actions_future.md` lists planned
  extensions.  Locking down the GUI before the DSL stabilises would
  inflate the cost of backward compatibility and UX rework.
- Execution-model complexity: boot scripts hook step execution via
  `BootSequencer::on_pre_step` / `on_post_step` (`src/bootscript.rs`).
  Mid-run apply / replace / stop requires safe pausing, state
  hand-off, and reset semantics — all of which need to align with
  Settings' Apply / Revert / Cancel behaviour.
- Error handling and UX: parse-error reporting, file I/O, template
  application, and log integration together require a full GUI
  workflow that was outside the previous Settings refactor's scope.

## Proposed solution

1) Add the tab and the data model
- New "Boot" tab in Settings (`tab-boot`).
- Add the following fields to `SettingsState` (illustrative):
  - `boot_enable: bool` (enabled for the current session)
  - `boot_script_path: String` (path to load from)
  - `boot_editor_text: Option<String>` (inline edit / preview;
    introduced gradually)
  - `boot_preset_name: String` (template selector)
  - `boot_log_triggers: bool` (whether to log trigger firings)

2) Command / worker integration
- Add to `regview::Command`:
  - `BootApplyFile { path: PathBuf }`
  - `BootApplyText { text: String }`
  - `BootEmitTemplate { name: String }` (populates the editor)
  - `BootControlStart | BootControlPause | BootControlStop | BootReload`
    (optional)
- In the worker loop in `main.rs`, build and hold a
  `BootSequencer::new` constructed from `parse_boot_script`, hooked
  into the existing `on_pre_step` / `on_post_step` callbacks.  Drop
  back to `None` on Stop.
- Apply with the safe pause → swap → resume sequence.  On failure,
  surface the error in the UI and keep the previous state intact.

3) UI / UX
- Phase A: file load + apply / stop + log on/off toggle.
- Phase B: select a template, expand into the editor, edit, apply.
- Phase C: preference integration (auto-apply at startup, persisted).
- Reconcile with the Settings footer's Apply / Revert / Cancel:
  - Boot's sequencer is only swapped on Apply; Revert merely drops
    unapplied UI edits; Cancel just closes the dialog.
  - Apply / stop buttons inside the Boot panel stay independent, so
    immediate operations remain fast.

4) i18n / documentation
- Add Boot-tab keys (tab name, button labels, etc.) to `locales/en`
  and `locales/ja`.
- Append GUI-driven workflow steps and a template list to
  `docs/en/usage.md` (and the JA companion `docs/ja/usage.md`).
- Surface in-app help that points to the existing samples
  (`samples/bootscripts/*.boot`).

5) Logging / observability
- Trace "trigger fired" events through the existing log path
  (e.g., `on_pc 0xE000: con_ctrl 0x11`).
- Optionally expose a small log panel in the GUI (later phase).

## TODO (implementation tasks)

- [ ] UI: add the "Boot" tab to Settings (matching the existing
      auto-size / width-cap conventions).
- [ ] State: add Boot-related fields to `SettingsState`; wire them
      into Open / Revert / Apply baseline handling.
- [ ] Commands: extend `regview::Command` with the Boot variants and
      send them from the GUI to the worker.
- [ ] Worker: thread Boot control (build / swap / stop) through the
      loops in `main.rs`.
- [ ] Parse / apply: surface `parse_boot_script` errors in the UI
      (red text, toast, etc.).
- [ ] File dialog: integrate `rfd` (remembering the last directory
      is optional).
- [ ] Templates: drive `emit_boot_template(NAME)` from the GUI, and
      render the preset list itself in the UI.
- [ ] i18n: add keys to `locales/en` and `locales/ja` (tab name,
      Load, Apply, Stop, templates, log toggle, etc.).
- [ ] Docs: append GUI workflow steps and sample links to
      `docs/en/usage.md` (and `docs/ja/usage.md`).
- [ ] Manual verification: walk an OS‑9 sample and check the trigger
      log / behaviour (including Pause → Apply → Resume).
- [ ] Later: inline editor (syntax highlighting optional), persisted
      auto-apply at startup.

## Open policy questions

- Persistence scope: should the previous Boot configuration auto-
  apply at startup?  Is there a per-profile / per-project scheme
  envisioned?
- Inline editing: stick to file-load only for now, or also offer a
  simple editor?  (To prevent accidental application, do we keep a
  single Apply path?)
- Mid-run swap: does Apply automatically pause → swap → resume, or
  do we require the user to drive that manually?
- Reset / rollback: should applying a new script offer a reset
  (PC / memory reinitialisation)?  Should the GUI grow a "Reset"
  action?
- Template exposure: which template names appear in the UI
  (`os9l2-boot` / `os9l2-io` / `os9l2-full` / …)?
- Log granularity: how detailed should trigger-firing logs be
  (PC / step / action)?  Do we need a dedicated log panel?
- Conflict resolution: how do we order — and label consistently —
  Settings' Apply / Revert / Cancel vs. the Boot tab's immediate
  apply / stop buttons?
- Thread safety: stick with the existing command-queue protocol for
  worker synchronisation, or improve the locking discipline?

## Phase ordering (implementation)

- Phase A: Boot tab (load / apply / stop / log toggle), template
  population, and error display.
- Phase B: inline editor, template selector, save / reload, richer
  UI toasts / guidance.
- Phase C: persisted settings (auto-apply), a "Reset" action, and
  optionally a log panel.

## References

- CLI entry points: `src/main.rs` (`--boot-script` /
  `--emit-boot-template`)
- Parsing / execution: `src/bootscript.rs` (`parse_boot_script` /
  `BootSequencer`)
- GUI: `src/regview.rs` (Settings dialog, command queue)
- Samples: `samples/bootscripts/*.boot`
- Future extensions: `docs/dsl_actions_future.md`
