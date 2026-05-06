# OS‑9 / NitrOS‑9 Integration Guide

> **⚠️ Status: not yet bootable.**
> Booting OS‑9 / NitrOS‑9 on em6809 is an **aspirational goal**, not a
> currently supported feature.  Several pieces are still missing — for
> example a working disk image / mass‑storage controller surface for
> the OS‑9 device tree, hardware vector wiring sufficient for the
> kernel's cooperative multitasking, and a tested boot script /
> module‑load path.
>
> The steps below describe the **intended** end‑state workflow once
> these pieces land.  Treat the document as a roadmap rather than a
> tutorial; commands shown will not run a real NitrOS‑9 kernel today.

> **Trademarks:** *OS‑9* is a trademark of Microware Systems
> Corporation; *NitrOS‑9* is an independent open‑source project.
> All references in this document are descriptive; this project is
> not affiliated with, endorsed by, or sponsored by either.  See the
> repository top‑level README "Trademarks" section for the full
> notice.

Typical steps and commands to boot/verify NitrOS‑9/OS‑9 on the emulator. Adjust module layout and entry to match your distribution/environment.

## What needs to land before boot works (Roadmap)

The table below lists the work we believe is required for a real
NitrOS‑9 kernel to come up on em6809.  The "Boot flow (example)"
section further down assumes everything below is in place.

### A. Hardware (chip‑level emulation)

| ID | Item | Status | Notes |
|---|---|---|---|
| A‑1 | Real‑chip disk controller | ✗ | The current `BlockDev` exposes a custom register layout, not a chip OS‑9 RBF drivers expect.  Either emulate **WD1773 / WD1793 FDC** or **WD33C93 SCSI**, or write a new RBF driver that targets BlockDev's layout. |
| A‑2 | OS‑9‑rate RTC tick | △ | `TimerDev` exists.  Need 50 / 60 Hz configuration, IRQ vector wiring, and connection to OS‑9 `D.Tick`. |
| A‑3 | CPU instruction completeness | △ | NitrOS‑9 leans on LBSR / MUL / CWAI / SYNC / SEX / ABX, etc.  LEAS/LEAU swap, ABX, TST mem, SBC borrow‑in already fixed; remaining bugs to be flushed by smoke test. |
| A‑4 | SWI / SWI2 / SWI3 push order | ✓ (PR #17) | MC6809 spec compliance landed.  Re‑verify against NitrOS‑9 ABI when boot test runs. |

### B. OS‑9 / NitrOS‑9 artifacts

| ID | Item | Status | Notes |
|---|---|---|---|
| B‑1 | OS‑9 distribution targeted at em6809's hardware | ✗ | OS‑9 / NitrOS‑9 is machine‑specific (Coco / Dragon / MM/1 / …).  We need **init / krn / krnp2 / device descriptors (`/Term`, `/D0`) / device drivers (SCF, RBF) / a boot‑track layout** built or ported for em6809's memory map and peripherals. |
| B‑2 | Module‑aware loader | ✗ | OS‑9 modules carry a header (CRC, name pointer, type/lang/sys revs).  Today's loader handles raw / S‑record only; we need either a module‑aware loader or a boot stub that pulls the kernel entry through the module header. |

### C. Memory map / bootstrap

| ID | Item | Status | Notes |
|---|---|---|---|
| C‑1 | DAT / MC6829 MMU compatibility with OS‑9 L2 | △ | `Mc6829` is implemented; need to confirm that NitrOS‑9's page‑in / page‑out / fork map duplication paths run correctly. |
| C‑2 | Reset vector + boot stub | △ | `$FFFE/$FFFF` -> init module entry, `D.ModDir` initialisation, system stack setup. |
| C‑3 | Interrupt vector wiring | △ | IRQ disambiguation across console RDRF / disk DRDY / RTC tick; FIRQ / NMI policy. |

### D. Integration & validation

| ID | Item | Status | Notes |
|---|---|---|---|
| D‑1 | Boot smoke test | ✗ | Kernel can read `init` from boot disk; shell process reaches `$ ` prompt; `dir` lists the boot disk. |
| D‑2 | Process‑switch validation | ✗ | `procs` lists multiple processes; DAT switching is stable under multi‑process load. |
| D‑3 | Performance | ✓ (probably) | NetBSD boot measured at 44 MIPS / 256 MHz on em68030 (sister project); realistic for em6809 too. |

### Effort sketch

| Block | Effort |
|---|---|
| A‑1 (FDC or RBF driver) | medium‑large (1–2 weeks) |
| A‑2 / A‑3 / A‑4 | small‑medium each (a few days) |
| B‑1 (NitrOS‑9 port) | **large** (several weeks; requires OS‑9 internals knowledge) |
| B‑2 (module loader) | small‑medium |
| C‑1 (DAT compat) | medium (bug‑dependent) |
| D (validation) | medium (iterative) |

**The bottleneck is B‑1.**  Targeting NitrOS‑9 to em6809's hardware
requires reading OS‑9 source (NitrOS‑9 is MIT‑family open source) and
re‑working asm; this is the leg of the project where the time budget
is hardest to predict.

## Preparation

- S‑Records for kernel and drivers (e.g., `nitros9_kernel.s19`, `drivers.s19`)
- Device map templates: `docs/en/os9_device_map_templates.md`
- Boot script templates: `samples/bootscripts/*.boot`

## Boot flow (example)

1. Load S‑Record and attach stdin
   - `target/release/em6809 path/to/nitros9_kernel.s19 --format srec --console-stdin --console-echo`
2. Load extra modules if needed
   - Load additional modules as needed (either via additional inputs if supported, or via GUI workflows).
3. Control start address as needed
   - `... --pc 0xE000`
4. Verify early I/O/branches via tracing

## Debugging tips

- Use the GUI disassembly view to inspect the first 256 bytes.
- No input? Confirm `--console-stdin` and device init order
- Slow output? Increase buffering: `--console-flush-every 128`

## References

- Device map examples: `docs/os9_device_map_templates.md`
- Boot scripts: `samples/bootscripts/README.md`, `samples/bootscripts/*.boot`
- Instruction coverage: `docs/en/instruction_status.md`
