# Extract em6809-core — Plan

This memo lays out how we split the headless emulator core out of
`em6809` into a separate repository, so emfe_plugins (and any other
embedder) can consume it without dragging in the eframe / egui /
i18n / serde / image / vte / rfd dependency tree the GUI app needs.

## Motivation

* **Portability for emfe_plugins.**  The MC6809 plugin in
  `emfe_plugins` already wraps the em6809 crate, but today it pulls
  the entire em6809 repo in as a submodule.  When emfe_plugins is
  rebuilt the submodule fetch grabs assets, locales, and binary
  source code that the plugin never touches.
* **Cleaner public API surface.**  Right now `em6809::cpu::Cpu` and
  friends are part of the same crate that exposes GUI internals
  behind `#[cfg(feature = "gui")]`.  An extracted `em6809-core`
  draws an explicit, durable boundary: anything pub from
  `em6809-core` is plugin-stable; anything in `em6809` is GUI
  scaffolding that may churn freely.
* **Independent versioning.**  A plugin can pin
  `em6809-core = "0.x.y"` without being held back by GUI
  refactors or eframe bumps in the GUI app, and vice versa.
* **Smaller dependency / clone footprint.**  `--no-default-features`
  on em6809 already produces a slim build, but the *repo* still
  carries the GUI source.  The split moves repo size in line with
  build size.

## Goals

* Extract the GUI-free emulator core into a new public repository
  `hha0x617/em6809-core` (MIT OR Apache-2.0, matching em6809).
* `em6809` keeps building and running unchanged from the user's
  perspective — same CLI, same GUI, same runtime behaviour.
* `emfe_plugins` MC6809 plugin starts depending on `em6809-core`
  directly (via submodule and Cargo path/git dep), with no behaviour
  change for the plugin.
* All existing tests for the core modules move with the code; the
  GUI app's CI keeps passing.

## Non-goals

* **No API redesign in this change.**  Modules and their public
  items move verbatim; renames, restructurings, and pub surface
  audits are deliberately deferred.  Extraction is mechanical so
  the diff is reviewable.
* **No crates.io publish in this round.**  We ship via git
  dependency only until the core stabilises; publishing is a later
  decision once the boundary has settled.
* **No GUI features moving across.**  Anything currently behind
  `#[cfg(feature = "gui")]` stays in `em6809`.

## Repository layout

### Before

```
hha0x617/em6809/                    GUI app + library (single crate)
├── Cargo.toml                       default = ["gui"], optional GUI deps
├── src/
│   ├── lib.rs                       pub mod cpu / bus / bootscript / …
│   ├── main.rs                      GUI binary entry
│   ├── cpu.rs / bus.rs / …          core modules
│   ├── platform/ / prefs.rs         #[cfg(feature = "gui")] modules
│   ├── regview.rs / settings_*.rs   GUI-only (binary-side)
│   └── …
├── tests/                           library tests, all GUI-free today
└── docs/, samples/, locales/, …

hha0x617/emfe_plugins/
└── external/em6809/                 git submodule of em6809 (full repo)
```

### After

```
hha0x617/em6809-core/                NEW.  Headless library only.
├── Cargo.toml                       no default features, ~2 deps
├── src/
│   ├── lib.rs                       pub mod cpu / bus / bootscript / …
│   ├── cpu.rs / bus.rs / …          (verbatim from em6809)
│   └── …
├── tests/                           (verbatim from em6809)
├── README.md                        outline + link back to em6809
├── LICENSE-MIT / LICENSE-APACHE     copies from em6809
└── .github/workflows/ci.yml         minimal: fmt + clippy + test

hha0x617/em6809/                     GUI app (existing repo, slimmed).
├── Cargo.toml                       depends on em6809-core via git
├── src/
│   ├── lib.rs                       `pub use em6809_core::{cpu, …}`
│   ├── main.rs                      unchanged
│   ├── platform/ / prefs.rs         GUI library modules stay here
│   ├── regview.rs / settings_*.rs   GUI binary-side modules stay here
│   └── …
├── tests/                           GUI-only or integration tests stay
└── …

hha0x617/emfe_plugins/
└── external/em6809-core/            CHANGED.  smaller submodule.
```

## Module split

The 10 always-available modules listed in `em6809/src/lib.rs` move
to `em6809-core`.  Everything else stays in em6809.

