# OS‑9 / NitrOS‑9 Integration Guide

Typical steps and commands to boot/verify NitrOS‑9/OS‑9 on the emulator. Adjust module layout and entry to match your distribution/environment.

## Preparation

- S‑Records for kernel and drivers (e.g., `nitros9_kernel.s19`, `drivers.s19`)
- Device map templates: `docs/en/os9_device_map_templates.md`
- Boot script templates: `samples/bootscripts/*.boot`

## Boot flow (example)

1. Load S‑Record and attach stdin
   - `target/release/mc6809emu --run --srec path/to/nitros9_kernel.s19 --console-stdin --console-echo ON`
2. Load extra modules if needed
   - `target/release/mc6809emu --run --srec kernel.s19 --srec drivers.s19 --console-stdin`
3. Control start address as needed
   - `... --pc 0xE000`
4. Verify early I/O/branches via trace
   - `target/release/mc6809emu --run --srec kernel.s19 --trace`

## Debugging tips

- Disassemble first 256 bytes: `... --disasm 0xE000:0xE100`
- No input? Confirm `--console-stdin` and device init order
- Slow output? Increase buffering: `--console-flush-every 128`

## References

- Device map examples: `docs/os9_device_map_templates.md`
- Boot scripts: `samples/bootscripts/README.md`, `samples/bootscripts/*.boot`
- Instruction coverage: `docs/en/instruction_status.md`
