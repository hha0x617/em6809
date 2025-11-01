This application was generated with the assistance of AI tools: Codex CLI as part of the vibe coding workflow.

# em6809

## Overview

em6809 is a software emulator for the MC6809. It provides a run-oriented CLI, rich console I/O controls, instruction tracing with resolved branch targets, and a disassembler. It can be used to boot and validate OS‑9/NitrOS‑9 setups.

## Install

- Build with Rust (stable):
  - `cargo build --release`

## Quick Start (GUI-only)

- Run and open GUI (paused by default):
  - `target/release/em6809 samples/hello/hello.s19 --format srec --console 0xFF00`
- Show help:
  - `target/release/em6809 --help`
- Start from an explicit PC:
  - `target/release/em6809 samples/hello/hello.s19 --format srec --console 0xFF00 --pc 0xE000`

## Common Use Cases

- Attach stdin with local echo: `... samples/echo/echo.s19 --console 0xFF00 --console-stdin --console-echo`
  (Disasm/Memory/Trace are available in the GUI window)

## Documentation

- CLI usage: `docs/en/usage.md`
- Console I/O: `docs/en/console_io.md`
- Tracing/Disassembly: `docs/en/tracing.md`
- OS‑9/NitrOS‑9 guide: `docs/en/os9_guide.md`
- Troubleshooting: `docs/en/troubleshooting.md`
- Instruction coverage: `docs/en/instruction_status.md`
- Register viewer integration (GUI): `docs/en/regview_integration.md`
- Environment variables (GUI): see `docs/en/regview_integration.md#environment-variables`

### Recent GUI viewer enhancements

- Listing fallback when `.lst` is missing (auto‑generated hex disassembly).
- Hex view toggle and Follow PC highlight with auto‑scroll.
- Event‑driven re‑disassembly for self‑modifying code (Full span / PC window modes).
- Right‑click context menu on listing lines: Run to here, Set PC to this address, Cancel.
- Hover highlight on listing lines; panes refresh immediately after context actions.

## Links

- Repository: https://github.com/hhagiwara/mc6809emu
- Issues: https://github.com/hhagiwara/mc6809emu/issues
- Releases: https://github.com/hhagiwara/mc6809emu/releases
- Docs index: `docs/README.md`
- English docs: `docs/en/`
- Japanese docs: `docs/ja/`
- Samples (traces, boot scripts): `samples/`

## References

- Motorola 6809 (overview): https://en.wikipedia.org/wiki/Motorola_6809
- MC6809 documentation archive (Bitsavers): http://bitsavers.org/components/motorola/6809/
- S‑Record format: https://en.wikipedia.org/wiki/SREC_(file_format)
- NitrOS‑9 project: https://sourceforge.net/projects/nitros9/
- 6809 community resources: http://www.6809.org.uk/
- LWTOOLS: https://www.lwtools.ca/

## Contributing and Policies

- Development/Operations: `AGENTS.md`
- Code of Conduct/Security: `CODE_OF_CONDUCT.md` / `SECURITY.md` (if present)

## License

MIT OR Apache‑2.0 (dual license). See `LICENSE-MIT` and `LICENSE-APACHE`.
