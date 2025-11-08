# Settings Persistence — Design and Plan

This document defines how the emulator persists Settings dialog values, how they interact with command‑line options, and how the final effective values are presented in the GUI.

## Goals
- Persist user preferences across runs in a simple, human‑readable format.
- Resolve configuration deterministically with clear precedence:
  - Command-line options (highest)
  - Persisted settings file
  - Built-in defaults (lowest)
- Surface the final effective values in the Settings dialog so users can see what actually applies at runtime.
- Keep the runtime safe: validate inputs; avoid breaking runs if the settings file is malformed.

## Precedence (authoritative)
- CLI overrides persisted values and defaults (per requirement).
- Persisted values apply when the CLI does not specify an option.
- Defaults apply when neither CLI nor persisted provide a value.

## File format and location
- Format: TOML via `serde` (`serde`, `toml` crates).
- Location (use `directories` crate to resolve per OS):
  - macOS: `~/Library/Application Support/em6809/config.toml`
  - Linux: `${XDG_CONFIG_HOME:-~/.config}/em6809/config.toml`
  - Windows: `%APPDATA%\em6809\config.toml`
- Behavior: create directory if missing; on read error, log a warning and continue with defaults.

## Data model
- `Prefs` (persisted): mirrors SettingsState fields relevant for persistence.
  - Suggested top-level sections: `general`, `console`, `block`, `gpio`, `timer`, `mmu`, `ui`.
  - Make fields `Option<T>` so we can distinguish “not set” vs “set to default value”.
- `CliOverrides`: concrete values from CLI parsing (`Option<T>` per field). Only populated for flags actually provided.
- `Effective`: resolved values + source per field.
  - `enum Source { Default, Persisted, Cli }`

Example (TOML):
```
[console]
base = "0xFF00"
stdin = true
local_echo = true
flush_every = "1"

[gpio]
base = "0xFF40"
bits = "8"
on_color = "#FF0000"
```

## Resolution logic
- On startup:
  1) Load `Prefs` from file (if present).
  2) Parse CLI into `CliOverrides`.
  3) Resolve into `Effective` per field: `Cli > Prefs > Defaults`.
  4) Initialize GUI `settings_baseline` from `Effective` values so the dialog shows what applies.
- On Apply:
  - Write new `Prefs` from the Settings dialog (only values that are persistable).
  - CLI remains authoritative for the current run; runtime effective values stay CLI if overridden. Next launch (without CLI flags) will use persisted.
  - Optional: show an info note when a field is currently CLI‑overridden.

## UI: showing effective values and overrides
- For each row/field in Settings:
  - Display the current editable value (the persisted candidate) in the input.
  - Display a subtle right‑aligned note: `Effective: <value> (CLI)` / `(Saved)` / `(Default)`.
  - If overridden by CLI, consider disabling the field or leave editable but add a small badge “Overridden by CLI” with a tooltip “Will take effect next launch without CLI flags”.
- Footer note: “CLI options override saved settings. Fields marked ‘CLI’ show current runtime values.”
- i18n keys (to add):
  - `effective = Effective: {value}`
  - `source-cli = CLI`
  - `source-persisted = Saved`
  - `source-default = Default`
  - `note-cli-overrides = Note: CLI options override saved settings.`

## Scope of persistence
- Persist “device wiring” and UX preferences:
  - General: `protect_vectors`
  - Console: `enable`, `base`, `stdin`, `echo`, `out_path`, `flush_every`, `flush_newline`, `rx_watermark`, `irq_hold`, `tee_stderr`
  - Block: `enable`, `base`, `file_path`, `irq_enable`, `firq`, `irq_hold`
  - GPIO: `enable`, `base`, `bits`, `on_color`
  - Timer: `enable`, `base`, `reload`, `irq_enable`, `firq`, `start`
  - MMU: `enable`, `regs_base`, `phys_bytes` (restart‑needed), runtime controls (`mode`, `task`, `prot`, maps/attrs`) as optional persistence
  - UI: language, recently used paths (optional), window prefs (optional)
- Items requiring restart (e.g., `mmu_phys_bytes`) should be persisted but editing indicates “requires restart”.

## Failure handling
- Malformed config: log warning; ignore offending keys; continue with defaults/CLI.
- Write failures: surface a non‑blocking GUI toast (and stderr) and keep running.

## Implementation plan
- Add `src/prefs.rs`:
  - `Prefs` struct (serde Serialize/Deserialize) with `Option<T>` fields.
  - `fn load() -> Prefs` and `fn save(&Prefs)` using `directories`.
- Extend CLI parsing in `main.rs` to populate `CliOverrides` (struct).
- Add `Effective` calculation helper to merge `CliOverrides`, `Prefs`, and defaults.
- Initialize `RegApp.settings_baseline` with `Effective` at startup.
- Update Settings UI (`regview.rs`) to render right‑side effective/source labels and a top note about CLI precedence. For CLI‑overridden fields:
  - Either disable inputs, or leave editable and show “Overridden by CLI” (recommended: leave editable so users can save for next run).
- On Apply:
  - Persist values from the dialog to `Prefs`.
  - Recompute `Effective` and refresh the right‑side labels.
- i18n: add the keys listed above in `locales/en/em6809.ftl` and `locales/ja/em6809.ftl`.

## Mapping (fields ↔ CLI flags)
- Console: `--console`, `--console-stdin`, `--console-echo`, `--console-out`, `--console-flush-every`, `--console-flush-newline`, `--console-tee-stderr`, `--console-rx-watermark`, `--console-irq-hold`
- Block: `--block`, `--block-base`, `--block-file`, `--block-irq`, `--block-firq`, `--block-irq-hold`
- GPIO: `--gpio`, `--gpio-bits`, (color is GUI‑only; persist only)
- Timer: `--timer`, `--timer-reload`, `--timer-irq`, `--timer-firq`, `--timer-start`
- MMU: `--mmu`, `--mmu-reg-base`, `--mmu-phys`, `--mmu-task`, `--mmu-mode`, `--protect-vectors`, `--prot`, `--attr`, `--mmu-map`, `--mmu-map-file`, `--preset`
- Boot (future GUI tab): `--boot-script`, `--emit-boot-template` (GUI persistence of last script path optional)

## Open questions
- Disable vs. editable‑but‑overridden for CLI‑locked fields?
- Persist “last used” paths or make them a recent‑list?
- Persist runtime MMU maps/attrs or keep them session‑only?
- Group save semantics: save all tabs on Apply, or per‑tab Apply? (currently global Apply).

