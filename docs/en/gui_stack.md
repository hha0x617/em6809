# GUI Stack Notes (eframe / egui)

This page documents how em6809 wires up its desktop GUI on top of
eframe / egui, the feature flags we pick, and the engineering
decisions made when we last bumped the dependency.  It is aimed at
contributors touching the GUI layer or upgrading dependencies — end
users don't need to read it.

## Current versions

| Crate | Version | Notes |
|-------|---------|-------|
| `eframe` | `0.34.x` | `default-features = false`; we explicitly enable `default_fonts`, `glow`, `wayland`, `x11`. |
| `egui_extras` | `0.34.x` | Default features. |
| `winit` | `0.30.x` | Indirect (eframe-managed) plus our own dep for raw window APIs. |

`Cargo.toml` keeps `eframe` and `egui_extras` pinned to the same
minor; bumping one without the other will fail the link step.

## Why `default-features = false` + `glow`

`eframe 0.34`'s default feature set turns on `wgpu`, which transitively
pulls `wgpu-hal 29`.  That depends on the **`windows = 0.62`** crate.
However, another transitive dependency, `gpu-allocator 0.28` (the
latest stable as of 2026-05), is still pinned to **`windows = 0.61`** —
and the two `windows` versions don't unify across
`windows_core::Param` traits.  The Windows build of em6809 fails at
compile time with ten `E0277` errors inside `wgpu-hal`.

The fix that works today is to drop `wgpu` from eframe's feature set
and rely on `glow` (OpenGL via `egui_glow`) instead.  em6809 has been
rendered through `glow` on every prior version (0.29 through 0.33), so
this is functionally a no-op for end users.

We will revisit the `wgpu` feature when `gpu-allocator` ships a release
that depends on `windows = 0.62` (or later) so the trait unification
hole closes.  Track it via `cargo tree -i windows`.

## Deferred deprecation: `Panel::show(ctx, …)` → `App::ui` migration

eframe 0.34 deprecated `Panel::show(ctx, …)` in favour of
`Panel::show_inside(ui, …)` driven by a new required trait method
`App::ui(&mut Ui, &mut Frame)`.  The previous `App::update(&mut Context, &mut Frame)`
method still exists, is still called every frame by eframe's native
loop, and does *not* error — it simply emits a deprecation warning.

em6809's entire frame logic currently lives in `App::update`.  The
`update` body is roughly **850 lines** and threads the `Context`
through several helpers and three viewport closures (Breakpoints, Call
Stack, Console).  Each viewport closure receives only a `Context`
from `Context::show_viewport_immediate` / `_deferred`, not a `Ui`,
which means a faithful migration to `show_inside` would also require
introducing a `Ui` wrapper at each viewport entry point (e.g. via
`egui::Area::new("root").show(ctx, |ui| { … show_inside(ui, …) })`).

For the 0.34 bump (PR #36) we made the **explicit decision to defer**
the structural migration:

1. The diff for migrating ~850 lines of `update` plus three viewport
   closures was judged too large to land in a single dependency-bump
   PR without harming reviewability.
2. The deprecated `show()` is a thin wrapper around `show_inside()`
   internally, so there is no functional difference today.
3. Active GUI development has moved to **emfe** (a Windows-focused
   plugin-based emulator framework — see the README's "Project
   direction" section).  em6809's eframe front-end is in maintenance
   mode, and a large UI refactor here is poor return on investment.

Concretely, the bump PR replaced the trivial deprecations (renames
like `Rounding` → `CornerRadius`, `Margin::same(f32)` → `Margin::same(i8)`,
`Ui::close_menu` → `Ui::close`, etc.) and pinned the remaining
`Panel::show(ctx, …)` warnings with `#[allow(deprecated)]` on the
**nine** functions that own those calls:

| File | Function | Why |
|------|----------|-----|
| `src/regview.rs` | `App::update` for `RegApp` | top-level menu bar + layout |
| `src/regview/gui/regs_ui.rs` | `render_regs_panel` | central registers panel |
| `src/regview/gui/list_ui.rs` | `render_listing_panel` | right-side listing |
| `src/regview/gui/mem_ui.rs` | `render_memory_panel` | right-side memory |
| `src/regview/gui/console_ui.rs` | `render_console` | inside Console viewport |
| `src/regview/gui/console_ui.rs` | `show_console_viewport` | viewport wiring |
| `src/regview/gui/breakpoints_ui.rs` | `show_breakpoints_viewport` | viewport wiring |
| `src/regview/gui/call_stack_ui.rs` | `show_call_stack_viewport` | viewport wiring |
| `src/settings_native.rs` | `SettingsNativeApp::draw` | Settings window |

Each annotation has a short comment pointing back to this document so
the deferred migration stays discoverable.

### When to lift the `#[allow(deprecated)]`

Trigger the migration when **any** of the following becomes true:

- egui announces a release that will *remove* `Panel::show(ctx, …)`
  (compile error rather than warning) — at that point we have no
  choice and the migration must land in the next dependency bump.
- emfe stabilises and we revisit em6809's GUI front-end with non-
  trivial improvements; rolling the migration into that work is
  cheaper than doing it in isolation.

Migration outline (kept brief; expand when actually executing):

1. Change `RegApp::update`'s signature to
   `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame)`.
2. At the top of the new body, take `let ctx = ui.ctx().clone();`
   so existing `ctx.input(…)` / `ctx.send_viewport_cmd(…)` calls keep
   working unchanged.
3. Replace each `Panel::show(ctx, |inner| { … })` with
   `Panel::show_inside(ui, |inner| { … })`; thread `ui` down into the
   `regs_ui::render_regs_panel` / `list_ui::render_listing_panel` /
   `mem_ui::render_memory_panel` helpers (their signatures change
   from `(&mut RegApp, &Context)` to `(&mut RegApp, &mut Ui)`).
4. For the Breakpoints / Call Stack / Console viewport closures —
   wrap the body in `egui::Area::new("root").show(ctx, |ui| { … })`
   and convert their `CentralPanel::default().show(ctx, …)` calls to
   `show_inside(ui, …)`.  Or accept that
   `Panel::show(ctx, …)` keeps making sense for top-level viewport
   roots and keep the `#[allow(deprecated)]` there indefinitely; egui
   may relax the deprecation for that specific case.
5. Provide a stub `fn update(&mut self, _ctx, _frame)` so the
   deprecated trait method satisfies any caller that might still
   reach for it.
6. Remove every `#[allow(deprecated)]` listed above and run
   `cargo check --all-targets` to confirm the warning surface is
   genuinely empty.

## Linux Wayland multi-viewport freeze

Independent of this dependency stack, em6809 has a long-standing
deadlock on Linux/Wayland (specifically Mutter on Ubuntu 24.04,
verified on a Parallels VM) when the Console viewport ends up fully
occluded behind the main window — we documented the symptom and
workarounds in the main README ("Known Issues").  The bump from 0.29
to 0.34 was tested against this scenario and **does not fix it**; we
treat it as a design-fit issue between egui's multi-viewport model
and Wayland's surface lifecycle, not as a bug we expect upstream to
address.
