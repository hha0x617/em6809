# Instruction Coverage and Conventions

This document summarizes implemented instructions/modes, disassembly conventions, flags/side effects, and practical verification steps.

## Overview

- The emulator covers major MC6809 instructions and addressing modes: short/long branches, A/B/D arithmetic, loads/stores for X/Y/U/S, memory shifts/clear/jump, and control ops.
- The disassembler spans pages 0x00/0x10/0x11, with unified operand formatting across immediate/direct/indexed/extended and indirect forms; branches show both relative offset and absolute target.

## Disassembly conventions

- Immediate: `#${nn}` (e.g., `#${10}`)
- Direct/Extended: `${addr}` (e.g., `${E000}`)
- Indexed: `±off,REG` (e.g., `-4,X` / `8,U`)
- Indirect: wrap the operand in brackets (e.g., `[10,X]`, `[${E040}]`)
- Branches: `Bcc/LBcc offset -> $${target}`

## Instruction coverage (summary)

- Branches
  - Short `Bcc` set: `BRA`, `BRN`, `BHI`, `BLS`, `BCC`, `BCS`, `BNE`, `BEQ`, `BVC`, `BVS`, `BPL`, `BMI`
  - Long `LBcc` set (pages 0x10/0x11)
  - `BSR` / `LBSR` (samples use LBSR to avoid overflow issues)
- Compare / Load / Store
  - `CMPA/B`, `SUBA/B`, `ADCA/B`, `ADDA/B`, `SBCA/B`, `ANDA/B`, `ORA/B`, `EORA/B`, `BITA/B`
  - `LDA/B/D`, `STA/B/D`, `LDX/U`, `STX/U`, `LDY/S`, `STY/S`, `CPX`, `CPY`, `CMPU`, `CMPS`, `LDS`, `STS`
  - Addressing: immediate/direct/indexed/extended (as defined per instruction)
- Memory ops / Shifts / Rotates / Test
  - `ASL/LSL`, `ASR/LSR`, `ROL`, `ROR`, `TST`, `CLR`, `COM`
  - Targets: direct/indexed/extended; accumulator variants (A/B) where applicable
- Transfer / Control
  - `JMP` (D/X/Y/U/S/extended/indexed/indirect), `JSR` (various modes)
  - `LEAX/Y/U/S`, `PSHS/U`, `PULS/U`, `EXG`, `TFR`
  - `SYNC`, `CWAI`, `MUL`, `DAA`, `SEX`, `ABX`, `ANDCC`, `ORCC`, `SWI/SWI2/SWI3`

## Coverage matrices (operands)

Accumulator (A/B) arithmetic and loads/stores by addressing mode (✓=implemented, —=N/A):

| Group | Instructions | Imm | Direct | Indexed | Extended |
|---|---|:--:|:--:|:--:|:--:|
| 8‑bit arithmetic A/B | `ADDA/B`, `ADCA/B`, `SUBA/B`, `SBCA/B` | ✓ | ✓ | ✓ | ✓ |
| Logic/bit | `ANDA/B`, `ORA/B`, `EORA/B`, `BITA/B` | ✓ | ✓ | ✓ | ✓ |
| Compare A/B | `CMPA/B` | ✓ | ✓ | ✓ | ✓ |
| Load | `LDA/B`, `LDD` | ✓ | ✓ | ✓ | ✓ |
| Store | `STA/B`, `STD` | — | ✓ | ✓ | ✓ |
| Index regs | `LDX/U`, `STX/U` | ✓/— | ✓ | ✓ | ✓ |
| Index regs | `LDY/S`, `STY/S` | ✓/— | ✓ | ✓ | ✓ |
| Compare | `CPX`, `CPY`, `CMPU`, `CMPS` | ✓ | ✓ | ✓ | ✓ |
| Stack ptr | `LDS`, `STS` | ✓/— | ✓ | ✓ | ✓ |

Memory ops and control (—=N/A):

| Group | Instructions | Direct | Indexed | Extended |
|---|---|:--:|:--:|:--:|
| Memory ops | `TST`, `CLR`, `COM` | ✓ | ✓ | ✓ |
| Shifts/rotates | `ASL/LSL`, `ASR/LSR`, `ROL`, `ROR` | ✓ | ✓ | ✓ |
| Jump/JSR | `JMP`, `JSR` | ✓ | ✓ | ✓ |

Branch/page coverage:

| Item | Content | Status |
|---|---|---|
| Short branches | `Bcc` full set | ✓ |
| Long branches | `LBcc` full set (0x10/0x11) | ✓ |
| Subroutines | `BSR`/`LBSR` | ✓ |
| Software interrupts | `SWI`, `SWI2`, `SWI3` | ✓ |

Notes:

- `Imm` stands for immediate. `Indexed` aims to cover variants (5‑bit/8‑bit/16‑bit displacements, indirect).
- For `LDX/U` and `LDY/S`, the Imm column is ✓ only for opcodes that define immediate forms; others are N/A.
- Cycle counts are modeled for instruction cores; platform‑specific memory wait states are not modeled.

## Flags and side effects (policy)

- N/Z/V/C follow the official MC6809 behavior; `DAA/MUL/CWAI` follow spec.
- Compare family (`CMP*`, `CP*`) behaves like subtraction for flags but does not modify data.
- `ANDCC/ORCC` update CCR via bitmasks; `CWAI` updates CCR and handles wait semantics.

## Known conventions/behavior

- Branches show both relative offset and absolute target; traces show post‑resolve PC.
- Indexed indirect uses brackets; composite forms (e.g., D as displacement) follow unified style.

## Verification steps

- Run trace samples with markers:
  - `target/release/mc6809emu --run --srec samples/traces/trace_all.s19 --trace`
- Static coverage check via disasm:
  - `... --disasm 0xE000:0xFFFF`
- Start PC policy:
  - `--pc` > reset vector > SREC entry (if invalid) > first loaded range start
