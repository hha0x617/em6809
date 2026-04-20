pub mod bootscript;
pub mod bus;
pub mod config;
pub mod cpu;
pub mod debug;
pub mod disasm;
pub mod io;
pub mod loader;
pub mod mmu;
pub mod timer;

// Modules that require GUI / i18n / serde dependencies.
#[cfg(feature = "gui")]
pub mod platform;
#[cfg(feature = "gui")]
pub mod prefs;