| Module          | Lines | Destination     | Notes                                                 |
| --------------- | ----- | --------------- | ----------------------------------------------------- |
| `bootscript`    | 782   | **em6809-core** | unchanged                                             |
| `bus`           | ~     | **em6809-core** | `Bus` trait + `Memory` + `WriteTrack`                 |
| `config`        | 168   | **em6809-core** | unchanged                                             |
| `cpu`           | 4389  | **em6809-core** | unchanged (largest single module)                     |
| `debug`         | 1023  | **em6809-core** | unchanged                                             |
| `disasm`        | 1285  | **em6809-core** | unchanged                                             |
| `io`            | 1265  | **em6809-core** | `IoBus`, `Mc6850Dev`, `BlockDev`, `GpioDev`, `TimerDev` |
| `loader`        | 310   | **em6809-core** | S-Record / BIN parser                                 |
| `mmu`           | 317   | **em6809-core** | `Mc6829`                                              |
| `timer`         | 144   | **em6809-core** | unchanged                                             |
| `platform`      | (gui) | em6809          | macOS / Linux / Windows GUI integration               |
| `prefs`         | (gui) | em6809          | TOML config persistence (uses serde / toml)           |

Binary-only modules (`cli`, `devices`, `i18n`, `loader_util`,
`regview`, `settings_native`, `settings_runtime`, `settings_worker`,
`console_mode`, `signals`) and `src/main.rs` itself all stay in
em6809.

### Tests

`em6809/tests/` currently contains 10 integration tests
(`blockdev`, `breakpoints_call_stack`, `console_iobus`, `cpu_basic`,
`cycles_more`, `instruction_boundaries`, `irq_and_protect`,
`os9_priority`, `step_over_out`, `swi_push_order`).  None of them
gate on the `gui` feature; they exercise the core modules only.
**All move to `em6809-core/tests/` verbatim.**

### Samples

`em6809/samples/` includes `bootscripts/`, `traces/`, and various
S-Record images.  Most of these are inputs the GUI app (or any
embedder) might want during testing.  We keep them in em6809 for
now — they're not strictly part of the "core" library — and emfe
will continue to use them via the existing path.  Revisit if it
becomes inconvenient.

### Docs

`docs/en/instruction_status.md` and `docs/ja/instruction_status.md`
are about the CPU core and could move with em6809-core.  Other docs
(usage, regview_integration, gui_stack, …) are GUI-app specific.
For minimal churn we **leave docs in em6809** initially and add a
top-level pointer from em6809-core README; revisit later if the
core grows its own user-facing docs.

