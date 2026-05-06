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

Typical steps and commands to boot/verify NitrOS‑9/OS‑9 on the emulator. Adjust module layout and entry to match your distribution/environment.

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
