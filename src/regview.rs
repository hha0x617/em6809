#![allow(dead_code)]

mod settings;
pub use settings::SettingsState;
// Expose GPIO snapshot publisher to outer modules
#[allow(unused_imports)]
pub use em6809::io::publish_gpio_broadcast as publish_gpio_snapshot;

use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct RegSnapshot {
    pub a: u8,
    pub b: u8,
    pub d: u16,
    pub x: u16,
    pub y: u16,
    pub u: u16,
    pub s: u16,
    pub dp: u8,
    pub pc: u16,
    pub cc: u8, // E F H I N Z V C bits
}

impl RegSnapshot {
    pub fn with_ab(mut self, a: u8, b: u8) -> Self {
        self.a = a;
        self.b = b;
        self
    }
    pub fn with_d(mut self, d: u16) -> Self {
        self.d = d;
        self
    }
    pub fn with_xy(mut self, x: u16, y: u16) -> Self {
        self.x = x;
        self.y = y;
        self
    }
    pub fn with_us(mut self, u: u16, s: u16) -> Self {
        self.u = u;
        self.s = s;
        self
    }
    pub fn with_dp_pc(mut self, dp: u8, pc: u16) -> Self {
        self.dp = dp;
        self.pc = pc;
        self
    }
    pub fn with_cc(mut self, cc: u8) -> Self {
        self.cc = cc;
        self
    }
}

    pub mod gui {
        use super::*;
        use crate::i18n::LOADER;
        use crate::settings_worker::{self, SettingsPayload, WorkerCommand};
        use eframe::{egui, App};
    use egui_extras::{Column, TableBuilder};
    use i18n_embed_fl::fl;
    use memmap2::Mmap;
    use std::collections::HashMap;
    use std::fs::File;
    use std::path::PathBuf;

    // Type aliases to simplify complex sync types
    type GpioSyncState = Option<(bool, u8, u32, u32)>;
    type MmuSyncState = Option<(bool, u8, [u16; 16])>;

        // macOS-specific helpers moved to crate::platform::macos

        use once_cell::sync::OnceCell;

        pub fn parse_u8_hex(s: &str) -> Option<u8> {
            let sl = s.trim();
            if sl.is_empty() {
                return None;
            }
            let t = sl.trim_start_matches("0x").trim_start_matches("0X");
            u16::from_str_radix(t, 16).ok().map(|v| (v & 0xFF) as u8)
        }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum DeviceTab {
        Console,
        Block,
        Gpio,
        Timer,
        Mmu,
    }

    #[derive(Clone, Debug)]
    pub enum Command {
        SetPc(u16),
        RunUntil(u16),
        ApplyRegs(RegSnapshot),
        MemWrite(Vec<(u16, u8)>),
        OpenImage {
            path: std::path::PathBuf,
            base: Option<u16>,
        },
        UiPromptBinBase(std::path::PathBuf),
        UiLoadLst(std::path::PathBuf),
        RefreshHexListing,
        GpioSetOut(u32),
        UiSetGpio(bool, u8, u32, u32),
        UiSetMmu(bool, u8, [u16; 16]),
        UiSetTimer(bool, bool, bool),
        UiSetBlockStatus {
            present: bool,
            base: Option<u16>,
            backing_path: Option<String>,
            backing_online: bool,
            last_cmd: u8,
            last_data: u8,
            status: u8,
        },
        UiSetGpioOnColor(u32),
        UiOpenSettings,
        /// Request opening Settings in an external top-level OS window
        UiOpenSettingsExternal,
        /// Set baseline settings + origins (0=Default,1=Saved,2=CLI) and optional GPIO color
        UiSetSettingsBaseline(SettingsState, Vec<(String, u8)>, Option<u32>),
        /// Worker acknowledgement that Apply/Save completed for a dialog instance
        UiAckSettings {
            instance_id: u64,
        },
        ApplyConsole {
            enable: bool,
            base: Option<u16>,
            out_path: Option<String>,
            stdin: bool,
            echo: bool,
            flush_every: Option<usize>,
            flush_newline: bool,
            rx_watermark: Option<usize>,
            irq_hold: Option<u32>,
            tee_stderr: bool,
        },
        ApplyBlock {
            enable: bool,
            base: Option<u16>,
            file_path: Option<String>,
            irq_enable: bool,
            firq: bool,
            irq_hold: Option<u32>,
        },
        ApplyGpio {
            enable: bool,
            base: Option<u16>,
            bits: Option<u8>,
        },
        ApplyTimer {
            enable: bool,
            base: Option<u16>,
            reload: Option<u16>,
            irq_enable: bool,
            firq: bool,
            start: bool,
        },
        // MMU controls (Phase 1)
        ApplyMmu {
            sys_mode: Option<bool>,
            task: Option<u8>,
            prot: Option<u8>,
            log_maps: Option<bool>,
            maps: Vec<(usize, u16)>, // (page, frame)
            attrs: Vec<(usize, u8)>, // (page, attr)
        },
        MmuIdentityMap,
        // MMU controls (Phase 2)
        ApplyMmuPreset {
            name: String,
        },
        ApplyMmuMapFileText {
            text: String,
        },
        /// Persist settings snapshot to disk
        SavePrefs {
            settings: SettingsState,
            gpio_on_color: u32,
            instance_id: u64,
        },
    }

    pub(super) static GLOBAL_CMDS: OnceCell<Arc<Mutex<Vec<Command>>>> = OnceCell::new();
    pub(super) static GLOBAL_SETTINGS: OnceCell<
        Arc<Mutex<(SettingsState, HashMap<String, u8>, Option<u32>)>>,
    > = OnceCell::new();
    pub(super) static FORCE_IN_APP_SETTINGS: OnceCell<bool> = OnceCell::new();
    pub static SHOW_ORIGINS_OVERRIDE: OnceCell<AtomicBool> = OnceCell::new();

    pub fn set_force_in_app_settings(v: bool) {
        let _ = FORCE_IN_APP_SETTINGS.set(v);
    }

    pub fn force_in_app_settings() -> bool {
        *FORCE_IN_APP_SETTINGS.get().unwrap_or(&false)
    }

    pub fn set_show_settings_origins(on: bool) {
        if let Some(flag) = SHOW_ORIGINS_OVERRIDE.get() {
            flag.store(on, Ordering::SeqCst);
        } else {
            let _ = SHOW_ORIGINS_OVERRIDE.set(AtomicBool::new(on));
        }
    }

    pub struct RegApp {
        pub snap: Arc<Mutex<RegSnapshot>>,
        pub paused: Arc<AtomicBool>,
        pub step_requests: Arc<AtomicUsize>,
        pub lst_mmap: Option<Arc<Mmap>>,
        pub lst_text: Option<Arc<Mutex<String>>>,
        pub use_hex: Arc<AtomicBool>,
        pub auto_hex_on_edit: Arc<AtomicBool>,
        pub scroll_to_pc_req: Arc<AtomicBool>,
        pub follow_pc_flag: Arc<AtomicBool>,
        pub span_policy: Arc<AtomicUsize>, // 0=FullSpan, 1=PcWindow
        pub window_len: Arc<AtomicUsize>,  // bytes for PcWindow
        pub cmds: Arc<Mutex<Vec<Command>>>,
        pub ja_font: Option<Arc<Vec<u8>>>,
        pub fonts_applied: bool,
        pub menu_fixed: bool,
        // Windows-only: previously used to override taskbar title; removed to keep caption unchanged
        pub use_japanese: bool,
        pub last_pc_for_scroll: u16,
        // File open dialog state
        pub open_dialog_active: bool,
        pub open_pending_path: Option<std::path::PathBuf>,
        pub open_bin_base_text: String,
        #[cfg(target_os = "linux")]
        pub csd: bool,
        #[cfg(target_os = "linux")]
        pub csd_maximized: bool,
        #[cfg(windows)]
        pub about_open: bool,
        #[cfg(windows)]
        pub about_icon_tex: Option<egui::TextureHandle>,
        #[cfg(target_os = "linux")]
        pub about_open: bool,
        #[cfg(target_os = "linux")]
        pub about_icon_tex: Option<egui::TextureHandle>,
        // Memory view state
        pub mem_follow_pc: Arc<AtomicBool>,
        pub mem_window_len: Arc<AtomicUsize>,
        pub mem_window_start: Arc<AtomicUsize>,  // u16 widened
        pub mem_buf: Arc<Mutex<(u16, Vec<u8>)>>, // current window snapshot
        pub mem_hl: Arc<Mutex<Option<(u16, usize)>>>, // highlight range
        pub mem_utf8: Arc<AtomicBool>,
        pub mem_jump_text: String,
        // GPIO LED color when ON
        pub gpio_on_color: egui::Color32,
        // Baseline color to support Revert in Settings
        pub gpio_on_color_baseline: egui::Color32,
        // Text field for direct hex input (e.g., 0xFF0000)
        pub gpio_on_color_text: String,
        pub mem_len_text: String,
        pub quit_flag: Arc<AtomicBool>,
        // Numeric UX toggle (DragValue vs TextEdit)
        pub mem_use_drag: bool,
        // Memory edit (phase 1)
        pub mem_edit_mode: bool,
        pub mem_pending: HashMap<u16, u8>,
        pub mem_selected: Option<u16>,
        pub mem_edit_buf: String,
        pub mem_focus_next: bool,
        // Caret boundary tracking for neighbor navigation
        pub mem_last_sel_addr: Option<u16>,
        pub mem_last_at_start: bool,
        pub mem_last_at_end: bool,

        // Editing state for registers and CC flags
        pub edit_mode: bool,
        pub edit_initialized: bool,
        pub edit_a: String,
        pub edit_b: String,
        pub edit_d: String,
        pub edit_x: String,
        pub edit_y: String,
        pub edit_u: String,
        pub edit_s: String,
        pub edit_dp: String,
        pub edit_pc: String,
        pub cc_e: bool,
        pub cc_f: bool,
        pub cc_h: bool,
        pub cc_i: bool,
        pub cc_n: bool,
        pub cc_z: bool,
        pub cc_v: bool,
        pub cc_c: bool,
        pub input_error: Option<String>,
        // GPIO view state (mirrors device snapshot)
        pub gpio_present: bool,
        pub gpio_bits: u8,
        pub gpio_out: u32,
        pub gpio_dir: u32,
        pub block_present: bool,
        pub block_base: Option<u16>,
        pub block_backing_path: Option<String>,
        pub block_backing_online: bool,
        pub block_last_cmd: u8,
        pub block_last_data: u8,
        pub block_status: u8,
        // Emulator -> GUI sync (shared across threads)
        pub gpio_sync: Arc<Mutex<GpioSyncState>>,
        pub mmu_sync: Arc<Mutex<MmuSyncState>>,
        // MMU grid
        pub mmu_present: bool,
        pub mmu_sys_mode: bool,
        pub mmu_task: u8,
        pub mmu_map_prev: [u16; 16],
        pub mmu_map_cur: [u16; 16],
        pub timer_present: bool,
        pub timer_pending: bool,
        pub timer_running: bool,
        // Settings dialog
        pub settings_open: bool,
        pub settings_tab: usize,
        pub settings: SettingsState,
        // Last applied/saved settings snapshot (baseline for Revert)
        pub settings_baseline: SettingsState,
        // Bump this each time Settings opens to reset egui's remembered window size
        pub settings_instance: u64,
        pub settings_origins: HashMap<String, u8>,
        pub settings_busy: bool,
        pub settings_busy_since: Option<std::time::Instant>,
        // Console status (for quick panel order)
        pub console_enabled: bool,
        pub console_base: Option<u16>,
        pub dev_tab: DeviceTab,
    }

    impl RegApp {
        fn open_in_app_settings(&mut self) {
            // Open and reset fields to baseline on each open
            self.settings = self.settings_baseline.clone();
            // Also reset color to its baseline for the GPIO tab
            self.gpio_on_color = self.gpio_on_color_baseline;
            self.gpio_on_color_text = format_color_hex(self.gpio_on_color);
            // Sync console quick-panel state from baseline
            self.console_enabled = self.settings_baseline.use_console;
            self.console_base = parse_u16_any(&self.settings_baseline.console_base_hex);
            let instance = settings_worker::next_dialog_instance();
            self.settings_instance = instance;
            settings_worker::set_active_instance(instance);
            self.settings_open = true;
            self.settings_busy = false;
            self.settings_busy_since = None;
        }
    }

    impl App for RegApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            // Initialize color text once if empty (for first render before opening settings)
            if self.gpio_on_color_text.is_empty() {
                self.gpio_on_color_text = format_color_hex(self.gpio_on_color);
            }
            // Failsafe: if busy state lingers too long, clear it
            if self.settings_busy {
                if let Some(since) = self.settings_busy_since {
                    if since.elapsed().as_secs_f32() > 1.0 {
                        self.settings_busy = false;
                        self.settings_busy_since = None;
                    }
                }
            }
            // Apply any latest GPIO state snapshot (for cases where UiSetGpio is missed)
            if let Ok(mut g) = self.gpio_sync.lock() {
                if let Some((present, bits, out, dir)) = g.take() {
                    self.gpio_present = present;
                    self.gpio_bits = bits;
                    self.gpio_out = out;
                    self.gpio_dir = dir;
                }
            }
            // Apply latest GPIO snapshot broadcast from worker (fallback when commands drop)
            if let Some((present, bits, out, dir)) = em6809::io::peek_gpio_broadcast() {
                self.gpio_present = present;
                self.gpio_bits = bits;
                self.gpio_out = out;
                self.gpio_dir = dir;
            }
            // Drain UI sync commands injected by emulator thread
            let drained_cmds = if let Ok(mut q) = self.cmds.lock() {
                q.drain(..).collect()
            } else {
                Vec::new()
            };
            let mut rest: Vec<Command> = Vec::new();
            for cmd in drained_cmds {
                match cmd {
                    Command::UiOpenSettings => {
                        self.open_in_app_settings();
                    }
                    Command::UiOpenSettingsExternal => {
                        #[cfg(windows)]
                        {
                            self.open_in_app_settings();
                        }
                        #[cfg(not(windows))]
                        {
                            if force_in_app_settings() {
                                self.open_in_app_settings();
                            } else {
                                settings::open_native_settings(ctx);
                            }
                        }
                    }
                    Command::UiSetSettingsBaseline(s, origins, color_opt) => {
                                self.settings_baseline = s.clone();
                                // Update origins map
                                self.settings_origins.clear();
                                for (k, v) in origins.into_iter() {
                                    self.settings_origins.insert(k, v);
                                }
                        // Settings were just applied; clear busy state
                        self.settings_busy = false;
                        self.settings_busy_since = None;
                        // Update quick-panel console state from new baseline
                        self.console_enabled = self.settings_baseline.use_console;
                        self.console_base = parse_u16_any(&self.settings_baseline.console_base_hex);
                        // publish to globals
                        let glob = GLOBAL_SETTINGS.get_or_init(|| {
                            Arc::new(Mutex::new((SettingsState::default(), HashMap::new(), None)))
                        });
                        if let Ok(mut g) = glob.lock() {
                            *g = (
                                self.settings_baseline.clone(),
                                self.settings_origins.clone(),
                                color_opt,
                            );
                        }
                        // Also refresh current fields even if dialog is open so UI matches applied preset
                                // Keep current preset selection if it was set locally and baseline is empty.
                                if !self.settings.mmu_preset_name.is_empty()
                                    && s.mmu_preset_name.is_empty()
                                {
                                    self.settings = self.settings_baseline.clone();
                                    self.settings.mmu_preset_name =
                                        self.settings_baseline.mmu_preset_name.clone();
                                } else {
                                    self.settings = self.settings_baseline.clone();
                                }
                        if let Some(rgb) = color_opt {
                            let col = egui::Color32::from_rgb(
                                ((rgb >> 16) & 0xFF) as u8,
                                ((rgb >> 8) & 0xFF) as u8,
                                (rgb & 0xFF) as u8,
                            );
                            self.gpio_on_color_baseline = col;
                            self.gpio_on_color = col;
                            self.gpio_on_color_text = format_color_hex(col);
                        }
                    }
                    Command::UiAckSettings { instance_id } => {
                        if instance_id == self.settings_instance {
                            self.settings_busy = false;
                            self.settings_busy_since = None;
                            // In case UiSetSettingsBaseline was dropped, refresh from latest baseline.
                            if let Some((s, origins, color_opt)) = get_latest_settings_baseline() {
                                self.settings_baseline = s.clone();
                                self.settings_origins = origins;
                                self.console_enabled = self.settings_baseline.use_console;
                                self.console_base =
                                    parse_u16_any(&self.settings_baseline.console_base_hex);
                                if !self.settings_open {
                                    self.settings = s;
                                }
                                if let Some(rgb) = color_opt {
                                    let col = egui::Color32::from_rgb(
                                        ((rgb >> 16) & 0xFF) as u8,
                                        ((rgb >> 8) & 0xFF) as u8,
                                        (rgb & 0xFF) as u8,
                                    );
                                    self.gpio_on_color_baseline = col;
                                    if !self.settings_open {
                                        self.gpio_on_color = col;
                                        self.gpio_on_color_text = format_color_hex(col);
                                    }
                                }
                            }
                        }
                    }
                    Command::UiSetGpio(present, bits, out, dir) => {
                        self.gpio_present = present;
                        self.gpio_bits = bits;
                        self.gpio_out = out;
                        self.gpio_dir = dir;
                    }
                    Command::UiSetGpioOnColor(rgb) => {
                        self.gpio_on_color = egui::Color32::from_rgb(
                            ((rgb >> 16) & 0xFF) as u8,
                            ((rgb >> 8) & 0xFF) as u8,
                            (rgb & 0xFF) as u8,
                        );
                    }
                    Command::UiSetMmu(sys, task, map) => {
                        self.mmu_present = true;
                        self.mmu_sys_mode = sys;
                        self.mmu_task = task;
                        self.mmu_map_prev = self.mmu_map_cur;
                        self.mmu_map_cur = map;
                    }
                    Command::UiSetTimer(present, pending, running) => {
                        self.timer_running = running;

                        self.timer_present = present;
                        self.timer_pending = pending;
                    }
                    Command::UiSetBlockStatus {
                        present,
                        base,
                        backing_path,
                        backing_online,
                        last_cmd,
                        last_data,
                        status,
                    } => {
                        self.block_present = present;
                        self.block_base = base;
                        self.block_backing_path = backing_path;
                        self.block_backing_online = backing_online;
                        self.block_last_cmd = last_cmd;
                        self.block_last_data = last_data;
                        self.block_status = status;
                        // Keep Settings dialog state aligned with actual device presence
                        self.settings_baseline.use_block = present;
                        if !self.settings_open {
                            self.settings.use_block = present;
                        }
                        if let Some(glob) = GLOBAL_SETTINGS.get() {
                            if let Ok(mut g) = glob.lock() {
                                g.0.use_block = present;
                            }
                        }
                    }
                    Command::ApplyMmuPreset { .. } => { /* worker-handled */ }
                    Command::ApplyMmuMapFileText { .. } => { /* worker-handled */ }
                    Command::UiPromptBinBase(path) => {
                        self.open_pending_path = Some(path);
                        self.open_bin_base_text = "0x0000".to_string();
                        self.open_dialog_active = true;
                    }
                    Command::UiLoadLst(path) => {
                        // Map .lst and switch to LST view immediately
                        if let Ok(f) = std::fs::File::open(&path) {
                            if let Ok(m) = unsafe { Mmap::map(&f) } {
                                self.lst_mmap = Some(Arc::new(m));
                                self.use_hex.store(false, Ordering::SeqCst);
                                // Center listing around current PC after switching views
                                self.scroll_to_pc_req.store(true, Ordering::SeqCst);
                            }
                        }
                    }
                    other => rest.push(other),
                }
            }
            if !rest.is_empty() {
                if let Ok(mut q) = self.cmds.lock() {
                    q.extend(rest);
                }
            }
            // Install JP-capable font once (if available) so Japanese labels render correctly
            if !self.fonts_applied {
                if let Some(bytes) = &self.ja_font {
                    let mut defs = egui::FontDefinitions::default();
                    defs.font_data.insert(
                        "ja".to_owned(),
                        egui::FontData::from_owned((**bytes).clone()),
                    );
                    // Proportional: prefer JP font first so menu/headings render Japanese nicely
                    defs.families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .insert(0, "ja".to_owned());
                    // Monospace: keep existing mono fonts first for alignment; append JP as fallback for CJK glyphs
                    defs.families
                        .entry(egui::FontFamily::Monospace)
                        .or_default()
                        .push("ja".to_owned());
                    ctx.set_fonts(defs);
                }
                self.fonts_applied = true;
            }

            // Keep scheduling native Settings viewport for a few frames after request (macOS multi-viewport create)
            // If we are forcing the in-app dialog (Windows fallback), skip pumping native viewports.
            if !force_in_app_settings() {
                settings::pump_native_settings(ctx);
            }

            // macOS: set app menu title, install About override, and install native system menubar once.
            #[cfg(target_os = "macos")]
            if !self.menu_fixed {
                em6809::platform::macos::set_app_menu_title("em6809");
                em6809::platform::macos::install_about_handler_once();
                {
                    em6809::platform::macos::install_preferences_handler_once(Box::new(
                        move || {
                            let _ = push_global_command(Command::UiOpenSettings);
                        },
                    ));
                }
                let cmds = self.cmds.clone();
                em6809::platform::macos::install_global_menubar_once(Box::new(move || {
                    // Runs on main thread via ObjC target/selector
                    let file = rfd::FileDialog::new()
                        .add_filter("Images", &["s19", "srec", "bin"])
                        .pick_file();
                    if let Some(path) = file {
                        let ext = path
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        if ext == "bin" {
                            if let Ok(mut q) = cmds.lock() {
                                q.push(Command::UiPromptBinBase(path));
                            }
                        } else {
                            if let Ok(mut q) = cmds.lock() {
                                q.push(Command::OpenImage { path, base: None });
                            }
                        }
                    }
                }));
                self.menu_fixed = true;
            }

            // (timer indicator top strip removed; shown below MMU panel)

            // Windows: simple menu bar with About
            #[cfg(windows)]
            {
                eframe::egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
                    eframe::egui::menu::bar(ui, |ui| {
                        ui.menu_button("File", |ui| {
                            if ui.button("Open…").clicked() {
                                let file = rfd::FileDialog::new()
                                    .add_filter("Images", &["s19", "srec", "bin"])
                                    .pick_file();
                                if let Some(path) = file {
                                    let ext = path
                                        .extension()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    if ext == "bin" {
                                        self.open_pending_path = Some(path);
                                        self.open_bin_base_text = "0x0000".to_string();
                                        self.open_dialog_active = true;
                                    } else {
                                        if let Ok(mut q) = self.cmds.lock() {
                                            q.push(Command::OpenImage { path, base: None });
                                        }
                                    }
                                }
                                ui.close_menu();
                            }
                            if ui.button("Settings…").clicked() {
                                self.open_in_app_settings();
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Quit").clicked() {
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                ui.close_menu();
                            }
                        });
                        ui.menu_button("Help", |ui| {
                            if ui.button("About em6809…").clicked() {
                                self.about_open = true;
                                ui.close_menu();
                            }
                        });
                    });
                });
                if self.about_open {
                    em6809::platform::windows::ensure_about_icon(ctx, &mut self.about_icon_tex);
                    em6809::platform::windows::show_about_dialog(
                        ctx,
                        &mut self.about_open,
                        &self.about_icon_tex,
                    );
                }
            }

            // Linux: experimental CSD header or legacy simple menu bar
            #[cfg(target_os = "linux")]
            {
                if self.csd {
                    let mut csd_max = self.csd_maximized;
                    // Defer opening settings until after header rendering to avoid borrow issues
                    let mut open_settings_req = false;
                    em6809::platform::linux::csd::show_header(ctx, &mut csd_max, |ui| {
                        ui.menu_button("File", |ui| {
                            if ui.button("Open…").clicked() {
                                let file = rfd::FileDialog::new()
                                    .add_filter("Images", &["s19", "srec", "bin"])
                                    .pick_file();
                                if let Some(path) = file {
                                    let ext = path
                                        .extension()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    if ext == "bin" {
                                        self.open_pending_path = Some(path);
                                        self.open_bin_base_text = "0x0000".to_string();
                                        self.open_dialog_active = true;
                                    } else {
                                        if let Ok(mut q) = self.cmds.lock() {
                                            q.push(Command::OpenImage { path, base: None });
                                        }
                                    }
                                }
                                ui.close_menu();
                            }
                            if ui.button("Settings…").clicked() {
                                open_settings_req = true;
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Quit").clicked() {
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                ui.close_menu();
                            }
                        });
                        ui.menu_button("Help", |ui| {
                            if ui.button("About em6809…").clicked() {
                                self.about_open = true;
                                ui.close_menu();
                            }
                        });
                    });
                    self.csd_maximized = csd_max;
                    if open_settings_req {
                        self.open_in_app_settings();
                    }
                } else {
                    eframe::egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
                        eframe::egui::menu::bar(ui, |ui| {
                            ui.menu_button("File", |ui| {
                                if ui.button("Open…").clicked() {
                                    let file = rfd::FileDialog::new()
                                        .add_filter("Images", &["s19", "srec", "bin"])
                                        .pick_file();
                                    if let Some(path) = file {
                                        let ext = path
                                            .extension()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("")
                                            .to_lowercase();
                                        if ext == "bin" {
                                            self.open_pending_path = Some(path);
                                            self.open_bin_base_text = "0x0000".to_string();
                                            self.open_dialog_active = true;
                                        } else {
                                            if let Ok(mut q) = self.cmds.lock() {
                                                q.push(Command::OpenImage { path, base: None });
                                            }
                                        }
                                    }
                                    ui.close_menu();
                                }
                                if ui.button("Settings…").clicked() {
                                    self.open_in_app_settings();
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button("Quit").clicked() {
                                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                    ui.close_menu();
                                }
                            });
                            ui.menu_button("Help", |ui| {
                                if ui.button("About em6809…").clicked() {
                                    self.about_open = true;
                                    ui.close_menu();
                                }
                            });
                        });
                    });
                }
                if self.about_open {
                    em6809::platform::linux::about::ensure_about_icon(
                        ctx,
                        &mut self.about_icon_tex,
                    );
                    em6809::platform::linux::about::show_about_dialog(
                        ctx,
                        &mut self.about_open,
                        &self.about_icon_tex,
                    );
                }
            }

            // Windows: no title override to keep window caption unchanged

            // If external quit is requested (e.g., Ctrl+C), close the window
            if self.quit_flag.load(Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            // Modal to request BIN base address after selecting a file
            if self.open_dialog_active {
                egui::Window::new("Open Binary: Base Address")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label("Enter base address for .bin (e.g., 0xE000)");
                        ui.text_edit_singleline(&mut self.open_bin_base_text);
                        ui.horizontal(|ui| {
                            if ui.button("OK").clicked() {
                                if let Some(path) = self.open_pending_path.take() {
                                    let txt = self.open_bin_base_text.trim();
                                    let base = if let Some(h) =
                                        txt.strip_prefix("0x").or(txt.strip_prefix("0X"))
                                    {
                                        u16::from_str_radix(h, 16).ok()
                                    } else {
                                        txt.parse::<u16>().ok()
                                    };
                                    if let Some(b) = base {
                                        if let Ok(mut q) = self.cmds.lock() {
                                            q.push(Command::OpenImage {
                                                path,
                                                base: Some(b),
                                            });
                                        }
                                        self.open_dialog_active = false;
                                    }
                                }
                            }
                            if ui.button("Cancel").clicked() {
                                self.open_dialog_active = false;
                                self.open_pending_path = None;
                            }
                        });
                    });
            }

            // Settings dialog (tabbed)
            if self.settings_open {
                let mut close_req = false;
                egui::Window::new(fl!(LOADER, "settings-title"))
                    .collapsible(false)
                    .resizable(false)
                    .fixed_size(egui::vec2(640.0, 400.0))
                    .min_size(egui::vec2(480.0, 400.0))
                    .max_size(egui::vec2(640.0, 400.0))
                    .id(egui::Id::new(("settings", self.settings_instance)))
                    .open(&mut self.settings_open)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            let tabs = [
                                (0, fl!(LOADER, "tab-general")),
                                (1, fl!(LOADER, "tab-console")),
                                (2, fl!(LOADER, "tab-block")),
                                (3, fl!(LOADER, "tab-gpio")),
                                (4, fl!(LOADER, "tab-timer")),
                                (5, fl!(LOADER, "tab-mmu")),
                                (6, fl!(LOADER, "tab-debug")),
                            ];
                            for (idx, label) in tabs.into_iter() {
                                let selected = self.settings_tab == idx;
                                if ui.selectable_label(selected, label).clicked() {
                                    self.settings_tab = idx;
                                }
                            }
                        });
                        ui.separator();
                        ui.add_space(4.0);
                        let available_height = ui.available_height();
                        let footer_reserved = 70.0;
                        let scroll_height = (available_height - footer_reserved).max(200.0);
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .max_height(scroll_height)
                            .show(ui, |ui| {
                                match self.settings_tab {
                            0 => {
                                // General tab: (moved protect_vectors to MMU tab)
                            }
                            1 => {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.use_console,
                                            fl!(LOADER, "use-console"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "console.enable",
                                        );
                                    });
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "console-base-hex"));
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.console_base_hex,
                                            )
                                            .hint_text("0xFF00")
                                            .desired_width(160.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "console.base",
                                        );
                                    });
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.console_stdin,
                                            fl!(LOADER, "console-read-stdin"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "console.stdin",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.console_echo,
                                            fl!(LOADER, "console-local-echo"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "console.echo",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.console_tee_stderr,
                                            fl!(LOADER, "console-tee-stderr"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "console.tee_stderr",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.console_flush_newline,
                                            fl!(LOADER, "console-flush-newline"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "console.flush_newline",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "console-flush-every"));
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.console_flush_every,
                                            )
                                            .hint_text("1")
                                            .desired_width(120.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "console.flush_every",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "console-output-file"));
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.console_out_path,
                                            )
                                            .hint_text("/tmp/console.txt")
                                            .desired_width(260.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "console.out_path",
                                        );
                                        if ui.button(fl!(LOADER, "browse")).clicked() {
                                            if let Some(path) = rfd::FileDialog::new()
                                                .set_directory(".")
                                                .pick_file()
                                            {
                                                self.settings.console_out_path =
                                                    path.to_string_lossy().to_string();
                                            }
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "console-rx-watermark"));
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.console_rx_wm,
                                            )
                                            .hint_text("1")
                                            .desired_width(120.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "console.rx_watermark",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "irq-hold"));
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.console_irq_hold,
                                            )
                                            .hint_text("0")
                                            .desired_width(120.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "console.irq_hold",
                                        );
                                    });
                                });
                            }
                            2 => {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.use_block,
                                            fl!(LOADER, "use-block"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "block.enable",
                                        );
                                    });
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "block-base-hex"));
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.block_base_hex,
                                            )
                                            .hint_text("0xFF20")
                                            .desired_width(100.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "block.base",
                                        );
                                    });
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "block-file-path"));
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.block_file_path,
                                            )
                                            .hint_text("/tmp/disk.img")
                                            .desired_width(260.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "block.file_path",
                                        );
                                        if ui.button(fl!(LOADER, "browse")).clicked() {
                                            if let Some(path) = rfd::FileDialog::new()
                                                .add_filter("Disk image", &["img", "bin", "raw"])
                                                .pick_file()
                                            {
                                                self.settings.block_file_path =
                                                    path.to_string_lossy().to_string();
                                            }
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.block_irq_enable,
                                            fl!(LOADER, "irq"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "block.irq_enable",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.block_firq,
                                            fl!(LOADER, "firq"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "block.firq",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "irq-hold"));
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.block_irq_hold,
                                            )
                                            .hint_text("0")
                                            .desired_width(80.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "block.irq_hold",
                                        );
                                    });
                                });
                            }
                            3 => {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.use_gpio,
                                            fl!(LOADER, "use-gpio"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "gpio.enable",
                                        );
                                    });
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "gpio-base-hex"));
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.gpio_base_hex,
                                            )
                                            .hint_text("0xFF40")
                                            .desired_width(100.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "gpio.base",
                                        );
                                    });
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "gpio-bits"));
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.settings.gpio_bits)
                                                .hint_text("8")
                                                .desired_width(60.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "gpio.bits",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "on-color"));
                                        // Direct hex input (e.g., 0xFF0000 or #FF0000)
                                        let te = egui::TextEdit::singleline(
                                            &mut self.gpio_on_color_text,
                                        )
                                        .desired_width(100.0)
                                        .hint_text("0xRRGGBB");
                                        let resp = ui.add(te);
                                        if resp.changed() {
                                            if let Some(col) =
                                                parse_color_hex(&self.gpio_on_color_text)
                                            {
                                                self.gpio_on_color = col;
                                            }
                                        }
                                        // Color picker stays in sync with text and vice versa
                                        let mut col = self.gpio_on_color;
                                        if egui::widgets::color_picker::color_edit_button_srgba(
                                            ui,
                                            &mut col,
                                            egui::widgets::color_picker::Alpha::Opaque,
                                        )
                                        .changed()
                                        {
                                            self.gpio_on_color = col;
                                            self.gpio_on_color_text = format_color_hex(col);
                                        }
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "gpio.on_color",
                                        );
                                    });
                                });
                            }
                            4 => {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.use_timer,
                                            fl!(LOADER, "use-timer"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "timer.enable",
                                        );
                                    });
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "timer-base-hex"));
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.timer_base_hex,
                                            )
                                            .hint_text("0xFF10")
                                            .desired_width(100.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "timer.base",
                                        );
                                    });
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "timer-reload-hex"));
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.timer_reload_hex,
                                            )
                                            .hint_text("0x03E8")
                                            .desired_width(100.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "timer.reload",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(fl!(LOADER, "timer-rate-hz"));
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.timer_rate_hz,
                                            )
                                            .hint_text("6000")
                                            .desired_width(100.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "timer.rate",
                                        );
                                    ui.label(fl!(LOADER, "timer-ips"));
                                    ui.add(
                                        egui::TextEdit::singleline(
                                            &mut self.settings.timer_ips,
                                        )
                                            .hint_text("1000000")
                                            .desired_width(120.0),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "timer.ips",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.timer_irq_enable,
                                            fl!(LOADER, "irq"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "timer.irq_enable",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.timer_firq,
                                            fl!(LOADER, "firq"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "timer.firq",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.settings.timer_start,
                                            fl!(LOADER, "start"),
                                        );
                                        super::gui::draw_origin_badge(
                                            ui,
                                            &self.settings_origins,
                                            "timer.start",
                                        );
                                    });
                                });
                            }
                            5 => {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut self.settings.use_mmu, fl!(LOADER, "use-mmu"));
                                    super::gui::draw_origin_badge(
                                        ui,
                                        &self.settings_origins,
                                        "mmu.enable",
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label(fl!(LOADER, "mmu-regs-base-hex"));
                                    ui.add(
                                        egui::TextEdit::singleline(
                                            &mut self.settings.mmu_regs_base_hex,
                                        )
                                        .hint_text("0xFFA0")
                                        .desired_width(100.0),
                                    );
                                    super::gui::draw_origin_badge(
                                        ui,
                                        &self.settings_origins,
                                        "mmu.regs_base",
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label(fl!(LOADER, "mmu-phys-bytes"));
                                    ui.add(
                                        egui::TextEdit::singleline(
                                            &mut self.settings.mmu_phys_bytes,
                                        )
                                        .hint_text("1048576")
                                        .desired_width(120.0),
                                    );
                                    super::gui::draw_origin_badge(
                                        ui,
                                        &self.settings_origins,
                                        "mmu.phys_bytes",
                                    );
                                });
                                ui.weak(fl!(LOADER, "mmu-restart-note"));
                                ui.separator();
                                // Runtime controls (apply without restart when MMU is active)
                                ui.horizontal(|ui| {
                                    ui.checkbox(
                                        &mut self.settings.mmu_mode_sys,
                                        fl!(LOADER, "system-mode"),
                                    );
                                    super::gui::draw_origin_badge(
                                        ui,
                                        &self.settings_origins,
                                        "mmu.mode",
                                    );
                                    ui.label(fl!(LOADER, "task"));
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.settings.mmu_task)
                                            .desired_width(40.0)
                                            .hint_text("0"),
                                    );
                                    super::gui::draw_origin_badge(
                                        ui,
                                        &self.settings_origins,
                                        "mmu.task",
                                    );
                                    ui.checkbox(
                                        &mut self.settings.mmu_log_maps,
                                        fl!(LOADER, "log-maps"),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.checkbox(
                                        &mut self.settings.protect_vectors,
                                        fl!(LOADER, "protect-vectors"),
                                    );
                                    super::gui::draw_origin_badge(
                                        ui,
                                        &self.settings_origins,
                                        "mmu.protect_vectors",
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label(fl!(LOADER, "prot-hex"));
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.settings.mmu_prot_hex)
                                            .desired_width(70.0)
                                            .hint_text("0x03"),
                                    );
                                    super::gui::draw_origin_badge(
                                        ui,
                                        &self.settings_origins,
                                        "mmu.prot",
                                    );
                                    if ui.button(fl!(LOADER, "idmap-current")).clicked() {
                                        if let Ok(mut q) = self.cmds.lock() {
                                            q.push(Command::MmuIdentityMap);
                                        }
                                    }
                                });
                                ui.add_space(4.0);
                                // Page->Frame map editor (hex)
                                ui.label(fl!(LOADER, "mmu-map-heading"));
                                let map_width = 42.0;
                                let map_grid_width = 200.0;
                                ui.scope(|ui| {
                                    ui.set_width(map_grid_width);
                                    egui::Grid::new("mmu_map_grid")
                                        .num_columns(4)
                                        .spacing(egui::vec2(12.0, 4.0))
                                        .show(ui, |ui| {
                                            for row in 0..4 {
                                                for col in 0..4 {
                                                    let p = row * 4 + col;
                                                    let cell_resp = ui.horizontal(|ui| {
                                                        ui.monospace(format!("P{:X}", p));
                                                        let h = ui.text_style_height(&egui::TextStyle::Monospace);
                                                        let resp = ui.add_sized(
                                                            (map_width, h + 6.0),
                                                            egui::TextEdit::singleline(
                                                                &mut self.settings.mmu_map_frames[p],
                                                            )
                                                            .hint_text("0x00")
                                                            .char_limit(4),
                                                        );
                                                        resp
                                                    });
                                                    // Clamp to 8-bit hex if user entered wider
                                                    if cell_resp.response.changed() {
                                                        let txt = self.settings.mmu_map_frames[p].trim();
                                                        if let Some(v) = super::gui::parse_u8_hex(txt) {
                                                            self.settings.mmu_map_frames[p] =
                                                                format!("{:02X}", v);
                                                        } else if !txt.is_empty() {
                                                            // keep last valid or reset
                                                            self.settings.mmu_map_frames[p].clear();
                                                        }
                                                    }
                                                }
                                                ui.end_row();
                                            }
                                        });
                                });
                                ui.add_space(6.0);
                                // Attributes per page (hex)
                                ui.label(fl!(LOADER, "attributes-note"));
                                let attr_width = 42.0;
                                let attr_grid_width = 200.0;
                                ui.scope(|ui| {
                                    ui.set_width(attr_grid_width);
                                    egui::Grid::new("mmu_attr_grid")
                                        .num_columns(4)
                                        .spacing(egui::vec2(12.0, 4.0))
                                        .show(ui, |ui| {
                                            for row in 0..4 {
                                                for col in 0..4 {
                                                    let p = row * 4 + col;
                                                    ui.horizontal(|ui| {
                                                        ui.monospace(format!("P{:X}", p));
                                                        let h = ui.text_style_height(&egui::TextStyle::Monospace);
                                                        ui.add_sized(
                                                            (attr_width, h + 6.0),
                                                            egui::TextEdit::singleline(
                                                                &mut self.settings.mmu_attr_bytes[p],
                                                            )
                                                            .hint_text("0x00"),
                                                        );
                                                    });
                                                }
                                                ui.end_row();
                                            }
                                        });
                                });
                                ui.add_space(6.0);
                                // Preset selection and apply
                                ui.horizontal(|ui| {
                                    ui.label(fl!(LOADER, "preset"));
                                    let presets = em6809::config::list_presets();
                                    let mut current = if !self.settings.mmu_preset_name.is_empty() {
                                        self.settings.mmu_preset_name.clone()
                                    } else if !self.settings_baseline.mmu_preset_name.is_empty() {
                                        self.settings_baseline.mmu_preset_name.clone()
                                    } else {
                                        "__custom__".to_string()
                                    };
                                    let display_text = if current == "__custom__" {
                                        "(Custom)".to_string()
                                    } else {
                                        current.clone()
                                    };
                                    egui::ComboBox::from_id_source("mmu_preset_combo")
                                        .width(180.0)
                                        .selected_text(display_text)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(
                                                &mut current,
                                                "__custom__".to_string(),
                                                "(Custom)",
                                            );
                                            for &name in presets.iter() {
                                                ui.selectable_value(
                                                    &mut current,
                                                    name.to_string(),
                                                    name,
                                                );
                                            }
                                        });
                                    if current == "__custom__" {
                                        self.settings.mmu_preset_name.clear();
                                    } else if current != self.settings.mmu_preset_name {
                                        self.settings.mmu_preset_name = current;
                                    }
                                    if ui.button(fl!(LOADER, "apply-preset")).clicked() {
                                        if let Ok(mut q) = self.cmds.lock() {
                                            q.push(Command::ApplyMmuPreset {
                                                name: self.settings.mmu_preset_name.clone(),
                                            });
                                        }
                                    }
                                    if ui.button(fl!(LOADER, "load-map-file")).clicked() {
                                        if let Some(path) = rfd::FileDialog::new()
                                            .add_filter("MMU map", &["txt", "cfg", "mmu"])
                                            .pick_file()
                                        {
                                            if let Ok(text) = std::fs::read_to_string(&path) {
                                                if let Ok(mut q) = self.cmds.lock() {
                                                    q.push(Command::ApplyMmuMapFileText { text });
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                            6 => {
                                let mut changed = false;
                                ui.vertical(|ui| {
                                    changed |= ui
                                        .checkbox(
                                            &mut self.settings.debug_settings_log,
                                            fl!(LOADER, "debug-settings-log"),
                                        )
                                        .changed();
                                    changed |= ui
                                        .checkbox(
                                            &mut self.settings.debug_show_origins,
                                            fl!(LOADER, "debug-show-origins"),
                                        )
                                        .changed();
                                    ui.separator();
                                    changed |= ui
                                        .checkbox(
                                            &mut self.settings.debug_console_log,
                                            fl!(LOADER, "debug-console-log"),
                                        )
                                        .changed();
                                    changed |= ui
                                        .checkbox(
                                            &mut self.settings.debug_block_log,
                                            fl!(LOADER, "debug-block-log"),
                                        )
                                        .changed();
                                    changed |= ui
                                        .checkbox(
                                            &mut self.settings.debug_gpio_log,
                                            fl!(LOADER, "debug-gpio-log"),
                                        )
                                        .changed();
                                    changed |= ui
                                        .checkbox(
                                            &mut self.settings.debug_mmu_log,
                                            fl!(LOADER, "debug-mmu-log"),
                                        )
                                        .changed();
                                });
                                if changed {
                                    let payload = SettingsPayload {
                                        settings: self.settings.clone(),
                                        gpio_on_color: ((self.gpio_on_color.r() as u32) << 16)
                                            | ((self.gpio_on_color.g() as u32) << 8)
                                            | (self.gpio_on_color.b() as u32),
                                        instance_id: self.settings_instance,
                                    };
                                    settings_worker::push_worker_command(
                                        WorkerCommand::Apply(payload),
                                    );
                                    self.settings_busy = true;
                                    self.settings_busy_since = Some(std::time::Instant::now());
                                }
                            }
                            _ => {}
                        }
                            });
                        ui.separator();
                        ui.add_space(4.0);
                        // Wrap the right-aligned row in a horizontal layout so it takes only row height
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Right-aligned buttons: Apply | Revert | Cancel
                                    if ui.button(fl!(LOADER, "cancel")).clicked() {
                                        close_req = true;
                                    }
                                    if ui.button(fl!(LOADER, "revert")).clicked() {
                                        // Restore fields to baseline and close dialog
                                        self.settings = self.settings_baseline.clone();
                                        self.gpio_on_color = self.gpio_on_color_baseline;
                                        self.gpio_on_color_text =
                                            format_color_hex(self.gpio_on_color);
                                    }
                                    let apply_clicked = ui
                                        .add_enabled(
                                            !self.settings_busy,
                                            egui::Button::new(fl!(LOADER, "apply")),
                                        )
                                        .clicked();
                                    if apply_clicked {
                                        let payload = SettingsPayload {
                                            settings: self.settings.clone(),
                                            gpio_on_color: ((self.gpio_on_color.r() as u32) << 16)
                                                | ((self.gpio_on_color.g() as u32) << 8)
                                                | (self.gpio_on_color.b() as u32),
                                            instance_id: self.settings_instance,
                                        };
                                        settings_worker::push_worker_command(
                                            WorkerCommand::SavePrefs(payload),
                                        );
                                        // Also enqueue to the viewer->worker command queue so paused GUI runs apply.
                                        if let Ok(mut q) = self.cmds.lock() {
                                            q.push(Command::SavePrefs {
                                                settings: self.settings.clone(),
                                                gpio_on_color: ((self.gpio_on_color.r() as u32)
                                                    << 16)
                                                    | ((self.gpio_on_color.g() as u32) << 8)
                                                    | (self.gpio_on_color.b() as u32),
                                                instance_id: self.settings_instance,
                                            });
                                        }
                                        self.settings_busy = true;
                                        self.settings_busy_since = Some(std::time::Instant::now());
                                    }
                                    ui.label(fl!(LOADER, "note-apply-now"));
                                    if self.settings_busy {
                                        ui.add(egui::Spinner::new().size(16.0));
                                    }
                                    ui.add_space(6.0);
                                },
                            );
                        });
                    });
                // Honor explicit close request from Cancel/Revert buttons
                if close_req {
                    self.settings_open = false;
                }
            }

            // Editing is user-controlled via Edit/Apply toggle (not tied to pause)
            let editing = self.edit_mode;
            // Initialize edit buffers from current snapshot the first time we enter editing mode
            if editing {
                if !self.edit_initialized {
                    if let Ok(s) = self.snap.lock() {
                        self.edit_a = format!("0x{:02X}", s.a);
                        self.edit_b = format!("0x{:02X}", s.b);
                        self.edit_d = format!("0x{:04X}", s.d);
                        self.edit_x = format!("0x{:04X}", s.x);
                        self.edit_y = format!("0x{:04X}", s.y);
                        self.edit_u = format!("0x{:04X}", s.u);
                        self.edit_s = format!("0x{:04X}", s.s);
                        self.edit_dp = format!("0x{:02X}", s.dp);
                        self.edit_pc = format!("0x{:04X}", s.pc);
                        self.cc_e = (s.cc & 0x80) != 0;
                        self.cc_f = (s.cc & 0x40) != 0;
                        self.cc_h = (s.cc & 0x20) != 0;
                        self.cc_i = (s.cc & 0x10) != 0;
                        self.cc_n = (s.cc & 0x08) != 0;
                        self.cc_z = (s.cc & 0x04) != 0;
                        self.cc_v = (s.cc & 0x02) != 0;
                        self.cc_c = (s.cc & 0x01) != 0;
                    }
                    self.input_error = None;
                    self.edit_initialized = true;
                }
            } else if self.edit_initialized {
                // When leaving editing mode, reset so we sync from latest snapshot next pause
                self.edit_initialized = false;
            }

            // Memory pane (placed to the left of the listing pane by ordering two right panels)
            egui::SidePanel::right("mem_panel")
                .resizable(false)
                .min_width(440.0)
                .default_width(440.0)
                .show(ctx, |ui| {
                    let mut follow_pc = self.mem_follow_pc.load(Ordering::SeqCst);
                    let _win_len = self.mem_window_len.load(Ordering::SeqCst);
                    let mut start = self.mem_window_start.load(Ordering::SeqCst) as u16;
                    ui.horizontal(|ui| {
                        ui.heading(fl!(LOADER, "memory"));
                        ui.separator();
                        if ui
                            .checkbox(&mut follow_pc, fl!(LOADER, "follow-pc"))
                            .clicked()
                        {
                            self.mem_follow_pc.store(follow_pc, Ordering::SeqCst);
                        }
                        let mut utf8 = self.mem_utf8.load(Ordering::SeqCst);
                        if ui.checkbox(&mut utf8, "UTF-8").clicked() {
                            self.mem_utf8.store(utf8, Ordering::SeqCst);
                        }
                        let mut use_drag = self.mem_use_drag;
                        if ui.checkbox(&mut use_drag, fl!(LOADER, "numeric")).clicked() {
                            self.mem_use_drag = use_drag;
                        }
                        ui.separator();
                        // Edit mode controls: in normal mode show only "Edit"; in edit mode show "Apply" and "Cancel".
                        let pending_count = self.mem_pending.len();
                        if !self.mem_edit_mode {
                            if ui.button(fl!(LOADER, "edit")).clicked() {
                                self.mem_edit_mode = true;
                                self.mem_selected = None;
                                self.mem_edit_buf.clear();
                            }
                        } else {
                            let apply_enabled = pending_count > 0;
                            if ui
                                .add_enabled(apply_enabled, egui::Button::new(fl!(LOADER, "apply")))
                                .clicked()
                            {
                                let mut items: Vec<(u16, u8)> =
                                    self.mem_pending.iter().map(|(a, v)| (*a, *v)).collect();
                                items.sort_by_key(|(a, _)| *a);
                                if let Ok(mut q) = self.cmds.lock() {
                                    q.push(Command::MemWrite(items));
                                }
                                self.mem_pending.clear();
                                self.mem_selected = None;
                                self.mem_edit_buf.clear();
                                // Exit edit mode after apply as requested
                                self.mem_edit_mode = false;
                                // Reset caret boundary tracking
                                self.mem_last_sel_addr = None;
                                self.mem_last_at_start = false;
                                self.mem_last_at_end = false;
                            }
                            if ui.button(fl!(LOADER, "cancel")).clicked() {
                                // Cancel discards pending edits and exits edit mode
                                self.mem_pending.clear();
                                self.mem_selected = None;
                                self.mem_edit_buf.clear();
                                self.mem_edit_mode = false;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(fl!(LOADER, "start"));
                        if self.mem_use_drag {
                            let mut sv: u16 = self.mem_window_start.load(Ordering::SeqCst) as u16;
                            let resp = ui.add_enabled(
                                !follow_pc,
                                egui::DragValue::new(&mut sv)
                                    .clamp_range(0u16..=0xFFFF)
                                    .speed(16.0),
                            );
                            ui.monospace(format!(" (0x{sv:04X})"));
                            if resp.changed() && !follow_pc {
                                start = sv;
                                self.mem_window_start.store(sv as usize, Ordering::SeqCst);
                                self.mem_jump_text = format!("0x{sv:04X}");
                            }
                        } else {
                            let te = egui::TextEdit::singleline(&mut self.mem_jump_text)
                                .desired_width(80.0)
                                .hint_text("0x0000");
                            // Disable editing of Start when following PC
                            let start_resp = ui.add_enabled(!follow_pc, te);
                            // Commit Start: Enter while focused or on focus loss
                            if !follow_pc {
                                if start_resp.has_focus() {
                                    if ui.input_mut(|i| {
                                        i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                                    }) {
                                        if let Some(val) =
                                            parse_num_u16_any_gui(&self.mem_jump_text)
                                        {
                                            start = val;
                                            self.mem_window_start
                                                .store(val as usize, Ordering::SeqCst);
                                            self.mem_jump_text = format!("0x{val:04X}");
                                        }
                                    }
                                } else if start_resp.lost_focus() {
                                    if let Some(val) = parse_num_u16_any_gui(&self.mem_jump_text) {
                                        start = val;
                                        self.mem_window_start.store(val as usize, Ordering::SeqCst);
                                        self.mem_jump_text = format!("0x{val:04X}");
                                    }
                                }
                            }
                        }
                        ui.label(fl!(LOADER, "len"));
                        if self.mem_use_drag {
                            let mut lv: usize = self.mem_window_len.load(Ordering::SeqCst);
                            let old = lv;
                            let resp = ui.add(
                                egui::DragValue::new(&mut lv)
                                    .clamp_range(16..=4096)
                                    .speed(1.0),
                            );
                            ui.label(fl!(LOADER, "bytes"));
                            if resp.changed() && lv != old {
                                let v = lv.clamp(16, 4096);
                                self.mem_window_len.store(v, Ordering::SeqCst);
                                self.mem_len_text = format!("{v}");
                            }
                        } else {
                            let len_te = egui::TextEdit::singleline(&mut self.mem_len_text)
                                .desired_width(64.0)
                                .hint_text("256");
                            let len_resp = ui.add(len_te);
                            // Commit Len: Enter while focused or on focus loss
                            if len_resp.has_focus() {
                                if ui.input_mut(|i| {
                                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                                }) {
                                    if let Some(v) = parse_num_usize_any_gui(&self.mem_len_text) {
                                        let v = v.clamp(16, 4096);
                                        self.mem_window_len.store(v, Ordering::SeqCst);
                                        self.mem_len_text = format!("{v}");
                                    }
                                }
                            } else if len_resp.lost_focus() {
                                if let Some(v) = parse_num_usize_any_gui(&self.mem_len_text) {
                                    let v = v.clamp(16, 4096);
                                    self.mem_window_len.store(v, Ordering::SeqCst);
                                    self.mem_len_text = format!("{v}");
                                }
                            }
                        }
                        // Jump button removed; Enter/focus-out on Start commits address
                        if ui.button(fl!(LOADER, "copy")).clicked() {
                            let dump = if let Ok(g) = self.mem_buf.lock() {
                                format_memory_dump((g.0, g.1.as_slice()))
                            } else {
                                String::new()
                            };
                            ui.output_mut(|o| o.copied_text = dump);
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let (s, bytes) = if let Ok(g) = self.mem_buf.lock() {
                                (g.0, g.1.clone())
                            } else {
                                (0u16, Vec::new())
                            };
                            let hl = self.mem_hl.lock().ok().and_then(|g| *g);
                            let (hl_start, hl_end) = if let Some((hs, hl)) = hl {
                                let end = hs.wrapping_add(hl as u16);
                                (Some(hs), Some(end))
                            } else {
                                (None, None)
                            };
                            let hl_color = egui::Color32::RED;
                            let _utf8_mode = self.mem_utf8.load(Ordering::SeqCst);
                            let mut off: usize = 0;
                            while off < bytes.len() {
                                let chunk = &bytes[off..bytes.len().min(off + 16)];
                                let addr = s.wrapping_add(off as u16);
                                ui.scope(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    ui.horizontal(|ui| {
                                        ui.monospace(format!("{addr:04X}: "));
                                        // hex bytes
                                        for (i, b) in chunk.iter().enumerate() {
                                            let a = addr.wrapping_add(i as u16);
                                            let in_hl =
                                                if let (Some(hs), Some(he)) = (hl_start, hl_end) {
                                                    if hs <= he {
                                                        a >= hs && a < he
                                                    } else {
                                                        a >= hs || a < he
                                                    }
                                                } else {
                                                    false
                                                };
                                            let shown = if let Some(v) = self.mem_pending.get(&a) {
                                                *v
                                            } else {
                                                *b
                                            };
                                            // Edit mode: selected cell becomes TextEdit, others clickable labels
                                            if self.mem_edit_mode && self.mem_selected == Some(a) {
                                                // ensure buffer contains current value
                                                if self.mem_edit_buf.is_empty() {
                                                    self.mem_edit_buf = format!("{shown:02X}");
                                                }
                                                let cell_id = egui::Id::new(("mem_hex_cell", a));
                                                // Revert overwrite mode: no need to snapshot buffer/caret pre-state
                                                let te = egui::TextEdit::singleline(
                                                    &mut self.mem_edit_buf,
                                                )
                                                .desired_width(24.0)
                                                .id_source(cell_id)
                                                .cursor_at_end(true);
                                                let resp = ui.add(te);
                                                if self.mem_focus_next {
                                                    resp.request_focus();
                                                    self.mem_focus_next = false;
                                                }
                                                // On focus gain (e.g., via Tab/Enter), select all text so即時上書き可能
                                                if resp.gained_focus() {
                                                    if let Some(mut state) =
                                                        egui::text_edit::TextEditState::load(
                                                            ui.ctx(),
                                                            resp.id,
                                                        )
                                                    {
                                                        let len = self.mem_edit_buf.len();
                                                        let range = egui::text::CCursorRange::two(
                                                            egui::text::CCursor::new(0),
                                                            egui::text::CCursor::new(len),
                                                        );
                                                        state.cursor.set_char_range(Some(range));
                                                        state.store(ui.ctx(), resp.id);
                                                    }
                                                }
                                                // Key handling: consume Tab/Enter to avoid default focus changes; arrows remain non-consuming
                                                let (
                                                    enter,
                                                    tab_any,
                                                    shift_held,
                                                    arrow_up,
                                                    arrow_down,
                                                    left_pressed,
                                                    right_pressed,
                                                    home_key,
                                                    end_key,
                                                ) = ui.input_mut(|i| {
                                                    let shift_now = i.modifiers.shift;
                                                    let enter = i.consume_key(
                                                        egui::Modifiers::NONE,
                                                        egui::Key::Enter,
                                                    );
                                                    // Consume both plain Tab and Shift+Tab
                                                    let tab_plain_c = i.consume_key(
                                                        egui::Modifiers::NONE,
                                                        egui::Key::Tab,
                                                    );
                                                    let tab_shift_c = i.consume_key(
                                                        egui::Modifiers::SHIFT,
                                                        egui::Key::Tab,
                                                    );
                                                    let tab_any = tab_plain_c || tab_shift_c;
                                                    let arrow_up = i.consume_key(
                                                        egui::Modifiers::NONE,
                                                        egui::Key::ArrowUp,
                                                    );
                                                    let arrow_down = i.consume_key(
                                                        egui::Modifiers::NONE,
                                                        egui::Key::ArrowDown,
                                                    );
                                                    // Consume left/right to detect a single navigation intent this frame
                                                    let left_pressed = i.consume_key(
                                                        egui::Modifiers::NONE,
                                                        egui::Key::ArrowLeft,
                                                    );
                                                    let right_pressed = i.consume_key(
                                                        egui::Modifiers::NONE,
                                                        egui::Key::ArrowRight,
                                                    );
                                                    let home_key = i.key_pressed(egui::Key::Home);
                                                    let end_key = i.key_pressed(egui::Key::End);
                                                    (
                                                        enter,
                                                        tab_any,
                                                        shift_now,
                                                        arrow_up,
                                                        arrow_down,
                                                        left_pressed,
                                                        right_pressed,
                                                        home_key,
                                                        end_key,
                                                    )
                                                });
                                                let shift_tab = tab_any && shift_held;
                                                let tab_plain = tab_any && !shift_held;
                                                // After TextEdit processed keys, re-check caret; only jump if caret stayed at boundary
                                                let (at_start_after, at_end_after) = {
                                                    if let Some(state) =
                                                        egui::text_edit::TextEditState::load(
                                                            ui.ctx(),
                                                            resp.id,
                                                        )
                                                    {
                                                        if let Some(r) = state.cursor.char_range() {
                                                            let len = self.mem_edit_buf.len();
                                                            let collapsed =
                                                                r.primary == r.secondary;
                                                            let idx = r.primary.index;
                                                            (
                                                                collapsed && idx == 0,
                                                                collapsed && idx == len,
                                                            )
                                                        } else {
                                                            (false, false)
                                                        }
                                                    } else {
                                                        (false, false)
                                                    }
                                                };
                                                // Reverted overwrite mode: rely on TextEdit's default insertion behavior
                                                // Cross-frame boundary tracking: require端滞在の明確な2度押し
                                                let now_at_start = at_start_after;
                                                let now_at_end = at_end_after;
                                                let same_cell = self.mem_last_sel_addr == Some(a);
                                                let move_prev = same_cell
                                                    && left_pressed
                                                    && self.mem_last_at_start
                                                    && now_at_start;
                                                let move_next = same_cell
                                                    && right_pressed
                                                    && self.mem_last_at_end
                                                    && now_at_end;
                                                let commit = resp.lost_focus()
                                                    || enter
                                                    || tab_any
                                                    || arrow_up
                                                    || arrow_down
                                                    || move_prev
                                                    || move_next
                                                    || home_key
                                                    || end_key;
                                                if commit {
                                                    if let Ok(v) = u8::from_str_radix(
                                                        self.mem_edit_buf.trim(),
                                                        16,
                                                    ) {
                                                        // Only keep pending entry if it differs from the original byte
                                                        if v != *b {
                                                            self.mem_pending.insert(a, v);
                                                        } else {
                                                            self.mem_pending.remove(&a);
                                                        }
                                                        // Decide next selection
                                                        let line_base = a & 0xFFF0;
                                                        let next_sel = if home_key {
                                                            line_base
                                                        } else if end_key {
                                                            line_base.wrapping_add(15)
                                                        } else if shift_tab || move_prev {
                                                            a.wrapping_sub(1)
                                                        } else if arrow_up {
                                                            a.wrapping_sub(16)
                                                        } else if arrow_down {
                                                            a.wrapping_add(16)
                                                        } else if enter || tab_plain || move_next {
                                                            a.wrapping_add(1)
                                                        } else {
                                                            a
                                                        };
                                                        if next_sel != a {
                                                            self.mem_selected = Some(next_sel);
                                                            self.mem_focus_next = true;
                                                            // Reset boundary tracking so新セルでも2度押しが必要
                                                            self.mem_last_sel_addr = None;
                                                            self.mem_last_at_start = false;
                                                            self.mem_last_at_end = false;
                                                        }
                                                        self.mem_edit_buf.clear();
                                                    } else {
                                                        // keep selection, but reset buffer to current value
                                                        self.mem_edit_buf = format!("{shown:02X}");
                                                    }
                                                }
                                                // Update boundary tracking for this cell each frame
                                                self.mem_last_sel_addr = Some(a);
                                                self.mem_last_at_start = now_at_start;
                                                self.mem_last_at_end = now_at_end;
                                            } else {
                                                let mut rt =
                                                    egui::RichText::new(format!("{shown:02X}"))
                                                        .monospace();
                                                if in_hl {
                                                    rt = rt.background_color(hl_color);
                                                }
                                                if let Some(&nv) = self.mem_pending.get(&a) {
                                                    if nv != *b {
                                                        rt = rt.background_color(
                                                            egui::Color32::from_rgb(255, 255, 128),
                                                        );
                                                    }
                                                }
                                                let label = egui::Label::new(rt).sense(
                                                    if self.mem_edit_mode {
                                                        egui::Sense::click()
                                                    } else {
                                                        egui::Sense::hover()
                                                    },
                                                );
                                                let resp = ui.add(label);
                                                if self.mem_edit_mode && resp.clicked() {
                                                    self.mem_selected = Some(a);
                                                    self.mem_edit_buf = String::new();
                                                    self.mem_focus_next = true;
                                                    // Reset boundary tracking on new selection
                                                    self.mem_last_sel_addr = None;
                                                    self.mem_last_at_start = false;
                                                    self.mem_last_at_end = false;
                                                }
                                            }
                                            if i != chunk.len() - 1 {
                                                ui.monospace(" ");
                                            }
                                        }
                                        // pad if fewer than 16 bytes
                                        for _ in chunk.len()..16 {
                                            ui.monospace("   ");
                                        }
                                        ui.monospace("| ");
                                        // ASCII per-cell render so only changed bytes get highlight
                                        for (i, ob) in chunk.iter().enumerate() {
                                            let a = addr.wrapping_add(i as u16);
                                            let shown_b = *self.mem_pending.get(&a).unwrap_or(ob);
                                            let ch = if (0x20..=0x7E).contains(&shown_b) {
                                                shown_b as char
                                            } else {
                                                '.'
                                            };
                                            let mut rt =
                                                egui::RichText::new(format!("{ch}")).monospace();
                                            if let Some(&nv) = self.mem_pending.get(&a) {
                                                if nv != *ob {
                                                    rt = rt.background_color(
                                                        egui::Color32::from_rgb(255, 255, 128),
                                                    );
                                                }
                                            }
                                            ui.add(egui::Label::new(rt));
                                        }
                                    });
                                });
                                off += 16;
                            }
                        });
                });

            // Right side: listing in its own scroll area and panel (so only listing scrolls)
            if self.lst_mmap.is_some() || self.lst_text.is_some() {
                egui::SidePanel::right("lst_panel")
                    .resizable(false)
                    .min_width(400.0)
                    .default_width(600.0)
                    .show(ctx, |ui| {
                        egui::ScrollArea::horizontal()
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.set_min_width(400.0);
                                ui.style_mut().wrap = Some(false);

                                let have_lst = self.lst_mmap.is_some();
                                let have_hex = self.lst_text.is_some();
                                let mut use_hex_view = self.use_hex.load(Ordering::SeqCst);
                                let mut auto_hex = self.auto_hex_on_edit.load(Ordering::SeqCst);
                                let mut follow_pc = self.follow_pc_flag.load(Ordering::SeqCst);
                                let mut policy = self.span_policy.load(Ordering::SeqCst);
                                let mut win_len = self.window_len.load(Ordering::SeqCst);
                                ui.horizontal(|ui| {
                                    ui.heading(fl!(LOADER, "listing"));
                                    let mode = if use_hex_view { "HEX" } else { "LST" };
                                    let pill = egui::RichText::new(mode)
                                        .monospace()
                                        .strong()
                                        .background_color(egui::Color32::from_rgb(220, 220, 220))
                                        .color(egui::Color32::BLACK);
                                    ui.add(egui::Label::new(pill));
                                    ui.separator();
                                    if have_lst && have_hex
                                        && ui.checkbox(&mut use_hex_view, fl!(LOADER, "hex-view")).clicked()
                                    {
                                        self.use_hex.store(use_hex_view, Ordering::SeqCst);
                                        if follow_pc {
                                            self.scroll_to_pc_req.store(true, Ordering::SeqCst);
                                        }
                                        if use_hex_view {
                                            if let Ok(mut q) = self.cmds.lock() {
                                                q.push(Command::RefreshHexListing);
                                            }
                                        }
                                    }
                                    if ui.checkbox(&mut auto_hex, fl!(LOADER, "auto-hex-on-edit")).clicked() {
                                        self.auto_hex_on_edit.store(auto_hex, Ordering::SeqCst);
                                    }
                                    if ui.checkbox(&mut follow_pc, fl!(LOADER, "follow-pc")).clicked() {
                                        self.follow_pc_flag.store(follow_pc, Ordering::SeqCst);
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label(fl!(LOADER, "span-mode"));
                                    let full = policy == 0;
                                    let select_full = full;
                                    if ui.radio(select_full, fl!(LOADER, "full-span")).clicked() {
                                        policy = 0;
                                        self.span_policy.store(0, Ordering::SeqCst);
                                    }
                                    let select_win = !full;
                                    if ui.radio(select_win, fl!(LOADER, "pc-window")).clicked() {
                                        policy = 1;
                                        self.span_policy.store(1, Ordering::SeqCst);
                                    }
                                    ui.add(egui::DragValue::new(&mut win_len).clamp_range(16..=4096).speed(1));
                                    ui.label(fl!(LOADER, "bytes"));
                                    if self.window_len.load(Ordering::SeqCst) != win_len {
                                        self.window_len.store(win_len, Ordering::SeqCst);
                                    }
                                });
                                egui::ScrollArea::both()
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.style_mut().wrap = Some(false);
                                        let _paused = self.paused.load(Ordering::SeqCst);
                                        let cur_pc = self.snap.lock().map(|g| g.pc).unwrap_or(0);
                                        let pc_hex = format!("{cur_pc:04X}");
                                        // Iterate lines and highlight the one whose address token matches PC when paused.
                                        let mut highlight_rect: Option<egui::Rect> = None;
                                        let render_lines = |ui: &mut egui::Ui, text: &str, highlight_rect: &mut Option<egui::Rect>| {
                                            for line in text.lines() {
                                                let ltrim = line.trim_start();
                                                let mut token = ltrim.split_whitespace().next().unwrap_or("");
                                                if token.ends_with(':') {
                                                    token = &token[..token.len() - 1];
                                                }
                                                let addr_for_line: Option<u16> =
                                                    if token.len() == 4 { u16::from_str_radix(token, 16).ok() } else { None };
                                                // Avoid highlighting macro definition lines even if they show a PC-like address
                                                let upper = ltrim.to_ascii_uppercase();
                                                let is_macro_line = upper.contains(" MACRO")
                                                    || upper.contains(" ENDM")
                                                    || upper.starts_with("MACRO ");
                                                let is_pc_addr_token =
                                                    addr_for_line.is_some() && token.eq_ignore_ascii_case(&pc_hex) && !is_macro_line;
                                                let (label_run_until, label_set_pc, label_cancel) = (
                                                    fl!(LOADER, "run-to-here"),
                                                    fl!(LOADER, "set-pc-to-here"),
                                                    fl!(LOADER, "cancel"),
                                                );
                                                if is_pc_addr_token {
                                                    let resp = ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(line)
                                                                .monospace()
                                                                .background_color(egui::Color32::from_rgb(255, 0, 0)),
                                                        )
                                                        .sense(egui::Sense::hover()),
                                                    );
                                                    if let Some(addr) = addr_for_line {
                                                        resp.context_menu(|ui| {
                                                            if ui.button(label_run_until).clicked() {
                                                                if let Ok(mut q) = self.cmds.lock() {
                                                                    q.push(Command::RunUntil(addr));
                                                                }
                                                                ui.close_menu();
                                                            }
                                                            if ui.button(label_set_pc).clicked() {
                                                                if let Ok(mut q) = self.cmds.lock() {
                                                                    q.push(Command::SetPc(addr));
                                                                }
                                                                ui.close_menu();
                                                            }
                                                            if ui.button(label_cancel).clicked() {
                                                                ui.close_menu();
                                                            }
                                                        });
                                                    }
                                                    *highlight_rect = Some(resp.rect);
                                                } else {
                                                    let resp = ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(line).monospace(),
                                                        )
                                                        .sense(egui::Sense::hover()),
                                                    );
                                                    if let Some(addr) = addr_for_line {
                                                        resp.context_menu(|ui| {
                                                            if ui.button(label_run_until).clicked() {
                                                                if let Ok(mut q) = self.cmds.lock() {
                                                                    q.push(Command::RunUntil(addr));
                                                                }
                                                                ui.close_menu();
                                                            }
                                                            if ui.button(label_set_pc).clicked() {
                                                                if let Ok(mut q) = self.cmds.lock() {
                                                                    q.push(Command::SetPc(addr));
                                                                }
                                                                ui.close_menu();
                                                            }
                                                            if ui.button(label_cancel).clicked() {
                                                                ui.close_menu();
                                                            }
                                                        });
                                                    }
                                                    // Hover highlight for the line under mouse cursor (non‑PC lines)
                                                    if resp.hovered() {
                                                        // Use a translucent fill so text remains readable, plus a subtle stroke
                                                        let fill =
                                                            egui::Color32::from_rgba_unmultiplied(128, 128, 128, 40);
                                                        let stroke_col = ui.visuals().widgets.hovered.fg_stroke.color;
                                                        ui.painter().rect_filled(
                                                            resp.rect,
                                                            egui::Rounding::same(0.0),
                                                            fill,
                                                        );
                                                        ui.painter().rect_stroke(
                                                            resp.rect,
                                                            egui::Rounding::same(0.0),
                                                            egui::Stroke::new(1.0, stroke_col),
                                                        );
                                                    }
                                                }
                                            }
                                        };
                                        if use_hex_view {
                                            if let Some(txt) = &self.lst_text {
                                                if let Ok(s) = txt.lock() {
                                                    render_lines(ui, s.as_str(), &mut highlight_rect);
                                                }
                                            }
                                        } else if let Some(m) = &self.lst_mmap {
                                            match std::str::from_utf8(&m[..]) {
                                                Ok(s) => render_lines(ui, s, &mut highlight_rect),
                                                Err(_) => {
                                                    let cow = String::from_utf8_lossy(&m[..]);
                                                    render_lines(ui, cow.as_ref(), &mut highlight_rect);
                                                }
                                            }
                                        }
                                        // Auto-scroll when PC changes or a view switch requested a scroll.
                                        // Make the scroll request sticky until we successfully scroll to a highlighted row.
                                        let need_scroll = self
                                            .scroll_to_pc_req
                                            .load(Ordering::SeqCst)
                                            || (cur_pc != self.last_pc_for_scroll);
                                        if follow_pc && need_scroll {
                                            if let Some(rect) = highlight_rect {
                                                ui.scroll_to_rect(rect, None);
                                                // Clear the request only after performing the scroll, and update last_pc marker.
                                                self.scroll_to_pc_req.store(false, Ordering::SeqCst);
                                                self.last_pc_for_scroll = cur_pc;
                                            }
                                        }
                                    });
                            });
                    });
            }

        // Left/center: registers and controls (scrollable)
        egui::CentralPanel::default().show(ctx, |ui| {
            // Ensure enough width so MMU map grid stays at 4 columns and splitter remains
            ui.set_min_width(320.0);
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.style_mut().wrap = Some(false);
                    let s = *self.snap.lock().unwrap();
                    ui.heading(fl!(LOADER, "registers-heading"));
                    ui.horizontal(|ui| {
                        if self.paused.load(Ordering::SeqCst) {
                            if ui.button(fl!(LOADER, "resume")).clicked() {
                                self.paused.store(false, Ordering::SeqCst);
                            }
                            if ui.button(fl!(LOADER, "step")).clicked() {
                                self.paused.store(true, Ordering::SeqCst);
                                self.step_requests.fetch_add(1, Ordering::SeqCst);
                            }
                        } else {
                            if ui.button(fl!(LOADER, "pause")).clicked() {
                                self.paused.store(true, Ordering::SeqCst);
                            }
                            if ui.button(fl!(LOADER, "step")).clicked() {
                                self.paused.store(true, Ordering::SeqCst);
                                self.step_requests.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                        ui.separator();
                        if self.edit_mode {
                            if ui.button(fl!(LOADER, "apply")).clicked() {
                                // Parse inputs; numbers accept 0x/0o/0b or decimal
                                let parse8 = |s: &str| -> Option<u8> {
                                    parse_num_u16_any_gui(s).and_then(|v| {
                                        if v <= 0xFF {
                                            Some(v as u8)
                                        } else {
                                            None
                                        }
                                    })
                                };
                                let parse16 = |s: &str| -> Option<u16> { parse_num_u16_any_gui(s) };
                                let a_opt = parse8(&self.edit_a);
                                let b_opt = parse8(&self.edit_b);
                                let d_opt = parse16(&self.edit_d);
                                let x_opt = parse16(&self.edit_x);
                                let y_opt = parse16(&self.edit_y);
                                let u_opt = parse16(&self.edit_u);
                                let s_opt = parse16(&self.edit_s);
                                let dp_opt = parse8(&self.edit_dp);
                                let pc_opt = parse16(&self.edit_pc);
                                let cc_val: u8 = (if self.cc_e { 0x80 } else { 0 })
                                    | (if self.cc_f { 0x40 } else { 0 })
                                    | (if self.cc_h { 0x20 } else { 0 })
                                    | (if self.cc_i { 0x10 } else { 0 })
                                    | (if self.cc_n { 0x08 } else { 0 })
                                    | (if self.cc_z { 0x04 } else { 0 })
                                    | (if self.cc_v { 0x02 } else { 0 })
                                    | (if self.cc_c { 0x01 } else { 0 });
                                if let (
                                    Some(mut a),
                                    Some(mut b),
                                    Some(x),
                                    Some(y),
                                    Some(u),
                                    Some(s),
                                    Some(dp),
                                    Some(pc),
                                ) = (a_opt, b_opt, x_opt, y_opt, u_opt, s_opt, dp_opt, pc_opt)
                                {
                                    // D takes precedence if provided
                                    if let Some(dv) = d_opt {
                                        a = (dv >> 8) as u8;
                                        b = dv as u8;
                                    }
                                    let snap = RegSnapshot {
                                        a,
                                        b,
                                        d: (((a as u16) << 8) | (b as u16)),
                                        x,
                                        y,
                                        u,
                                        s,
                                        dp,
                                        pc,
                                        cc: cc_val,
                                    };
                                    if let Ok(mut q) = self.cmds.lock() {
                                        q.push(Command::ApplyRegs(snap));
                                    }
                                    self.input_error = None;
                                    // Leave editing mode after successful apply
                                    self.edit_mode = false;
                                    self.edit_initialized = false;
                                } else {
                                    self.input_error = Some(fl!(LOADER, "invalid-reg-values"));
                                }
                            }
                            if ui.button(fl!(LOADER, "cancel")).clicked() {
                                if let Ok(s2) = self.snap.lock() {
                                    self.edit_a = format!("0x{:02X}", s2.a);
                                    self.edit_b = format!("0x{:02X}", s2.b);
                                    self.edit_d = format!("0x{:04X}", s2.d);
                                    self.edit_x = format!("0x{:04X}", s2.x);
                                    self.edit_y = format!("0x{:04X}", s2.y);
                                    self.edit_u = format!("0x{:04X}", s2.u);
                                    self.edit_s = format!("0x{:04X}", s2.s);
                                    self.edit_dp = format!("0x{:02X}", s2.dp);
                                    self.edit_pc = format!("0x{:04X}", s2.pc);
                                    self.cc_e = (s2.cc & 0x80) != 0;
                                    self.cc_f = (s2.cc & 0x40) != 0;
                                    self.cc_h = (s2.cc & 0x20) != 0;
                                    self.cc_i = (s2.cc & 0x10) != 0;
                                    self.cc_n = (s2.cc & 0x08) != 0;
                                    self.cc_z = (s2.cc & 0x04) != 0;
                                    self.cc_v = (s2.cc & 0x02) != 0;
                                    self.cc_c = (s2.cc & 0x01) != 0;
                                }
                                self.input_error = None;
                                self.edit_mode = false;
                                self.edit_initialized = false;
                            }
                        } else if ui.button(fl!(LOADER, "edit")).clicked() {
                            self.edit_mode = true;
                            self.edit_initialized = false;
                        }
                    });
                    if let Some(err) = &self.input_error {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                    ui.separator();
                    TableBuilder::new(ui)
                        .striped(true)
                        .resizable(false)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .column(Column::auto().at_least(40.0))
                        .column(Column::remainder())
                        .header(22.0, |mut header| {
                            header.col(|ui| {
                                ui.strong(fl!(LOADER, "reg"));
                                let rect = ui.max_rect();
                                let stroke = egui::Stroke::new(
                                    1.0,
                                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                                );
                                ui.painter()
                                    .rect_stroke(rect, egui::Rounding::same(0.0), stroke);
                            });
                            header.col(|ui| {
                                ui.strong(fl!(LOADER, "value"));
                                let rect = ui.max_rect();
                                let stroke = egui::Stroke::new(
                                    1.0,
                                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                                );
                                ui.painter()
                                    .rect_stroke(rect, egui::Rounding::same(0.0), stroke);
                            });
                        })
                        .body(|mut body| {
                            // Editable rows while paused; otherwise show live snapshot values
                            let mut add_row =
                                |name: &str, edit: &mut String, hint: &str, show_val: String| {
                                    body.row(22.0, |mut row| {
                                        row.col(|ui| {
                                            ui.monospace(name);
                                            let rect = ui.max_rect();
                                            let stroke = egui::Stroke::new(
                                                1.0,
                                                ui.visuals().widgets.noninteractive.bg_stroke.color,
                                            );
                                            ui.painter().rect_stroke(
                                                rect,
                                                egui::Rounding::same(0.0),
                                                stroke,
                                            );
                                        });
                                        row.col(|ui| {
                                            if editing {
                                                let te = egui::TextEdit::singleline(edit)
                                                    .desired_width(100.0)
                                                    .hint_text(hint);
                                                let _resp = ui.add(te);
                                            } else {
                                                ui.monospace(show_val);
                                            }
                                            let rect = ui.max_rect();
                                            let stroke = egui::Stroke::new(
                                                1.0,
                                                ui.visuals().widgets.noninteractive.bg_stroke.color,
                                            );
                                            ui.painter().rect_stroke(
                                                rect,
                                                egui::Rounding::same(0.0),
                                                stroke,
                                            );
                                        });
                                    });
                                };
                            add_row("A", &mut self.edit_a, "0x00", format!("${:02X}", s.a));
                            add_row("B", &mut self.edit_b, "0x00", format!("${:02X}", s.b));
                            add_row("D", &mut self.edit_d, "0x0000", format!("${:04X}", s.d));
                            add_row("X", &mut self.edit_x, "0x0000", format!("${:04X}", s.x));
                            add_row("Y", &mut self.edit_y, "0x0000", format!("${:04X}", s.y));
                            add_row("U", &mut self.edit_u, "0x0000", format!("${:04X}", s.u));
                            add_row("S", &mut self.edit_s, "0x0000", format!("${:04X}", s.s));
                            add_row("DP", &mut self.edit_dp, "0x00", format!("${:02X}", s.dp));
                            add_row("PC", &mut self.edit_pc, "0x0000", format!("${:04X}", s.pc));

                            // CC flags as toggles
                            body.row(24.0, |mut row| {
                                row.col(|ui| {
                                    ui.monospace("CC");
                                    let rect = ui.max_rect();
                                    let stroke = egui::Stroke::new(
                                        1.0,
                                        ui.visuals().widgets.noninteractive.bg_stroke.color,
                                    );
                                    ui.painter()
                                        .rect_stroke(rect, egui::Rounding::same(0.0), stroke);
                                });
                                row.col(|ui| {
                                    if editing {
                                        ui.horizontal(|ui| {
                                            ui.toggle_value(&mut self.cc_e, "E");
                                            ui.toggle_value(&mut self.cc_f, "F");
                                            ui.toggle_value(&mut self.cc_h, "H");
                                            ui.toggle_value(&mut self.cc_i, "I");
                                            ui.toggle_value(&mut self.cc_n, "N");
                                            ui.toggle_value(&mut self.cc_z, "Z");
                                            ui.toggle_value(&mut self.cc_v, "V");
                                            ui.toggle_value(&mut self.cc_c, "C");
                                        });
                                    } else {
                                        let flags = [
                                            ('E', 0b1000_0000u8),
                                            ('F', 0b0100_0000u8),
                                            ('H', 0b0010_0000u8),
                                            ('I', 0b0001_0000u8),
                                            ('N', 0b0000_1000u8),
                                            ('Z', 0b0000_0100u8),
                                            ('V', 0b0000_0010u8),
                                            ('C', 0b0000_0001u8),
                                        ];
                                        let mut cc_text = String::with_capacity(17);
                                        for (i, (ch, mask)) in flags.iter().enumerate() {
                                            if i > 0 {
                                                cc_text.push(' ');
                                            }
                                            if (s.cc & mask) != 0 {
                                                cc_text.push(*ch);
                                            } else {
                                                cc_text.push(ch.to_ascii_lowercase());
                                            }
                                        }
                                        ui.monospace(cc_text);
                                    }
                                    let rect = ui.max_rect();
                                    let stroke = egui::Stroke::new(
                                        1.0,
                                        ui.visuals().widgets.noninteractive.bg_stroke.color,
                                    );
                                    ui.painter()
                                        .rect_stroke(rect, egui::Rounding::same(0.0), stroke);
                                });
                            });
                        });

                    ui.separator();
                    // Console -> Block -> GPIO -> Timer -> MMU quick panels (tabbed)
                    ui.horizontal(|ui| {
                        let mut tab_button = |label: &str, tab: DeviceTab| {
                            if ui.selectable_label(self.dev_tab == tab, label).clicked() {
                                self.dev_tab = tab;
                            }
                        };
                        tab_button("Console", DeviceTab::Console);
                        tab_button("Block", DeviceTab::Block);
                        tab_button("GPIO", DeviceTab::Gpio);
                        tab_button("Timer", DeviceTab::Timer);
                        tab_button("MMU", DeviceTab::Mmu);
                    });
                    ui.separator();
                    match self.dev_tab {
                        DeviceTab::Console => {
                            if self.console_enabled {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.monospace("Console:");
                                        if let Some(base) = self.console_base {
                                            ui.monospace(format!("@${:04X}", base));
                                        }
                                    });
                                    ui.horizontal_wrapped(|ui| {
                                        let (label, color) = if self.settings_baseline.console_stdin {
                                            ("STDIN", egui::Color32::from_rgb(0x4C, 0xAF, 0x50))
                                        } else {
                                            ("STDIN OFF", egui::Color32::DARK_GRAY)
                                        };
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(70.0, 18.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(rect.shrink(3.0), 4.0, color);
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            label,
                                            egui::TextStyle::Monospace.resolve(ui.style()),
                                            egui::Color32::BLACK,
                                        );
                                    });
                                });
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    ui.monospace("Console:");
                                    ui.small("disabled");
                                });
                            }
                        }
                        DeviceTab::Block => {
                            if self.block_present {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.monospace("Block device:");
                                        if let Some(base) = self.block_base {
                                            ui.monospace(format!("@${base:04X}"));
                                        }
                                        ui.add_space(8.0);
                                        let badge = |ui: &mut egui::Ui, label: &str, on: bool| {
                                            let (rect, _) = ui.allocate_exact_size(
                                                egui::vec2(48.0, 18.0),
                                                egui::Sense::hover(),
                                            );
                                            let color = if on {
                                                egui::Color32::from_rgb(0x4C, 0xAF, 0x50)
                                            } else {
                                                egui::Color32::from_gray(90)
                                            };
                                            ui.painter().rect_filled(rect.shrink(3.0), 4.0, color);
                                            ui.painter().text(
                                                rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                label,
                                                egui::TextStyle::Monospace.resolve(ui.style()),
                                                egui::Color32::BLACK,
                                            );
                                        };
                                        let irq_on = self.settings_baseline.block_irq_enable;
                                        let firq_on = self.settings_baseline.block_firq;
                                        badge(ui, "IRQ", irq_on);
                                        ui.add_space(6.0);
                                        badge(ui, "FIRQ", firq_on);
                                    });
                                    ui.horizontal_wrapped(|ui| {
                                        let (label, color) = if self.block_backing_online {
                                            ("ONLINE", egui::Color32::from_rgb(0x4C, 0xAF, 0x50))
                                        } else if self.block_backing_path.is_some() {
                                            ("MISSING", egui::Color32::from_rgb(0xE6, 0xA8, 0x13))
                                        } else {
                                            ("NO FILE", egui::Color32::DARK_GRAY)
                                        };
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(70.0, 18.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(rect.shrink(3.0), 4.0, color);
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            label,
                                            egui::TextStyle::Monospace.resolve(ui.style()),
                                            egui::Color32::BLACK,
                                        );
                                        if let Some(path) = &self.block_backing_path {
                                            let display = std::path::Path::new(path)
                                                .file_name()
                                                .and_then(|s| s.to_str())
                                                .unwrap_or(path);
                                            let resp = ui.small(display.to_string());
                                            resp.on_hover_text(path);
                                        } else {
                                            ui.small("[no backing file]");
                                        }
                                    });
                                    ui.horizontal_wrapped(|ui| {
                                        ui.monospace(format!(
                                            "CMD=${:02X} DATA=${:02X} STAT=${:02X}",
                                            self.block_last_cmd, self.block_last_data, self.block_status
                                        ));
                                    });
                                });
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    ui.monospace("Block device:");
                                    ui.small("disabled");
                                });
                            }
                        }
                        DeviceTab::Gpio => {
                            if self.gpio_present {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("GPIO LEDs:");
                                        if let Some(base) =
                                            parse_u16_any(&self.settings_baseline.gpio_base_hex)
                                        {
                                            ui.monospace(format!("@${base:04X}"));
                                        }
                                    });
                                    let bits = self.gpio_bits as usize;
                                    let mut new_out = self.gpio_out;
                                    let rows = (bits + 7) / 8;
                                    for r in 0..rows {
                                        let start = r * 8;
                                        let end = usize::min(bits, start + 8);
                                        ui.horizontal(|ui| {
                                            for i in start..end {
                                                let is_out = ((self.gpio_dir >> i) & 1) != 0;
                                                let on = ((new_out >> i) & 1) != 0;
                                                let color = if is_out {
                                                    if on {
                                                        self.gpio_on_color
                                                    } else {
                                                        egui::Color32::DARK_GRAY
                                                    }
                                                } else {
                                                    egui::Color32::GRAY
                                                };
                                                let label = format!("{i:02}");
                                                let (rect, resp) = ui.allocate_exact_size(
                                                    egui::vec2(20.0, 20.0),
                                                    egui::Sense::click(),
                                                );
                                                ui.painter().rect_filled(rect.shrink(2.0), 4.0, color);
                                                ui.painter().text(
                                                    rect.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    label,
                                                    egui::TextStyle::Small.resolve(ui.style()),
                                                    egui::Color32::BLACK,
                                                );
                                                if is_out && resp.clicked() {
                                                    new_out ^= 1u32 << i;
                                                }
                                            }
                                        });
                                    }
                                    if new_out != self.gpio_out {
                                        self.gpio_out = new_out;
                                        if let Ok(mut q) = self.cmds.lock() {
                                            q.push(Command::GpioSetOut(new_out));
                                        }
                                    }
                                });
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    ui.monospace("GPIO:");
                                    ui.small("disabled");
                                });
                            }
                        }
                        DeviceTab::Timer => {
                            if self.timer_present && self.settings_baseline.use_timer {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.monospace("Timer:");
                                        if let Some(base) =
                                            parse_u16_any(&self.settings_baseline.timer_base_hex)
                                        {
                                            ui.monospace(format!("@${base:04X}"));
                                        }
                                        ui.add_space(8.0);
                                        let badge = |ui: &mut egui::Ui, label: &str, on: bool| {
                                            let (rect, _) = ui.allocate_exact_size(
                                                egui::vec2(48.0, 18.0),
                                                egui::Sense::hover(),
                                            );
                                            let color = if on {
                                                egui::Color32::from_rgb(0x4C, 0xAF, 0x50)
                                            } else {
                                                egui::Color32::from_gray(90)
                                            };
                                            ui.painter().rect_filled(rect.shrink(3.0), 4.0, color);
                                            ui.painter().text(
                                                rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                label,
                                                egui::TextStyle::Monospace.resolve(ui.style()),
                                                egui::Color32::BLACK,
                                            );
                                        };
                                        badge(ui, "IRQ", self.settings_baseline.timer_irq_enable);
                                        ui.add_space(6.0);
                                        badge(ui, "FIRQ", self.settings_baseline.timer_firq);
                                        ui.add_space(6.0);
                                        badge(ui, "START", self.settings_baseline.timer_start);
                                    });
                                    ui.horizontal(|ui| {
                                        let run_color = if self.timer_running {
                                            egui::Color32::LIGHT_GREEN
                                        } else {
                                            egui::Color32::DARK_GRAY
                                        };
                                        let (rrect, _) = ui
                                            .allocate_exact_size(egui::vec2(50.0, 18.0), egui::Sense::hover());
                                        ui.painter().rect_filled(rrect.shrink(3.0), 6.0, run_color);
                                        let run_label = if self.timer_running { "RUN" } else { "stop" };
                                        ui.painter().text(
                                            rrect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            run_label,
                                            egui::TextStyle::Monospace.resolve(ui.style()),
                                            egui::Color32::BLACK,
                                        );
                                        let color = if self.timer_pending {
                                            egui::Color32::LIGHT_RED
                                        } else {
                                            egui::Color32::DARK_GREEN
                                        };
                                        let (rect, _resp) = ui
                                            .allocate_exact_size(egui::vec2(60.0, 18.0), egui::Sense::hover());
                                        ui.painter().rect_filled(rect.shrink(3.0), 6.0, color);
                                        let label = if self.timer_pending { "PENDING" } else { "idle" };
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            label,
                                            egui::TextStyle::Monospace.resolve(ui.style()),
                                            egui::Color32::BLACK,
                                        );
                                    });
                                });
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    ui.monospace("Timer:");
                                    ui.small("disabled");
                                });
                            }
                        }
                        DeviceTab::Mmu => {
                            if self.settings_baseline.use_mmu {
                                if self.mmu_present {
                                    ui.vertical(|ui| {
                                        let title = if self.mmu_sys_mode {
                                            format!("MMU map (SYS) task{}", self.mmu_task)
                                        } else {
                                            format!("MMU map (USER) task{}", self.mmu_task)
                                        };
                                        ui.monospace(title);
                                        ui.horizontal_wrapped(|ui| {
                                            for p in 0..16 {
                                                let f = self.mmu_map_cur[p];
                                                let prev = self.mmu_map_prev[p];
                                                let changed = f != prev;
                                                let text = format!("P{:X}:F{:02X}", p, f & 0xFF);
                                                let (rect, _resp) = ui.allocate_exact_size(
                                                    egui::vec2(70.0, 20.0),
                                                    egui::Sense::hover(),
                                                );
                                                let bg = if changed {
                                                    egui::Color32::DARK_GREEN
                                                } else {
                                                    egui::Color32::DARK_GRAY
                                                };
                                                ui.painter().rect_filled(rect.shrink(2.0), 3.0, bg);
                                                ui.painter().text(
                                                    rect.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    text,
                                                    egui::TextStyle::Monospace.resolve(ui.style()),
                                                    egui::Color32::WHITE,
                                                );
                                            }
                                        });
                                    });
                                }
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    ui.monospace("MMU:");
                                    ui.small("disabled");
                                });
                            }
                        }
                    }

                    // Error display moved next to the Edit/Apply controls at the top

                    ctx.request_repaint_after(std::time::Duration::from_millis(16));
                 });
        });
        }
    }

    pub struct RegView {
        pub snapshot: Arc<Mutex<RegSnapshot>>,
        pub paused: Arc<AtomicBool>,
        pub step_requests: Arc<AtomicUsize>,
        pub lst_mmap: Option<Arc<Mmap>>,
        pub lst_text: Option<Arc<Mutex<String>>>,
        use_hex: Arc<AtomicBool>,
        auto_hex_on_edit: Arc<AtomicBool>,
        scroll_to_pc_req: Arc<AtomicBool>,
        follow_pc_flag: Arc<AtomicBool>,
        span_policy: Arc<AtomicUsize>,
        window_len: Arc<AtomicUsize>,
        cmds: Arc<Mutex<Vec<Command>>>,
        ja_font: Option<Arc<Vec<u8>>>,
        _handle: Option<std::thread::JoinHandle<()>>,
        // Memory view shared state
        mem_follow_pc: Arc<AtomicBool>,
        mem_window_len: Arc<AtomicUsize>,
        mem_window_start: Arc<AtomicUsize>,
        mem_buf: Arc<Mutex<(u16, Vec<u8>)>>,
        mem_hl: Arc<Mutex<Option<(u16, usize)>>>,
        mem_utf8: Arc<AtomicBool>,
        quit_flag: Arc<AtomicBool>,
        alive: Arc<AtomicBool>,
        started: Arc<AtomicBool>,
        // Emulator->GUI sync
        gpio_sync: Arc<Mutex<GpioSyncState>>,
        mmu_sync: Arc<Mutex<MmuSyncState>>,
        dev_tab: DeviceTab,
    }

    impl Clone for RegView {
        fn clone(&self) -> Self {
            Self {
                snapshot: Arc::clone(&self.snapshot),
                paused: Arc::clone(&self.paused),
                step_requests: Arc::clone(&self.step_requests),
                lst_mmap: self.lst_mmap.as_ref().map(Arc::clone),
                lst_text: self.lst_text.as_ref().map(Arc::clone),
                use_hex: Arc::clone(&self.use_hex),
                auto_hex_on_edit: Arc::clone(&self.auto_hex_on_edit),
                scroll_to_pc_req: Arc::clone(&self.scroll_to_pc_req),
                follow_pc_flag: Arc::clone(&self.follow_pc_flag),
                span_policy: Arc::clone(&self.span_policy),
                window_len: Arc::clone(&self.window_len),
                cmds: Arc::clone(&self.cmds),
                ja_font: self.ja_font.as_ref().map(Arc::clone),
                _handle: None,
                mem_follow_pc: Arc::clone(&self.mem_follow_pc),
                mem_window_len: Arc::clone(&self.mem_window_len),
                mem_window_start: Arc::clone(&self.mem_window_start),
                mem_buf: Arc::clone(&self.mem_buf),
                mem_hl: Arc::clone(&self.mem_hl),
                mem_utf8: Arc::clone(&self.mem_utf8),
                quit_flag: Arc::clone(&self.quit_flag),
                alive: Arc::clone(&self.alive),
                started: Arc::clone(&self.started),
                gpio_sync: Arc::new(Mutex::new(None)),
                mmu_sync: Arc::new(Mutex::new(None)),
                dev_tab: self.dev_tab,
            }
        }
    }

    impl RegView {
        #[allow(dead_code)]
        fn is_japanese_locale() -> bool {
            if let Ok(v) = std::env::var("EM6809_LANG") {
                let vl = v.to_lowercase();
                if vl.starts_with("ja") {
                    return true;
                }
                if vl.starts_with("en") {
                    return false;
                }
            }
            for key in ["LC_ALL", "LC_MESSAGES", "LANG"].iter() {
                if let Ok(v) = std::env::var(key) {
                    if v.to_lowercase().starts_with("ja") {
                        return true;
                    }
                }
            }
            #[cfg(windows)]
            {
                #[link(name = "kernel32")]
                extern "system" {
                    fn GetUserDefaultLocaleName(lpLocaleName: *mut u16, cchLocaleName: i32) -> i32;
                }
                let mut buf = [0u16; 85];
                let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
                if n > 0 {
                    if let Ok(s) = String::from_utf16(&buf[..(n as usize - 1)]) {
                        if s.to_lowercase().starts_with("ja") {
                            return true;
                        }
                    }
                }
            }
            false
        }
        pub fn spawn(
            title: &str,
            lst_path: Option<PathBuf>,
            lst_fallback_text: Option<String>,
            initial_start: u16,
        ) -> Self {
            let snapshot = Arc::new(Mutex::new(RegSnapshot::default()));
            let paused = Arc::new(AtomicBool::new(false));
            let step_requests = Arc::new(AtomicUsize::new(0));
            let lst_mmap: Option<Arc<Mmap>> = lst_path.and_then(|p| {
                if let Ok(f) = File::open(p) {
                    unsafe { Mmap::map(&f).ok() }.map(Arc::new)
                } else {
                    None
                }
            });
            let lst_text: Option<Arc<Mutex<String>>> =
                lst_fallback_text.map(|s| Arc::new(Mutex::new(s)));
            let default_use_hex_view: bool = lst_mmap.is_none();
            let use_hex = Arc::new(AtomicBool::new(default_use_hex_view));
            let auto_hex_on_edit = Arc::new(AtomicBool::new(true));
            let scroll_to_pc_req = Arc::new(AtomicBool::new(false));
            let follow_pc_flag = Arc::new(AtomicBool::new(true));
            let span_policy = Arc::new(AtomicUsize::new(0)); // FullSpan
            let window_len = Arc::new(AtomicUsize::new(256));
            let cmds: Arc<Mutex<Vec<Command>>> = Arc::new(Mutex::new(Vec::new()));

            // Memory view defaults
            let mem_follow_pc = Arc::new(AtomicBool::new(true));
            let mem_window_len = Arc::new(AtomicUsize::new(256));
            let mem_window_start = Arc::new(AtomicUsize::new(initial_start as usize));
            let mem_buf: Arc<Mutex<(u16, Vec<u8>)>> = Arc::new(Mutex::new((0, Vec::new())));
            let mem_hl: Arc<Mutex<Option<(u16, usize)>>> = Arc::new(Mutex::new(None));
            let mem_utf8 = Arc::new(AtomicBool::new(false));

            // Try to discover a Japanese-capable font from common system locations
            let ja_font: Option<Arc<Vec<u8>>> = {
                if let Ok(p) = std::env::var("EM6809_GUI_FONT") {
                    if !p.is_empty() {
                        if let Ok(b) = std::fs::read(&p) {
                            Some(Arc::new(b))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    #[cfg(windows)]
                    {
                        let candidates = [
                            "C:/Windows/Fonts/Meiryo.ttc",
                            "C:/Windows/Fonts/meiryo.ttc",
                            "C:/Windows/Fonts/YuGothR.ttc",
                            "C:/Windows/Fonts/YuGothM.ttc",
                            "C:/Windows/Fonts/msgothic.ttc",
                        ];
                        let mut found: Option<Arc<Vec<u8>>> = None;
                        for p in candidates.iter() {
                            if let Ok(b) = std::fs::read(p) {
                                found = Some(Arc::new(b));
                                break;
                            }
                        }
                        found
                    }
                    #[cfg(target_os = "macos")]
                    {
                        let candidates = [
                            // English-name Hiragino (newer macOS)
                            "/System/Library/Fonts/Hiragino Sans W6.ttc",
                            "/System/Library/Fonts/Hiragino Sans W3.ttc",
                            "/Library/Fonts/Hiragino Sans W6.ttc",
                            "/Library/Fonts/Hiragino Sans W3.ttc",
                            // Older ProN names (may be installed in /Library/Fonts)
                            "/Library/Fonts/Hiragino Kaku Gothic ProN W6.otf",
                            "/Library/Fonts/Hiragino Kaku Gothic ProN W3.otf",
                            // Japanese-name variants (some macOS locales)
                            "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc",
                            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
                            "/System/Library/Fonts/AppleGothic.ttf",
                        ];
                        let mut found: Option<Arc<Vec<u8>>> = None;
                        for p in candidates.iter() {
                            if let Ok(b) = std::fs::read(p) {
                                found = Some(Arc::new(b));
                                break;
                            }
                        }
                        found
                    }
                    #[cfg(all(not(windows), not(target_os = "macos")))]
                    {
                        let candidates = [
                            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                            "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf",
                            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
                            "/usr/share/fonts/truetype/noto/NotoSansCJKjp-Regular.otf",
                        ];
                        let mut found: Option<Arc<Vec<u8>>> = None;
                        for p in candidates.iter() {
                            if let Ok(b) = std::fs::read(p) {
                                found = Some(Arc::new(b));
                                break;
                            }
                        }
                        found
                    }
                }
            };

            let snap_clone = Arc::clone(&snapshot);
            let paused_clone = Arc::clone(&paused);
            let step_requests_clone = Arc::clone(&step_requests);
            let lst_clone = lst_mmap.as_ref().map(Arc::clone);
            let lst_text_clone = lst_text.as_ref().map(Arc::clone);
            let use_hex_clone = Arc::clone(&use_hex);
            let follow_clone = Arc::clone(&follow_pc_flag);
            let span_policy_clone = Arc::clone(&span_policy);
            let window_len_clone = Arc::clone(&window_len);
            let cmds_clone = Arc::clone(&cmds);
            let ja_font_clone = ja_font.as_ref().map(Arc::clone);
            let title = title.to_string();
            let use_japanese = Self::is_japanese_locale();

            let mem_follow_pc_thread = mem_follow_pc.clone();
            let mem_window_len_thread = mem_window_len.clone();
            let mem_window_start_thread = mem_window_start.clone();
            let mem_buf_thread = mem_buf.clone();
            let mem_hl_thread = mem_hl.clone();
            let mem_utf8_thread = mem_utf8.clone();
            let quit_flag_thread = Arc::new(AtomicBool::new(false));
            let quit_flag_thread_clone = quit_flag_thread.clone();
            let alive_flag = Arc::new(AtomicBool::new(false));
            let alive_flag_thread = alive_flag.clone();
            let started_flag = Arc::new(AtomicBool::new(false));
            let started_flag_thread = started_flag.clone();
            let auto_hex_on_edit_thread = Arc::clone(&auto_hex_on_edit);
            let scroll_to_pc_req_thread = Arc::clone(&scroll_to_pc_req);
            let handle = std::thread::spawn(move || {
                let mut native_options = eframe::NativeOptions::default();
                // Make the window compact and appropriate for the register grid
                let base = eframe::egui::ViewportBuilder::default()
                    .with_inner_size(eframe::egui::vec2(1400.0, 700.0))
                    .with_min_inner_size(eframe::egui::vec2(1400.0, 700.0))
                    .with_resizable(true);
                #[cfg(all(unix, not(target_os = "macos")))]
                let vp = {
                    #[cfg(target_os = "linux")]
                    {
                        let mut b = base.clone().with_app_id("em6809");
                        if std::env::var("EM6809_CSD")
                            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                            .unwrap_or(false)
                        {
                            b = b.with_decorations(false);
                        }
                        b
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        base.with_app_id("em6809")
                    }
                };
                #[cfg(not(all(unix, not(target_os = "macos"))))]
                let vp = base;
                native_options.viewport = vp;
                // On Windows, winit requires the event loop to be created on the main thread by default.
                // Allow creating it on this background thread to avoid a panic.
                #[cfg(windows)]
                {
                    native_options.event_loop_builder = Some(Box::new(|builder| {
                        use winit::platform::windows::EventLoopBuilderExtWindows;
                        builder.with_any_thread(true);
                    }));
                }
                // Note: On some Unix configurations (e.g., WSL2/Wayland), winit requires
                // the event loop to live on the main thread. If creating on a background
                // thread panics, we catch it below and continue without the GUI.
                let app = RegApp {
                    snap: snap_clone,
                    paused: paused_clone,
                    step_requests: step_requests_clone,
                    lst_mmap: lst_clone,
                    lst_text: lst_text_clone,
                    use_hex: use_hex_clone,
                    auto_hex_on_edit: Arc::clone(&auto_hex_on_edit_thread),
                    scroll_to_pc_req: Arc::clone(&scroll_to_pc_req_thread),
                    follow_pc_flag: follow_clone,
                    span_policy: span_policy_clone,
                    window_len: window_len_clone,
                    cmds: cmds_clone,
                    ja_font: ja_font_clone,
                    fonts_applied: false,
                    menu_fixed: false,
                    use_japanese,
                    last_pc_for_scroll: 0,
                    open_dialog_active: false,
                    open_pending_path: None,
                    open_bin_base_text: String::new(),
                    #[cfg(target_os = "linux")]
                    csd: std::env::var("EM6809_CSD")
                        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                        .unwrap_or(false),
                    #[cfg(target_os = "linux")]
                    csd_maximized: false,
                    #[cfg(windows)]
                    about_open: false,
                    #[cfg(windows)]
                    about_icon_tex: None,
                    #[cfg(target_os = "linux")]
                    about_open: false,
                    #[cfg(target_os = "linux")]
                    about_icon_tex: None,
                    mem_follow_pc: mem_follow_pc_thread.clone(),
                    mem_window_len: mem_window_len_thread.clone(),
                    mem_window_start: mem_window_start_thread.clone(),
                    mem_buf: mem_buf_thread.clone(),
                    mem_hl: mem_hl_thread.clone(),
                    mem_utf8: mem_utf8_thread.clone(),
                    mem_jump_text: format!("0x{initial_start:04X}"),
                    mem_len_text: format!("{}", mem_window_len_thread.load(Ordering::SeqCst)),
                    quit_flag: quit_flag_thread_clone,
                    mem_use_drag: false,
                    mem_edit_mode: false,
                    mem_pending: HashMap::new(),
                    mem_selected: None,
                    mem_edit_buf: String::new(),
                    mem_focus_next: false,
                    mem_last_sel_addr: None,
                    mem_last_at_start: false,
                    mem_last_at_end: false,
                    edit_mode: false,
                    edit_initialized: false,
                    edit_a: String::new(),
                    edit_b: String::new(),
                    edit_d: String::new(),
                    edit_x: String::new(),
                    edit_y: String::new(),
                    edit_u: String::new(),
                    edit_s: String::new(),
                    edit_dp: String::new(),
                    edit_pc: String::new(),
                    cc_e: false,
                    cc_f: false,
                    cc_h: false,
                    cc_i: false,
                    cc_n: false,
                    cc_z: false,
                    cc_v: false,
                    cc_c: false,
                    input_error: None,
                    gpio_present: true,
                    gpio_bits: 8,
                    gpio_out: 0,
                    gpio_dir: 0xFFFF_FFFF,
                    block_present: false,
                    block_base: None,
                    block_backing_path: None,
                    block_backing_online: false,
                    block_last_cmd: 0,
                    block_last_data: 0,
                    block_status: 0,
                    gpio_on_color: egui::Color32::LIGHT_YELLOW,
                    gpio_on_color_baseline: egui::Color32::LIGHT_YELLOW,
                    gpio_on_color_text: String::new(),
                    gpio_sync: Arc::new(Mutex::new(None)),
                    mmu_sync: Arc::new(Mutex::new(None)),
                    mmu_present: false,
                    mmu_sys_mode: false,
                    mmu_task: 0,
                    mmu_map_prev: [0u16; 16],
                    mmu_map_cur: [0u16; 16],
                    timer_present: false,
                    timer_pending: false,
                    timer_running: false,
                    settings_open: false,
                    settings_tab: 0,
                    settings: SettingsState::default(),
                    settings_baseline: SettingsState::default(),
                    settings_instance: 0,
                    settings_busy: false,
                    settings_busy_since: None,
                    settings_origins: HashMap::new(),
                    console_enabled: false,
                    console_base: None,
                    dev_tab: DeviceTab::Console,
                };
                // Windows supports any-thread event loop via ExtWindows; run normally.
                // On Unix (incl. WSL2), winit may require main-thread; catch and fall back headless.
                #[cfg(windows)]
                {
                    started_flag_thread.store(true, Ordering::SeqCst);
                    alive_flag_thread.store(true, Ordering::SeqCst);
                    let _ = eframe::run_native(&title, native_options, Box::new(|_| Box::new(app)));
                    // Window closed: mark not alive. Keep `started` true so is_closed() becomes true.
                    alive_flag_thread.store(false, Ordering::SeqCst);
                }
                #[cfg(not(windows))]
                {
                    use std::panic::{catch_unwind, AssertUnwindSafe};
                    // Suppress panic noise only during GUI bootstrap on Unix
                    let prev_hook = std::panic::take_hook();
                    std::panic::set_hook(Box::new(|_| {}));
                    started_flag_thread.store(true, Ordering::SeqCst);
                    alive_flag_thread.store(true, Ordering::SeqCst);
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        let _ = eframe::run_native(&title, native_options, Box::new(|_| Box::new(app)));
                    })).map_err(|_| {
                        eprintln!(
                            "[regview] GUI failed to start on this platform/thread. Running headless."
                        );
                    });
                    std::panic::set_hook(prev_hook);
                    alive_flag_thread.store(false, Ordering::SeqCst);
                    started_flag_thread.store(false, Ordering::SeqCst);
                }
            });

            Self {
                snapshot,
                paused,
                step_requests,
                lst_mmap,
                lst_text,
                use_hex,
                auto_hex_on_edit,
                scroll_to_pc_req,
                follow_pc_flag,
                span_policy,
                window_len,
                cmds,
                ja_font,
                _handle: Some(handle),
                mem_follow_pc,
                mem_window_len,
                mem_window_start,
                mem_buf,
                mem_hl,
                mem_utf8,
                quit_flag: quit_flag_thread,
                alive: alive_flag,
                started: started_flag,
                gpio_sync: Arc::new(Mutex::new(None)),
                mmu_sync: Arc::new(Mutex::new(None)),
                dev_tab: DeviceTab::Console,
            }
        }

        pub fn create(
            _title: &str,
            lst_path: Option<PathBuf>,
            lst_fallback_text: Option<String>,
            initial_start: u16,
        ) -> Self {
            // Build the same shared state as spawn(), but do not start a GUI thread.
            let snapshot = Arc::new(Mutex::new(RegSnapshot::default()));
            let paused = Arc::new(AtomicBool::new(false));
            let step_requests = Arc::new(AtomicUsize::new(0));
            let lst_mmap: Option<Arc<Mmap>> = lst_path.and_then(|p| {
                if let Ok(f) = File::open(p) {
                    unsafe { Mmap::map(&f).ok() }.map(Arc::new)
                } else {
                    None
                }
            });
            let lst_text: Option<Arc<Mutex<String>>> =
                lst_fallback_text.map(|s| Arc::new(Mutex::new(s)));
            let default_use_hex_view: bool = lst_mmap.is_none();
            let use_hex = Arc::new(AtomicBool::new(default_use_hex_view));
            let auto_hex_on_edit = Arc::new(AtomicBool::new(true));
            let scroll_to_pc_req = Arc::new(AtomicBool::new(false));
            let follow_pc_flag = Arc::new(AtomicBool::new(true));
            let span_policy = Arc::new(AtomicUsize::new(0));
            let window_len = Arc::new(AtomicUsize::new(256));
            let cmds: Arc<Mutex<Vec<Command>>> = Arc::new(Mutex::new(Vec::new()));
            let mem_follow_pc = Arc::new(AtomicBool::new(true));
            let mem_window_len = Arc::new(AtomicUsize::new(256));
            let mem_window_start = Arc::new(AtomicUsize::new(initial_start as usize));
            let mem_buf: Arc<Mutex<(u16, Vec<u8>)>> = Arc::new(Mutex::new((0, Vec::new())));
            let mem_hl: Arc<Mutex<Option<(u16, usize)>>> = Arc::new(Mutex::new(None));
            let mem_utf8 = Arc::new(AtomicBool::new(false));
            let quit_flag = Arc::new(AtomicBool::new(false));
            let alive = Arc::new(AtomicBool::new(false));
            let started = Arc::new(AtomicBool::new(false));

            // Font discovery (same as spawn)
            let ja_font: Option<Arc<Vec<u8>>> = {
                if let Ok(p) = std::env::var("EM6809_GUI_FONT") {
                    if !p.is_empty() {
                        if let Ok(b) = std::fs::read(&p) {
                            Some(Arc::new(b))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    #[cfg(windows)]
                    {
                        let candidates = [
                            "C:/Windows/Fonts/Meiryo.ttc",
                            "C:/Windows/Fonts/meiryo.ttc",
                            "C:/Windows/Fonts/YuGothR.ttc",
                            "C:/Windows/Fonts/YuGothM.ttc",
                            "C:/Windows/Fonts/msgothic.ttc",
                        ];
                        let mut found: Option<Arc<Vec<u8>>> = None;
                        for p in candidates.iter() {
                            if let Ok(b) = std::fs::read(p) {
                                found = Some(Arc::new(b));
                                break;
                            }
                        }
                        found
                    }
                    #[cfg(target_os = "macos")]
                    {
                        let candidates = [
                            // English-name Hiragino (newer macOS)
                            "/System/Library/Fonts/Hiragino Sans W6.ttc",
                            "/System/Library/Fonts/Hiragino Sans W3.ttc",
                            "/Library/Fonts/Hiragino Sans W6.ttc",
                            "/Library/Fonts/Hiragino Sans W3.ttc",
                            // Older ProN names (may be installed in /Library/Fonts)
                            "/Library/Fonts/Hiragino Kaku Gothic ProN W6.otf",
                            "/Library/Fonts/Hiragino Kaku Gothic ProN W3.otf",
                            // Japanese-name variants (some macOS locales)
                            "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc",
                            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
                            "/System/Library/Fonts/AppleGothic.ttf",
                        ];
                        let mut found: Option<Arc<Vec<u8>>> = None;
                        for p in candidates.iter() {
                            if let Ok(b) = std::fs::read(p) {
                                found = Some(Arc::new(b));
                                break;
                            }
                        }
                        found
                    }
                    #[cfg(all(not(windows), not(target_os = "macos")))]
                    {
                        let candidates = [
                            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                            "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf",
                            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
                            "/usr/share/fonts/truetype/noto/NotoSansCJKjp-Regular.otf",
                        ];
                        let mut found: Option<Arc<Vec<u8>>> = None;
                        for p in candidates.iter() {
                            if let Ok(b) = std::fs::read(p) {
                                found = Some(Arc::new(b));
                                break;
                            }
                        }
                        found
                    }
                }
            };

            Self {
                snapshot,
                paused,
                step_requests,
                lst_mmap,
                lst_text,
                use_hex,
                auto_hex_on_edit,
                scroll_to_pc_req,
                follow_pc_flag,
                span_policy,
                window_len,
                cmds,
                ja_font,
                _handle: None,
                mem_follow_pc,
                mem_window_len,
                mem_window_start,
                mem_buf,
                mem_hl,
                mem_utf8,
                quit_flag,
                alive,
                started,
                gpio_sync: Arc::new(Mutex::new(None)),
                mmu_sync: Arc::new(Mutex::new(None)),
                dev_tab: DeviceTab::Console,
            }
        }

        pub fn run_blocking(&self, title: &str) {
            let snap_clone = Arc::clone(&self.snapshot);
            let paused_clone = Arc::clone(&self.paused);
            let step_requests_clone = Arc::clone(&self.step_requests);
            let lst_clone = self.lst_mmap.as_ref().map(Arc::clone);
            let lst_text_clone = self.lst_text.as_ref().map(Arc::clone);
            let use_hex_clone = Arc::clone(&self.use_hex);
            let follow_clone = Arc::clone(&self.follow_pc_flag);
            let span_policy_clone = Arc::clone(&self.span_policy);
            let window_len_clone = Arc::clone(&self.window_len);
            let cmds_clone = Arc::clone(&self.cmds);
            let ja_font_clone = self.ja_font.as_ref().map(Arc::clone);
            let auto_hex_on_edit = Arc::clone(&self.auto_hex_on_edit);
            let mem_follow_pc_thread = self.mem_follow_pc.clone();
            let mem_window_len_thread = self.mem_window_len.clone();
            let mem_window_start_thread = self.mem_window_start.clone();
            let mem_buf_thread = self.mem_buf.clone();
            let mem_hl_thread = self.mem_hl.clone();
            let mem_utf8_thread = self.mem_utf8.clone();
            let initial_start = self.mem_window_start.load(Ordering::SeqCst) as u16;
            let use_japanese = Self::is_japanese_locale();

            let mut native_options = eframe::NativeOptions::default();
            let base = eframe::egui::ViewportBuilder::default()
                .with_inner_size(eframe::egui::vec2(1500.0, 700.0))
                .with_min_inner_size(eframe::egui::vec2(900.0, 400.0))
                .with_resizable(false);
            #[cfg(all(unix, not(target_os = "macos")))]
            let vp = {
                #[cfg(target_os = "linux")]
                {
                    let mut b = base.clone().with_app_id("em6809");
                    if std::env::var("EM6809_CSD")
                        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                        .unwrap_or(false)
                    {
                        b = b.with_decorations(false);
                    }
                    b
                }
                #[cfg(not(target_os = "linux"))]
                {
                    base.with_app_id("em6809")
                }
            };
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            let vp = base;
            native_options.viewport = vp;
            #[cfg(windows)]
            {
                native_options.event_loop_builder = Some(Box::new(|builder| {
                    use winit::platform::windows::EventLoopBuilderExtWindows;
                    builder.with_any_thread(true);
                }));
            }
            let app = RegApp {
                snap: snap_clone,
                paused: paused_clone,
                step_requests: step_requests_clone,
                lst_mmap: lst_clone,
                lst_text: lst_text_clone,
                use_hex: use_hex_clone,
                auto_hex_on_edit: Arc::clone(&auto_hex_on_edit),
                scroll_to_pc_req: Arc::clone(&self.scroll_to_pc_req),
                follow_pc_flag: follow_clone,
                span_policy: span_policy_clone,
                window_len: window_len_clone,
                cmds: cmds_clone,
                ja_font: ja_font_clone,
                fonts_applied: false,
                menu_fixed: false,
                use_japanese,
                last_pc_for_scroll: 0,
                open_dialog_active: false,
                open_pending_path: None,
                open_bin_base_text: String::new(),
                #[cfg(target_os = "linux")]
                csd: std::env::var("EM6809_CSD")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false),
                #[cfg(target_os = "linux")]
                csd_maximized: false,
                #[cfg(windows)]
                about_open: false,
                #[cfg(windows)]
                about_icon_tex: None,
                #[cfg(target_os = "linux")]
                about_open: false,
                #[cfg(target_os = "linux")]
                about_icon_tex: None,
                mem_follow_pc: mem_follow_pc_thread,
                mem_window_len: mem_window_len_thread,
                mem_window_start: mem_window_start_thread,
                mem_buf: mem_buf_thread,
                mem_hl: mem_hl_thread,
                mem_utf8: mem_utf8_thread,
                mem_jump_text: format!("0x{initial_start:04X}"),
                mem_len_text: format!("{}", self.mem_window_len.load(Ordering::SeqCst)),
                quit_flag: self.quit_flag.clone(),
                mem_use_drag: false,
                mem_edit_mode: false,
                mem_pending: HashMap::new(),
                mem_selected: None,
                mem_edit_buf: String::new(),
                mem_focus_next: false,
                mem_last_sel_addr: None,
                mem_last_at_start: false,
                mem_last_at_end: false,
                edit_mode: false,
                edit_initialized: false,
                edit_a: String::new(),
                edit_b: String::new(),
                edit_d: String::new(),
                edit_x: String::new(),
                edit_y: String::new(),
                edit_u: String::new(),
                edit_s: String::new(),
                edit_dp: String::new(),
                edit_pc: String::new(),
                cc_e: false,
                cc_f: false,
                cc_h: false,
                cc_i: false,
                cc_n: false,
                cc_z: false,
                cc_v: false,
                cc_c: false,
                input_error: None,
                gpio_present: true,
                gpio_bits: 8,
                gpio_out: 0,
                gpio_dir: 0xFFFF_FFFF,
                block_present: false,
                block_base: None,
                block_backing_path: None,
                block_backing_online: false,
                block_last_cmd: 0,
                block_last_data: 0,
                block_status: 0,
                gpio_on_color: egui::Color32::LIGHT_YELLOW,
                gpio_on_color_baseline: egui::Color32::LIGHT_YELLOW,
                gpio_on_color_text: String::new(),
                gpio_sync: Arc::new(Mutex::new(None)),
                mmu_sync: Arc::new(Mutex::new(None)),
                mmu_present: false,
                mmu_sys_mode: false,
                mmu_task: 0,
                mmu_map_prev: [0u16; 16],
                mmu_map_cur: [0u16; 16],
                timer_present: false,
                timer_pending: false,
                timer_running: false,
                settings_open: false,
                settings_tab: 0,
                settings: SettingsState::default(),
                settings_baseline: SettingsState::default(),
                settings_instance: 0,
                settings_busy: false,
                settings_busy_since: None,
                settings_origins: HashMap::new(),
                console_enabled: false,
                console_base: None,
                dev_tab: DeviceTab::Console,
            };
            #[cfg(windows)]
            {
                self.started.store(true, Ordering::SeqCst);
                self.alive.store(true, Ordering::SeqCst);
                let _ = eframe::run_native(title, native_options, Box::new(|_| Box::new(app)));
                self.alive.store(false, Ordering::SeqCst);
                self.started.store(false, Ordering::SeqCst);
            }
            #[cfg(not(windows))]
            {
                use std::panic::{catch_unwind, AssertUnwindSafe};
                let prev = std::panic::take_hook();
                std::panic::set_hook(Box::new(|_| {}));
                self.started.store(true, Ordering::SeqCst);
                self.alive.store(true, Ordering::SeqCst);
                let res: Result<eframe::Result<(), eframe::Error>, _> =
                    catch_unwind(AssertUnwindSafe(|| {
                        eframe::run_native(title, native_options, Box::new(|_| Box::new(app)))
                    }));
                std::panic::set_hook(prev);
                self.alive.store(false, Ordering::SeqCst);
                self.started.store(false, Ordering::SeqCst);
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        eprintln!("[regview] GUI error: {e} — running headless");
                    }
                    Err(_) => {
                        eprintln!(
                            "[regview] GUI failed to start on this platform/thread. Running headless."
                        );
                    }
                }
            }
        }

        pub fn push(&self, s: RegSnapshot) {
            if let Ok(mut guard) = self.snapshot.lock() {
                *guard = s;
            }
        }

        pub fn request_close(&self) {
            self.quit_flag.store(true, Ordering::SeqCst);
        }

        pub fn is_closed(&self) -> bool {
            self.started.load(Ordering::SeqCst) && !self.alive.load(Ordering::SeqCst)
        }

        pub fn update_hex_listing(&self, s: String) {
            if let Some(ref t) = self.lst_text {
                if let Ok(mut g) = t.lock() {
                    *g = s;
                }
            }
        }

        pub fn update_hex_lines(&self, items: Vec<(u16, String)>) {
            if let Some(ref t) = self.lst_text {
                if let Ok(mut g) = t.lock() {
                    let mut lines: Vec<String> = g.lines().map(|s| s.to_string()).collect();
                    use std::collections::HashMap;
                    let mut map: HashMap<u16, usize> = HashMap::new();
                    for (i, l) in lines.iter().enumerate() {
                        let trimmed = l.trim_start();
                        if let Some(tok) = trimmed.split_whitespace().next() {
                            let tok = tok.trim_end_matches(':');
                            if tok.len() == 4 {
                                if let Ok(addr) = u16::from_str_radix(tok, 16) {
                                    map.insert(addr, i);
                                }
                            }
                        }
                    }
                    for (addr, text) in items.into_iter() {
                        if let Some(&idx) = map.get(&addr) {
                            lines[idx] = text;
                        }
                    }
                    *g = lines.join("\n");
                }
            }
        }

        pub fn get_span_policy(&self) -> (usize, usize) {
            (
                self.span_policy.load(Ordering::SeqCst),
                self.window_len.load(Ordering::SeqCst),
            )
        }
        pub fn force_hex_view(&self) {
            self.use_hex.store(true, Ordering::SeqCst);
            // When switching to Hex programmatically (e.g., after edits), also request centering on PC if following.
            if self.follow_pc_flag.load(Ordering::SeqCst) {
                self.scroll_to_pc_req.store(true, Ordering::SeqCst);
            }
        }
        // Memory view controls
        pub fn get_memory_window(&self) -> (u16, usize, bool) {
            (
                self.mem_window_start.load(Ordering::SeqCst) as u16,
                self.mem_window_len.load(Ordering::SeqCst),
                self.mem_follow_pc.load(Ordering::SeqCst),
            )
        }
        pub fn set_memory_window_start(&self, start: usize) {
            self.mem_window_start.store(start, Ordering::SeqCst);
        }
        pub fn update_memory_window(&self, start: u16, bytes: Vec<u8>) {
            if let Ok(mut g) = self.mem_buf.lock() {
                *g = (start, bytes);
            }
        }
        pub fn set_memory_highlight(&self, start: u16, len: usize) {
            if let Ok(mut g) = self.mem_hl.lock() {
                if len == 0 {
                    *g = None;
                } else {
                    *g = Some((start, len));
                }
            }
        }
        pub fn take_command(&self) -> Option<Command> {
            if let Ok(mut q) = self.cmds.lock() {
                if !q.is_empty() {
                    return Some(q.remove(0));
                }
            }
            None
        }

        pub fn is_paused(&self) -> bool {
            self.paused.load(Ordering::SeqCst)
        }

        pub fn set_paused(&self, v: bool) {
            self.paused.store(v, Ordering::SeqCst)
        }

        /// Returns true if the CPU loop should pause before executing the next instruction.
        /// If paused and there is at least one step request, this consumes one request and returns false.
        pub fn should_pause(&self) -> bool {
            if !self.is_paused() {
                return false;
            }
            let pending = self.step_requests.load(Ordering::SeqCst);
            if pending > 0 {
                // Consume one step request and allow one instruction to proceed
                self.step_requests.fetch_sub(1, Ordering::SeqCst);
                return false;
            }
            true
        }
    }

    pub use RegView as Viewer;

    impl RegView {
        pub fn request_load_lst(&self, path: std::path::PathBuf) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiLoadLst(path));
            }
        }
        pub fn auto_hex_on_edit(&self) -> bool {
            self.auto_hex_on_edit.load(Ordering::SeqCst)
        }
        /// Worker-side: pop only non-UI commands that must be handled by the worker.
        /// Leave UiSet* and UI-only commands (e.g., prompts, LST load) for the GUI thread.
        pub fn take_command_worker(&self) -> Option<Command> {
            if let Ok(mut q) = self.cmds.lock() {
                // Process commands in FIFO order, skipping UI-only variants
                if let Some(idx) = q.iter().position(|c| {
                    !matches!(
                        c,
                        Command::UiSetGpio(..)
                            | Command::UiSetMmu(..)
                            | Command::UiSetTimer(..)
                            | Command::UiSetBlockStatus { .. }
                            | Command::UiSetGpioOnColor(..)
                            | Command::UiOpenSettingsExternal
                            | Command::UiSetSettingsBaseline(..)
                            | Command::UiAckSettings { .. }
                            | Command::UiPromptBinBase(..)
                            | Command::UiLoadLst(..)
                    )
                }) {
                    return Some(q.remove(idx));
                }
            }
            None
        }
        pub fn sync_gpio(&self, present: bool, bits: u8, out: u32, dir: u32) {
            if let Ok(mut g) = self.gpio_sync.lock() {
                *g = Some((present, bits, out, dir));
            }
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetGpio(present, bits, out, dir));
            }
        }
        pub fn set_gpio_on_color(&self, rgb: u32) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetGpioOnColor(rgb));
            }
        }
        pub fn sync_mmu(&self, sys_mode: bool, task: u8, map: [u16; 16]) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetMmu(sys_mode, task, map));
            }
        }
        pub fn sync_timer(&self, present: bool, pending: bool, running: bool) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetTimer(present, pending, running));
            }
        }
        pub fn sync_block_status(
            &self,
            present: bool,
            base: Option<u16>,
            backing_path: Option<String>,
            backing_online: bool,
            last_cmd: u8,
            last_data: u8,
            status: u8,
        ) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetBlockStatus {
                    present,
                    base,
                    backing_path,
                    backing_online,
                    last_cmd,
                    last_data,
                    status,
                });
            }
        }
        pub fn set_settings_baseline(
            &self,
            s: SettingsState,
            origins: Vec<(String, u8)>,
            gpio_on_color: Option<u32>,
        ) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetSettingsBaseline(s, origins, gpio_on_color));
            }
        }
        pub fn init_global_cmds(&self) {
            let _ = GLOBAL_CMDS.set(self.cmds.clone());
            let _ = GLOBAL_SETTINGS.set(Arc::new(Mutex::new((
                SettingsState::default(),
                HashMap::new(),
                None,
            ))));
        }
    }

    fn parse_num_u16_any_gui(s: &str) -> Option<u16> {
        let sl = s.trim();
        let (base, digits) = if let Some(rest) = sl.strip_prefix("0x").or(sl.strip_prefix("0X")) {
            (16, rest)
        } else if let Some(rest) = sl.strip_prefix("0o").or(sl.strip_prefix("0O")) {
            (8, rest)
        } else if let Some(rest) = sl.strip_prefix("0b").or(sl.strip_prefix("0B")) {
            (2, rest)
        } else {
            (10, sl)
        };
        u32::from_str_radix(digits, base).ok().and_then(|v| {
            if v <= 0xFFFF {
                Some(v as u16)
            } else {
                None
            }
        })
    }

    fn parse_num_usize_any_gui(s: &str) -> Option<usize> {
        let sl = s.trim();
        let (base, digits) = if let Some(rest) = sl.strip_prefix("0x").or(sl.strip_prefix("0X")) {
            (16, rest)
        } else if let Some(rest) = sl.strip_prefix("0o").or(sl.strip_prefix("0O")) {
            (8, rest)
        } else if let Some(rest) = sl.strip_prefix("0b").or(sl.strip_prefix("0B")) {
            (2, rest)
        } else {
            (10, sl)
        };
        usize::from_str_radix(digits, base).ok()
    }

