//! em6809 — the GUI application.
//!
//! The headless emulator core (CPU, bus, MMU, I/O devices,
//! disassembler, debugger primitives, boot-script runtime) lives
//! in the [em6809-core](https://github.com/hha0x617/em6809-core)
//! crate and is re-exported below for source-level compatibility.
//! Binary code that says `em6809::cpu::Cpu`, `em6809::bus::Memory`,
//! etc. continues to compile against this façade unchanged.
//!
//! GUI-only modules (`platform`, `prefs`) are gated behind the
//! default-on `gui` feature and live in this crate.
//!
//! See `docs/extract_em6809_core_plan.md` for the rationale and
//! phasing of the core-extraction split.

// Re-exports from em6809-core: keeps `em6809::{cpu, bus, …}` working
// after the headless modules moved out of this crate.
pub use em6809_core::{bootscript, bus, config, cpu, debug, disasm, io, loader, mmu, timer};

// GUI-only modules stay in this crate.
#[cfg(feature = "gui")]
pub mod platform;
#[cfg(feature = "gui")]
pub mod prefs;
