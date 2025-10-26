# Repository Guidelines

## Project Structure
- `src/`: Rust sources (`cpu.rs`, `bus.rs`, `io.rs`, `disasm.rs`, `main.rs`, etc.)
- `samples/`: Example programs and traces (e.g., `samples/traces/trace_all.asm`)
- `docs/`: Documentation (e.g., `docs/instruction_status.md`)
- `tests/`: Rust tests

## Build, Run, Format
- Build: `cargo build` (Rust 1.70+ recommended)
- Run: `cargo run -- <image> [options]`
- Format: `cargo fmt --all`
- Lint (optional): `cargo clippy --all-targets`

## Key CLI Options (recent additions)
- `--help`: print detailed usage
- `--run`: run until Ctrl+C (ignores `--step`)
- Disassembler: `--disasm 0xADDR LEN`, `--hex` (bytes + text)
- Console: `--console 0xADDR`, `--console-in "escapes"`, `--console-stdin`,
  `--console-out PATH`, `--console-flush-every N`, `--console-echo`, `--console-tee-stderr`
- PC init order: `--pc` > reset vector (FFFE/FFFF, if valid) > SREC entry (if vector invalid) > first loaded range

## Coding Style
- Rust edition 2021, 4-space indent
- Prefer `snake_case` for items; keep naming consistent with existing modules
- Disasm output style: `#${..}` for immediates, `${..}` for addresses, `±off,REG` (or `[,REG]`) for indexed; long/short branches show `offset -> $target`

## Testing Guidelines
- Start with targeted tests for modified modules, then broader tests
- For CPU/ISA changes, add/extend trace samples in `samples/traces/`
- Run: `cargo test`

## Commit & Pull Request Guidelines
- Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`)
- Keep PRs focused; include trace snippets or `--hex --trace` diffs for emulator changes

## Security & Data
- Do not commit licensed ROMs; use `samples/` and synthetic S19 for tests
- Validate inputs (ROM size, entry points, memory ranges)
