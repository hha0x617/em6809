# MMU sample

`mmu_map_demo.asm` exercises the MC6829 MMU registers directly:

- Programs page `$0C` to different frames for system/user maps, so switching `MMU_MODE` flips which string appears at `$C100`.
- Enables NX and write-protect flags, then writes to the vectors page to show `fault_status` updates.

Build (requires `lwasm` from lwtools):

```bash
lwasm -9 --format=srec -o samples/mmu/mmu_map_demo.s19 samples/mmu/mmu_map_demo.asm
```

Run (console output, MMU logging on):

```bash
cargo run -- samples/mmu/mmu_map_demo.s19 \
  --mmu --console 0xFF00 --console-echo --mmu-log-maps --run
```

Expected console flow:

- A greeting, then user map view (page `$0C` → frame `$0D`, showing `[user map view]`).
- Switch to system map (page `$0C` → frame `$0C`, showing `[system map view]`).
- Write attempt to the protected vectors page, printing `fault_status=$01` when WPROT triggers.
