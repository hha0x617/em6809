# OS-9 Boot Scripts

This folder contains boot script examples that reproduce the OS‑9 boot phases: initialization, kernel mapping, user transfer, and I/O initialization. Use these with `--boot-script`.

Examples assume console I/O at `0xFF00`. Adjust addresses to your image and hardware.

Run examples:
- Full sequence: `cargo run -- <image> --mmu --boot-script samples/bootscripts/os9l2_full_sequence.boot --console 0xFF00 --trace --step 800`
- Phase only (init+map): `cargo run -- <image> --mmu --boot-script samples/bootscripts/os9l2_boot.boot --console 0xFF00 --step 200`
- Phase only (user switch): `cargo run -- <image> --mmu --boot-script samples/bootscripts/os9l2_user_transfer.boot --step 520`
- Phase only (I/O init): `cargo run -- <image> --mmu --boot-script samples/bootscripts/os9l2_io.boot --console 0xFF00 --step 600`

See also: [`em6809-core/docs/en/os9_device_map_templates.md`](https://github.com/hha0x617/em6809-core/blob/main/docs/en/os9_device_map_templates.md) for I/O map guidance.
