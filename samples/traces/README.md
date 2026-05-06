# Trace Samples

This folder contains a minimal MC6809 assembly program to exercise key opcodes and verify emulator behavior via `--trace`.

- `trace_all.asm`: covers
  - Short branches: BRN/BHI/BLS/BCC/BCS/BVC/BVS/BPL/BMI/BGE/BLT/BGT/BLE (plus BEQ/BNE)
  - Long branches (page 2, 0x10): LBcc
  - Page-2 Y/CPY/LDY/STY variants (immediate/direct/indexed/extended)
  - Page-2 LDS/STS variants
  - Page-3 CMPS variants

Build (lwtools lwasm):
- `lwasm -9 --format=srec -o trace_all.s19 trace_all.asm`

Run with trace:
- `em6809 trace_all.s19 --format srec --trace --step 1200`
- Increase `--step` as needed to pass further tests.

Optional console markers:
- Add `--console 0xFF00` to see branch results on stdout.
- The program prints markers like `BEQ: T` or `BNE: N` for taken/not-taken.

Notes:
- The program spins at `DONE:`; stop with Ctrl+C or limit via `--step`.
- You can attach a console via `--console 0xFF00`, but it is not required for tracing.
