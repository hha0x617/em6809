use eframe::egui;

#[derive(Clone, Debug, Default)]
pub struct SettingsState {
    pub protect_vectors: bool,
    pub use_console: bool,
    pub console_base_hex: String,
    pub console_stdin: bool,
    pub console_echo: bool,
    pub console_out_path: String,
    pub console_flush_every: String,
    pub console_flush_newline: bool,
    pub console_rx_wm: String,
    pub console_irq_hold: String,
    pub console_tee_stderr: bool,
    pub debug_console_log: bool,
    pub debug_block_log: bool,
    pub debug_gpio_log: bool,
    pub debug_mmu_log: bool,
    pub debug_settings_log: bool,
    pub debug_show_origins: bool,
    pub use_block: bool,
    pub block_base_hex: String,
    pub block_file_path: String,
    pub block_irq_enable: bool,
    pub block_firq: bool,
    pub block_irq_hold: String,
    pub use_gpio: bool,
    pub gpio_base_hex: String,
    pub gpio_bits: String,
    pub use_timer: bool,
    pub timer_base_hex: String,
    pub timer_reload_hex: String,
    pub timer_rate_hz: String,
    pub timer_ips: String,
    pub timer_irq_enable: bool,
    pub timer_firq: bool,
    pub timer_start: bool,
    pub use_mmu: bool,
    pub mmu_regs_base_hex: String,
    pub mmu_phys_bytes: String,
    pub mmu_mode_sys: bool,
    pub mmu_edit_sys: bool,
    pub mmu_task: String,
    pub mmu_prot_hex: String,
    pub mmu_log_maps: bool,
    pub mmu_map_frames_sys: [String; 16],
    pub mmu_map_frames_user: [String; 16],
    pub mmu_attr_bytes_sys: [String; 16],
    pub mmu_attr_bytes_user: [String; 16],
    pub mmu_preset_name: String,
}

pub fn open_native_settings(ctx: &egui::Context) {
    crate::settings_native::open_native_settings(ctx);
}

pub fn pump_native_settings(ctx: &egui::Context) {
    crate::settings_native::pump_native_settings_viewport(ctx);
}
