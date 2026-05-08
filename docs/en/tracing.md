# Tracing, Debugging, and Disassembly (GUI)

How to inspect instructions, registers, and resolved branches in the GUI; how to step; disassembly and hex dump views.

## Execution trace

- Use the GUI to pause/resume and step execution.
- Traces show registers and CCR changes; branches display both relative offset and absolute target.

## Disassembly

- The listing view shows disassembly; focus around reset or PC as needed.

Operand conventions (summary)

- Immediate: `#${nn}`; Direct/Extended: `${addr}`; Indexed: `±off,REG`; Indirect: `[...]`
- Branches: `Bcc/LBcc offset -> $${target}`

## Hex dump

- Use the hex view to inspect bytes for a selected range.

## Sample ROMs and checks

- Load `samples/traces/trace_all.s19` or `samples/traces/trace_all_startfirst.s19` and observe trace/branches in the GUI.

For instruction coverage and flag semantics, see [`em6809-core/docs/en/instruction_status.md`](https://github.com/hha0x617/em6809-core/blob/main/docs/en/instruction_status.md).