> **Update (after Phase 3):** the "revisit later" point landed
> sooner than expected.  Both `instruction_status.md` and
> `os9_device_map_templates.md` (EN + JA) ended up moving to
> [em6809-core/docs/](https://github.com/hha0x617/em6809-core/tree/main/docs)
> together with the AI-collaboration provenance notice on the new
> README.  em6809's `docs/README.md` retains an outbound link
> table for them.  See em6809-core PR #1 and the matching em6809
> cleanup PR for the migration commits.

## API boundary

Today every core module is `pub mod foo;` in `em6809/src/lib.rs`,
so the boundary is already public.  After the move, the same items
are `pub mod foo;` in `em6809-core/src/lib.rs`, and the GUI side
re-exports them so binary code that says `em6809::cpu::Cpu`
continues to compile:

```rust
// em6809/src/lib.rs (after the cut)
pub use em6809_core::{bootscript, bus, config, cpu, debug, disasm,
                     io, loader, mmu, timer};

// GUI-only modules stay in em6809
#[cfg(feature = "gui")]
pub mod platform;
#[cfg(feature = "gui")]
pub mod prefs;
```

That re-export keeps **zero source-level changes** in em6809's
binary code.  Once the dust settles, follow-up PRs can incrementally
import directly from `em6809_core::cpu::Cpu` if we want to retire
the re-export, but that's optional.

## Phase plan

### Phase 0 — this planning PR

Land `docs/extract_em6809_core_plan.md` (this file) so the approach
is reviewable in writing first.  No code changes.

### Phase 1 — create em6809-core

* Create `hha0x617/em6809-core` (public, MIT OR Apache-2.0).
* Seed with the 10 core modules + `tests/` + `Cargo.toml` (no
  default features, deps = `once_cell`, `libc`).
* Add minimal CI workflow (`fmt-check`, `clippy`, `test`).
* Add README pointing at em6809 for context.
* Verify `cargo build` and `cargo test` are green standalone.

### Phase 2 — em6809 consumes em6809-core

* In `em6809/Cargo.toml` add a git dependency on `em6809-core`.
* Replace `pub mod cpu;` etc. with `pub use em6809_core::cpu;`
  re-exports in `em6809/src/lib.rs`.
* Delete the source files now living in em6809-core
  (`src/{bootscript,bus,config,cpu,debug,disasm,io,loader,mmu,timer}.rs`).
* Delete the test files moved to em6809-core (or keep
  GUI-flavoured ones if any).
* Run `cargo build` and `cargo test` for em6809; smoke-test the
  GUI on Windows to confirm nothing regressed.

The Phase 2 PR description must call out the git dep pin (commit
SHA) so reviewers know exactly which em6809-core revision the GUI
sits on.

### Phase 3 — emfe_plugins switches submodule

* In `emfe_plugins`, replace `external/em6809` submodule with
  `external/em6809-core`.
* Update the MC6809 plugin's `Cargo.toml` path / git dep.
* Verify the plugin still builds inside emfe and that runtime
  behaviour is unchanged.

Phases 1, 2, 3 each ship as their own PR (Phase 1 in the new
em6809-core repo, Phase 2 in em6809, Phase 3 in emfe_plugins) so
each step is small and reversible.

## Versioning policy

* Until the core stabilises, em6809-core stays on **`0.x`** and is
  consumed via **git dependency pinned to a commit SHA** (or a
  short-lived tag) — not via crates.io.  This keeps version
  coordination cheap during the early flux.
* `em6809-core` follows semver in spirit even on `0.x`: any
  breaking change to `pub` items in `cpu`, `bus`, `bootscript`,
  `io`, `mmu`, `loader`, `disasm`, `config`, `timer`, or `debug`
  bumps the minor.  Internal cleanups bump the patch.
* When ready to publish to crates.io we promote to `0.x.0` (or
  `1.0` if the API has stabilised in the meantime) and switch
  em6809 / emfe_plugins to the registry version.

## CI for em6809-core

Single workflow `.github/workflows/ci.yml`:

* Triggers: `push` to main, `pull_request` to main, `workflow_dispatch`.
* Matrix: ubuntu-latest only initially (the core is platform-
  agnostic Rust; macOS / Windows can be added later if needed).
* Steps: checkout → install stable toolchain →
  `cargo fmt --check` → `cargo clippy --all-targets -- -D warnings`
  → `cargo test --all-targets`.
* No GUI deps means CI is fast (no apt-install, no eframe build).

`em6809`'s existing CI is unchanged; it just resolves the new git
dependency during its build.

## Risks and open questions

| Risk                                                  | Mitigation |
| ----------------------------------------------------- | ---------- |
| API drift between em6809-core and em6809 (forgetting to bump the GUI-side re-export when the core changes shape) | The re-export pattern (`pub use em6809_core::*`) means changes propagate automatically.  CI on em6809 catches build breakage immediately. |
| Coordinated release pain (two PRs to land a single user-visible change) | While on git deps with SHA pins it's just "bump the SHA in em6809".  The bottleneck only matters once we publish. |
| Cyclic dep risk (em6809-core can't depend on em6809) | Document this in em6809-core's AGENTS.md / README; em6809-core stays headless by definition. |
| Hidden GUI-only assumptions in core modules           | Today none exist (verified by `cargo build --no-default-features`).  Phase 1 standalone CI catches any regression. |
| `samples/` lives in em6809 but core tests might reference it | None of the existing tests/ in em6809 reference `samples/`; if a future core test needs it, copy the specific sample into em6809-core. |
| emfe_plugins also has plugin-level submodules (`emfe_plugins/external/em6809`) — risk of stale references | Phase 3 explicitly addresses this. |

### Open questions for review

1. **Repo name.**  `em6809-core` is the working title.  Alternatives:
   `mc6809-core` (chip-name first), `em6809-emu` (less common
   convention), `em6809-cpu` (narrower scope).  Going with
   `em6809-core` unless the user prefers another.
2. **Hyphen vs underscore.**  Repo `em6809-core`, crate name
   `em6809_core` (hyphen → underscore in Rust crate names).
   Standard Rust convention; flag if anything different is wanted.
3. **License files.**  Copy MIT + Apache 2.0 verbatim from em6809;
   the project memory says `feedback_pr_only_workflow` applies, so
   the new repo uses the same PR-only flow.  Confirm.
4. **Continue git dep, or jump to crates.io publish later?**
   Current plan: git dep until stable, then publish.  No decision
   needed now.
5. **Where do `bus.rs::WriteTrack` and friends live?**  Currently
   `em6809::bus::WriteTrack` is in the core path of the bus module.
   Used by the GUI's `regview` (event-driven re-disassembly).  It's
   a trait wrapper, no GUI deps, so it moves with `bus`.  Verified.

## Decision points

The user has approved option A (separate repo) and asked for this
plan first.  After this PR merges, the recommended order:

1. Create `hha0x617/em6809-core` skeleton (Phase 1).
2. Land Phase 2 PR on `em6809` to consume em6809-core.
3. Land Phase 3 PR on `emfe_plugins` to switch submodule.

Each phase is independent and reversible; if anything goes wrong
mid-stream we can pause without leaving the tree in a half-extracted
state.
