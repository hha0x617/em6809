# Configuration vs. Boot Script — Interaction Reference

em6809 has two layers that can shape device behaviour:

- **Configuration** — `config.toml` + command-line flags.  Static
  setup applied once at startup.
- **Boot script** — `--boot-script PATH` (`.boot` files).  Dynamic
  runtime tweaks applied during execution at PC / step triggers.

Both can target the same registers (MMU mode, console control,
block-device IRQ wiring, etc.), so this page documents the order
they run in, where they can overlap, and the failure modes that
arise when the two disagree.

For the *what* of each layer see:

- Configuration: `docs/settings_persistence.md`
- Boot scripts (DSL + samples): `samples/bootscripts/README.md`,
  `samples/bootscripts/*.boot`, and the planning memo at
  `docs/boot_tab_todo.md`

## Startup sequence

The order is fixed and lives in `src/main.rs` around lines 484–1585
(setup) and 4400–5500 (run loop):

```
Phase 1 — Static setup (deterministic)
─ CLI parse
─ prefs::load()                       config.toml from APPDATA / XDG / Library
─ Effective = resolve(CLI > Persisted > Defaults)
─ Build Bus + attach devices using Effective values:
    console (base / stdin / echo / firq / rx_wm / irq_hold / …)
    block   (base / file / irq / firq / irq_hold)
    timer   (base / reload / irq / firq)
    gpio    (base / bits)
    mmu     (regs_base / phys_bytes / mode / map / attr / prot)
─ Load image (S-Record / BIN)

Phase 2 — Boot script setup (optional)
─ if --boot-script: parse → BootSequencer::new()
    (only builds the trigger list; nothing executes yet)

Phase 3 — Run loop (dynamic, per step)
─ loop:
    boot_seq.on_pre_step(bus, mmu_regs_base, pc, cpu)
        ↓ fires PC / step triggers, executes Actions
    cpu.step()
    boot_seq.on_post_step(bus, mmu_regs_base)
```

Configuration is therefore **always applied before** the boot
script runs — there is no race or ordering ambiguity.  Where the
two interact is when a boot-script `Action` writes the same
register a configuration choice already initialised.

## Interaction matrix

Each boot-script Action and the configuration field it can override:

| Boot-script Action               | Writes to                       | Configuration counterpart                        | Overlap |
| -------------------------------- | ------------------------------- | ------------------------------------------------ | ------- |
| `mode system` / `mode user`      | `mmu_regs_base + 0x11`          | `--mmu-mode`                                     | ✓ overrides at runtime |
| `prot 0xNN`                      | `mmu_regs_base + 0x12`          | `--protect-vectors`, `--prot`                    | ✓ |
| `mmu_map page frame`             | `mmu_regs_base + page`          | `--mmu-map`, `--mmu-map-file`, `--preset`        | ✓ (intended; staged map roll-out) |
| `attr page val`                  | `mmu_regs_base + 0x18 + page`   | `--attr`                                         | ✓ |
| `con_ctrl V`                     | console control register        | `[console]` / `--console-*`                      | ✓ |
| `con_rx_wm N`                    | console RX watermark            | `--console-rx-watermark`                         | ✓ |
| `con_irq_hold N`                 | console IRQ hold                | `--console-irq-hold`                             | ✓ |
| `con_firq on / off`              | console IRQ routing             | `[console].firq`                                 | ✓ |
| `blk_irq on / off`               | block-device IRQ enable         | `[block].irq_enable`, `--block-irq`              | ✓ |
| `blk_firq on / off`              | block-device FIRQ routing       | `[block].firq`, `--block-firq`                   | ✓ |
| `blk_irq_hold N`                 | block-device IRQ hold           | `[block].irq_hold`                               | ✓ |
| `irq_mask on / off`              | CPU `CC.I` flag                 | (CC is not a configuration field)                | runtime-only |
| `firq_mask on / off`             | CPU `CC.F` flag                 | (CC is not a configuration field)                | runtime-only |

In every overlap case the semantic is **last write wins**: the
configuration value is in effect from Phase 1 until a matching
boot-script Action fires in Phase 3.

For most flows this is intended — the configuration sets a sensible
static base, and the boot script tunes things at the *right moment*
(e.g., switch from `mode system` to `mode user` after the kernel
maps land).  The footguns below are when the user has not realised
the boot script will overwrite their configuration.

## Known footguns

### A. Implicit overwrite (mental-model mismatch)

The user persists a value via Settings (e.g., `console.firq = false`)
and assumes it is authoritative for the run.  A boot script with
`con_firq on` silently flips it mid-execution.  The configuration
file is unchanged on disk, but the runtime IRQ routing differs.

*Mitigation today:* none.  Read both layers when debugging.

*Suggested follow-up:* the (still-unimplemented) Settings GUI Boot
tab should surface "Boot script will modify these fields:" so the
relationship is visible.

### B. Silent no-op (boot script targets a disabled device)

If `console.enable = false` (or the equivalent CLI flag is absent),
the bus does not have an attached console.  A boot script with
`con_ctrl 0x11` calls `bus.as_any_mut().downcast_mut::<IoBus<…>>()`
in `bootscript.rs::apply_action`, which **does** succeed (the IoBus
itself is present), then calls `with_console_mut(...)` which is a
no-op when no console is attached — **with no warning logged**.

The visible symptom is "the boot script seems to fire but nothing
changes".

*Mitigation today:* none.  Verify your configuration enables every
device the boot script targets.

*Suggested follow-up:* one-shot warning the first time a console-
or block-targeting Action runs while the corresponding device is
disabled.

### C. MMU base address mismatch (memory corruption risk)

`Action::Mode`, `Action::Prot`, `Action::Map`, and `Action::Attr`
all do `bus.write8(mmu_regs_base + offset, value)` unconditionally.
If the MMU is **not** enabled (`mmu_regs_base = 0` or any value that
does not actually correspond to MMU I/O), these writes land on
plain RAM at `offset`.

For example, an `mmu_regs_base` of `0` plus `prot 0xFF` writes
`0xFF` to address `0x0012`, which is in the OS‑9 direct-page area.

*Mitigation today:* none in the runtime; the parser doesn't
cross-check with configuration.

*Suggested follow-up:* `BootSequencer::on_pre_step` should detect
"MMU-class Action with no plausible `mmu_regs_base`" and either log
a warning or skip the Action.

## Authoring guidelines

When you write a `.boot` file, treat the configuration as the
contract for what's available:

1. Confirm every device the script touches is enabled in the
   configuration the run will actually use (CLI args plus
   `config.toml`).  `--preset-list` and `--emit-boot-template` print
   the relevant base addresses.
2. For MMU scripts, set `--mmu` (or persist `mmu.enable = true`) and
   match the script's assumed `regs_base` to the actual `--mmu-reg-base`.
3. Treat any Action that is also a configuration field as
   *temporary* — the next run with a different boot script (or none)
   will revert to the configuration's value.
4. For long sequences, add a comment block at the top of the
   `.boot` file naming the configuration that the script assumes.
   This is the cheapest way to document the implicit dependency.

## See also

- `docs/settings_persistence.md` — configuration model, file format,
  and CLI > saved > defaults precedence.
- `docs/boot_tab_todo.md` — the planned GUI Boot tab; that document
  also lists the boot-script DSL Actions.
- `samples/bootscripts/README.md` — runnable boot-script examples.
- `src/bootscript.rs` — `BootSequencer` and `Action` definitions.
- `src/main.rs` — the actual startup sequence and run-loop hooks.