fn format_color_hex(c: egui::Color32) -> String {
    format!("0x{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

fn parse_color_hex(s: &str) -> Option<egui::Color32> {
        let sl = s.trim();
        let digits = if let Some(rest) = sl.strip_prefix("0x").or(sl.strip_prefix("0X")) {
            rest
        } else if let Some(rest) = sl.strip_prefix('#') {
            rest
        } else {
            sl
        };
        if digits.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&digits[0..2], 16).ok()?;
        let g = u8::from_str_radix(&digits[2..4], 16).ok()?;
        let b = u8::from_str_radix(&digits[4..6], 16).ok()?;
        Some(egui::Color32::from_rgb(r, g, b))
    }

    fn settings_origin_badges_enabled() -> bool {
        static SHOW: OnceCell<bool> = OnceCell::new();
        if let Some(flag) = SHOW_ORIGINS_OVERRIDE.get() {
            return flag.load(Ordering::SeqCst);
        }
        *SHOW.get_or_init(|| {
            let env_enabled = std::env::var("EM6809_SHOW_SETTINGS_ORIGINS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            let cli_enabled = std::env::args().any(|arg| arg == "--show-settings-origins");
            env_enabled || cli_enabled
        })
    }

    pub fn draw_origin_badge(
        ui: &mut egui::Ui,
        origins: &std::collections::HashMap<String, u8>,
        key: &str,
    ) {
        if !settings_origin_badges_enabled() {
            return;
        }
        use egui::{Color32, Rounding, Stroke, TextStyle, Vec2};
        // Resolve origin → label + background color
        let (label, bg) = match origins.get(key).copied().unwrap_or(0) {
            2 => ("CLI", Color32::from_rgb(220, 80, 60)), // red-ish
            1 => ("Saved", Color32::from_rgb(70, 130, 200)), // blue-ish
            _ => ("Default", Color32::from_gray(110)),    // gray
        };
        // Prepare galley for the text
        let text_color = Color32::WHITE;
        let padding = Vec2::new(6.0, 2.0);
        // Measure text using Small style for compact tags
        let galley = ui.painter().layout_no_wrap(
            label.to_string(),
            TextStyle::Small.resolve(ui.style()),
            text_color,
        );
        let size = galley.size() + padding * 2.0;
        // Allocate space and paint a rounded rectangle with the text inside
        let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter().rect_filled(rect, Rounding::same(6.0), bg);
        // Optional subtle border for contrast
        let border = Stroke {
            width: 1.0,
            color: bg.gamma_multiply(0.8),
        };
        ui.painter().rect_stroke(rect, Rounding::same(6.0), border);
        ui.painter()
            .galley(rect.left_top() + padding, galley, egui::Color32::WHITE);
}

fn parse_u16_any(s: &str) -> Option<u16> {
    let sl = s.trim();
    if sl.is_empty() {
        return None;
    }
    let (base, digits) = if let Some(rest) = sl.strip_prefix("0x").or(sl.strip_prefix("0X")) {
        (16, rest)
    } else if let Some(rest) = sl.strip_prefix("0o").or(sl.strip_prefix("0O")) {
        (8, rest)
    } else if let Some(rest) = sl.strip_prefix("0b").or(sl.strip_prefix("0B")) {
        (2, rest)
    } else {
        (10, sl)
    };
    u32::from_str_radix(digits, base)
        .ok()
        .and_then(|v| if v <= 0xFFFF { Some(v as u16) } else { None })
}

    fn format_memory_line(addr: u16, bytes: &[u8], _pc: u16) -> String {
        use core::fmt::Write as _;
        let mut s = String::with_capacity(80);
        let _ = write!(&mut s, "{addr:04X}: ");
        for i in 0..16 {
            if i < bytes.len() {
                let b = bytes[i];
                let _ = write!(&mut s, "{b:02X} ");
            } else {
                let _ = write!(&mut s, "   ");
            }
        }
        let _ = write!(&mut s, " | ");
        for i in 0..16 {
            if i < bytes.len() {
                let ch = bytes[i];
                let c = if (0x20..=0x7E).contains(&ch) {
                    ch as char
                } else {
                    '.'
                };
                let _ = write!(&mut s, "{c}");
            } else {
                let _ = write!(&mut s, " ");
            }
        }
        s
    }

    fn format_memory_dump((start, bytes): (u16, &[u8])) -> String {
        let mut out = String::new();
        let mut off = 0usize;
        while off < bytes.len() {
            let chunk = &bytes[off..bytes.len().min(off + 16)];
            let line = format_memory_line(start.wrapping_add(off as u16), chunk, 0xFFFF);
            out.push_str(&line);
            out.push('\n');
            off += 16;
        }
        out
    }
}

pub use gui::set_force_in_app_settings;
pub use gui::Command;
pub use gui::Viewer;
// Public helpers for external/native windows to interact with the same viewer command queue
pub fn push_global_command(cmd: Command) -> bool {
    if let Some(q) = gui::GLOBAL_CMDS.get() {
        if let Ok(mut v) = q.lock() {
            v.push(cmd);
            return true;
        }
    }
    false
}

pub fn push_global_commands_front(cmds: Vec<Command>) -> bool {
    if let Some(q) = gui::GLOBAL_CMDS.get() {
        if let Ok(mut v) = q.lock() {
            for cmd in cmds.into_iter().rev() {
                v.insert(0, cmd);
            }
            return true;
        }
    }
    false
}

pub fn get_latest_settings_baseline() -> Option<(
    SettingsState,
    std::collections::HashMap<String, u8>,
    Option<u32>,
)> {
    if let Some(g) = gui::GLOBAL_SETTINGS.get() {
        if let Ok(lock) = g.lock() {
            return Some((lock.0.clone(), lock.1.clone(), lock.2));
        }
    }
    None
}

fn push_settings_command(cmds: &Arc<Mutex<Vec<Command>>>, native: bool) {
    let _ = native; // unified: always use in-app dialog
    if let Ok(mut q) = cmds.lock() {
        q.push(Command::UiOpenSettings);
    }
}
