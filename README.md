MC6809 Emulator (Rust)

Overview
- Cycle-counted MC6809 CPU core written in Rust. Focuses on functional correctness first, then refines cycle accuracy by addressing mode and instruction.

Status
- Implemented: core registers/flags, fetch/decode/execute loop, memory bus, binary/SREC loader.
- Addressing: immediate/direct/extended; indexed (5-bit/8/16-bit offsets, A/B/D offsets, ,R variants with auto inc/dec, PC-relative, basic indirect), absolute extended indirect [n16].
- Instructions: a useful subset including loads/stores (A/B/X), arithmetic/logic for A/B (SUB/CMP/SBC/AND/BIT/EOR/ADC/OR/ADD), branches (short/long), JSR/JMP/RTS/BSR/LBSR, TFR/EXG, PSHS/PSHU/PULS/PULU, SWI/SWI2/SWI3/RTI.
- Interrupts: NMI/IRQ/FIRQ request and service between instructions using standard vectors.
- Cycles: refined indexed extras and baseline costs for many instructions; exceptions/stack ops tuned; overall cycles exposed via CLI.

Build
- Rust 1.70+ recommended.
- Build: `cargo build`
- Run: `cargo run -- <image> [options]`

CLI Options
- `--format bin|srec`: force image format (default: infer by extension `.s19/.srec` => SREC, otherwise BIN)
- `--base 0xADDR`: base address for raw binary load
- `--pc 0xADDR`: set PC after reset/load (overrides entry)
- `--trace`: print trace per instruction
- `--step N`: execute N instructions then return
- `--dump-regs`: print register dump
- `--dump-mem 0xADDR LEN`: hex dump of memory range
- `--cycles`: print total cycles after execution
- `--disasm 0xADDR LEN`: disassemble a memory region and exit
- `--hex`: show instruction bytes alongside disassembly
- `--bp 0xADDR`: add a breakpoint; run stops when PC hits it (can repeat)
- `--mmu`: enable MC6829-like MMU backend (4KB pages, 16 pages, 8 tasks)
- `--mmu-phys BYTES`: physical memory size (default 1048576)
- `--mmu-reg-base 0xADDR`: MMU register window base (default 0xFFA0)
- `--mmu-task N`: initial task (0-7)
- `--mmu-idmap`: identity map current task pages (logical page n -> frame n)
- `--mmu-map P=F`: set page-to-frame mapping (repeatable). Accepts hex like `0x3=0x42` or decimal `3=66`.
- `--mmu-map-file path`: load mappings/attrs from a text file. Syntax per line:
  - `task N` | `mode user|system` | `prot HEX` (write to PROT_CTRL)
  - `map P=F` (page→frame) | `attr P=HEX` (page attributes)
- `--mmu-mode user|sys`: set initial mapping mode
- `--prot HEX`: set `PROT_CTRL` bits directly
- `--attr P=HEX`: set attribute byte for page `P`
- `--console 0xADDR`: attach a simple memory-mapped console at base. Registers:
  - `+0`: TXDATA (write a byte to print; read returns 0)
  - `+1`: STATUS (bit0 TXRDY=1 always; bit1 RXRDY=0)
  - `+2,+3`: reserved
  - `--console-in STRING`: prefill RX FIFO with bytes from STRING (read from `+0`)
  - `--console-rx-wm N`: set RX IRQ watermark (bytes >= N trigger IRQ when RX IRQ enabled)
  - `--console-irq-hold N`: keep console IRQ/FIRQ asserted for N steps after it fires (debounce/hold)
- `--preset os9l2`: apply recommended defaults for OS-9 Level II (enables MMU, keeps id-map; defaults console at `0xFF00` if not specified)
  - Also supports `--preset os9l2-basic`, `--preset os9l2-sysmap`, `--preset os9l2-boot`, `--preset os9l2-io`, `--preset os9l2-ramdisk`
  - Alias: `--mmu-template <name>`
- `--preset-list`: list available presets
- `--boot-script path`: apply a simple boot-time script with triggers (on_pc / on_step) to modify MMU (mode/prot/map/attr) during early execution
- `--emit-boot-template name`: write a template boot script `<name>.boot` (e.g., `os9l2-boot`)

Vectors
- RESET: 0xFFFE/0xFFFF
- NMI: 0xFFFA/0xFFFB
- SWI: 0xFFF8/0xFFF9
- IRQ: 0xFFF6/0xFFF7
- FIRQ: 0xFFF4/0xFFF5
- SWI2: 0xFFF2/0xFFF3 (opcode 0x10 0x3F)
- SWI3: 0xFFF0/0xFFF1 (opcode 0x11 0x3F)

Cycle Accuracy Notes
- Indexed addressing adds extra cycles based on form (5-bit=+0, n8=+1, n16=+2, ,R++/--=+1, indirect adds +3, [n16]=+5).
- Baseline instruction costs are applied on the instruction side (e.g., LDA/STA indexed base=4, LDX/STX indexed base=5).
- Branches: short taken=3 / not=2; long taken=5 / not=4.
- Stack ops: PSH/PUL cost = 5 + bytes moved.
- Exceptions: SWI=19, SWI2/SWI3=20, RTI full=15/min=12; NMI/IRQ=19, FIRQ=10.

Testing
- Unit tests in `tests/` validate behavior and cycle counts for a subset.
- Run: `cargo test`

Planned Work
- Complete instruction set (shifts/rotates, 16-bit arithmetic, compare/test on 16-bit, more branches).
- Disassembler and breakpoints.
- Deeper cycle-accurate bus timing if required (per-memory-access accounting).
MMU/Protection Notes
- `attr` bit0=WPROT, bit1=RPROT, bit2=NX (execute deny). `prot` bits: bit0=WPROT_EN, bit1=IRQ on fault, bit2=RPROT_EN, bit3=NX_EN, bit4=FIRQ on fault.
- `mode`: selects user map (0) or system map (1). Each task has both maps.
- On protection fault, `FAULT_STATUS` (regs+0x13) latches the cause; read to clear.
 - Presets:
   - `os9l2-basic`: user mode, ID-map, vector page WPROT.
   - `os9l2-sysmap`: system mode, ID-map, NX null page, vector WPROT, NX_EN+IRQ.
   - `os9l2-boot`: system mode, ID-map, NX null page, vector WPROT, NX_EN+WPROT_EN+IRQ.
   - `os9l2-io`: system mode, ID-map, NX null page, IO page(0xE) NX, IO isolates to high frame, vector WPROT.
   - `os9l2-ramdisk`: user mode, ID-map, pages 0x08..0x0B remapped to high frames for RAM-disk, vector WPROT.
Boot Script
- Syntax (one per line; hex or decimal accepted):
  - `on_pc 0xADDR mode user|sys`
  - `on_pc 0xADDR prot 0xHEX`
  - `on_pc 0xADDR map P=F`
  - `on_pc 0xADDR attr P=HEX`
  - `on_step N mode user|sys` (after N instructions)
  - Lines starting with `#` or `//` are comments
- Example (see `samples/bootscripts/os9l2_sequence.boot`):
  - System mode at RESET, enable protections, map kernel pages, switch to user after 512 steps, then enable console RX/FIRQ with watermark/hold.
