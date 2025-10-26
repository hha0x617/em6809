# Instruction Implementation Status (MC6809)

This document summarizes the current coverage in `src/cpu.rs` and highlights areas recently implemented.

## Implemented Highlights
- Conditional branches (short): BRA, BNE, BEQ, BRN, BHI, BLS, BCC/BHS, BCS/BLO, BVC, BVS, BPL, BMI, BGE, BLT, BGT, BLE
- Conditional branches (long, 0x10): LBcc (all via `cond_eval`)
- Y/CPY/LDY/STY (page 2) variants:
  - LDY: immediate/direct/indexed/extended (0x10 8E/9E/AE/BE)
  - STY: direct/indexed/extended (0x10 9F/AF/BF)
  - CMPY: immediate/direct/indexed/extended (0x10 8C/9C/AC/BC)
- U/CMPS (page 3) variants:
  - CMPU: immediate/direct/indexed/extended (0x11 83/93/A3/B3)
  - CMPS: immediate/direct/indexed/extended (0x11 8C/9C/AC/BC)
- S-register ops (page 2):
  - LDS: immediate/direct/indexed/extended (0x10 CE/DE/EE/FE)
  - STS: direct/indexed/extended (0x10 DF/EF/FF)
- Many core ALU/load/store/indexed addressing modes are present for A/B/X/Y/U and extended/direct/indexed forms.

## Known Gaps / Behavior Notes
- Any 0x10/0x11 sub-opcode not listed above will fall back to `UNIMPL-PFX` (trace-visible). These can be implemented on demand.
- The default `UNIMPL` path for unknown base opcodes remains as a safety fallback.
- Trace strings are concise; if you need mnemonic-rich traces for specific opcodes, open an issue.

## Quick Trace Verification
- Use `samples/traces/trace_all.asm` to exercise the implemented groups.
- Build: `lwasm -9 --format=srec -o trace_all.s19 trace_all.asm`
- Run: `mc6809emu trace_all.s19 --format srec --trace --step 800`

If you find an opcode landing in UNIMPL paths, please share the opcode and addressing form; we will prioritize adding it.
