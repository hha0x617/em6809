# Contributing to em6809

Thanks for your interest!  em6809 is a MC6809 software emulator written in
Rust.  It's used both as a standalone GUI app and as a library by
[emfe_plugin_mc6809](https://github.com/hha0x617/emfe_plugins/tree/master/mc6809).

## Getting the source

```bash
git clone https://github.com/hha0x617/em6809.git
```

## Build prerequisites

- **Rust stable** ([install via rustup](https://rustup.rs))

No other system dependencies are required on Windows.  On Linux/macOS the
GUI (`eframe`/`egui`) needs the usual X11 / Cocoa / graphics libs — refer
to the [eframe README](https://github.com/emilk/egui/tree/master/crates/eframe)
if you hit linker errors.

## Building

```bash
# Full build (standalone GUI + library)
cargo build --release

# Library only, no GUI dependencies — same shape that emfe_plugin_mc6809 builds
cargo build --release --no-default-features --lib

# Tests
cargo test

# Style / lint
cargo fmt --all -- --check
cargo clippy --all-targets
```

## Making a change

1. Fork the repository and create a feature branch off `main`.
2. Keep commits small and focused.  CPU correctness fixes deserve their
   own commits with a reproducer in the message body.
3. Run `cargo fmt` + `cargo test` before pushing.
4. Open a pull request against `main`.  CI must pass before merge.

## Commit style

- Subject line ≤ 72 chars, imperative mood, optional `type(scope):` prefix
  (`feat(cpu):`, `fix(mmu):`, `docs:`, `ci:`, `chore:`).
- Body wrapped to 72 chars, focused on motivation and trade-offs.
- For ISA correctness fixes, include the failing case (mnemonic, operands,
  expected vs observed register / memory state) so reviewers can reproduce
  it quickly.

## Reporting bugs / requesting features

Use the issue templates in [`.github/ISSUE_TEMPLATE/`](.github/ISSUE_TEMPLATE/).
Security vulnerabilities go through [`SECURITY.md`](SECURITY.md) instead.

## Code of Conduct

This project follows the [Contributor Covenant 2.1](CODE_OF_CONDUCT.md).
By participating you are expected to uphold those standards.  Reports
of unacceptable behaviour go to the contact address listed in the
Code of Conduct.

## License

By submitting a contribution you agree it will be licensed under the same
dual **MIT OR Apache-2.0** terms as the rest of the repository, without
any additional terms or conditions.
