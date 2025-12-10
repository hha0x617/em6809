// runtime module scaffolding will be filled in next step
use crate::loader_util;
use crate::settings_runtime;
use crate::settings_worker;
use crate::{i18n, regview};
use em6809::bootscript::{parse_boot_script, BootSequencer};
use em6809::bus;
use em6809::bus::Bus;
use em6809::cpu::Cpu;
use em6809::io::IoBus;
use em6809::mmu::Mc6829;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

pub struct RunConfig {
    pub stop_requested: Arc<AtomicBool>,
    pub paused_start: bool,
    pub print_cycles: bool,
    pub boot_script_path: Option<String>,
    pub console_stdin: bool,
    pub mmu_regs_base: u16,
}

pub fn run(
    cfg: RunConfig,
    mut cpu: Cpu,
    mut bus: Box<dyn Bus>,
    loaded_ranges: Vec<(u16, u16)>,
    reg_viewer: Option<regview::Viewer>,
) {
    // For now, reuse existing main.rs loop; full extraction will replace this.
    let _ = (cfg, cpu, bus, loaded_ranges, reg_viewer);
    i18n::init_from_env(); // keep side-effect parity for now
}
