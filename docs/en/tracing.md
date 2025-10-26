# Tracing, Debugging, and Disassembly

How to inspect instructions, registers, and resolved branches during execution; how to step; how to disassemble and hex dump memory.

## Execution trace

- Run with trace:
  - `target/release/mc6809emu --run --srec path/to/rom.s19 --trace`
- Step one instruction at a time:
  - `target/release/mc6809emu --srec path/to/rom.s19 --step`

Branches show both the relative offset and absolute target; register/CCR changes are visible.

## Disassembly

- Disassemble a memory range:
  - `target/release/mc6809emu --disasm 0xE000:0xE100`
- Narrow focus (e.g., around reset):
  - `... --disasm 0xE000:0xE050`

Operand conventions (summary)

- Immediate: `#${nn}`; Direct/Extended: `${addr}`; Indexed: `±off,REG`; Indirect: `[...]`
- Branches: `Bcc/LBcc offset -> $${target}`

## Hex dump

- Dump a range:
  - `target/release/mc6809emu --hex 0xE000:0xE040`

## Sample ROMs and checks

- Trace sample with markers:
  - `target/release/mc6809emu --run --srec samples/traces/trace_all.s19 --trace`
- START‑first variant:
  - `target/release/mc6809emu --run --srec samples/traces/trace_all_startfirst.s19 --trace`

For instruction coverage and flag semantics, see `docs/en/instruction_status.md`.
