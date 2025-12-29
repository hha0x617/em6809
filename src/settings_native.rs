#![allow(dead_code, unused_imports)]

use eframe::egui;
use std::collections::HashMap;

use crate::i18n::LOADER;
use crate::regview::{
    draw_origin_badge, format_color_hex, get_latest_settings_baseline, parse_color_hex,
    push_global_command, Command as UiCommand, SettingsState,
};
use crate::settings_worker::{self, CloseReason, SettingsPayload, WorkerCommand};
#[cfg(windows)]
use em6809::platform::windows::native_settings as win_settings;
use i18n_embed_fl::fl;
use once_cell::sync::OnceCell;
use rfd::FileDialog;
use std::env;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct SettingsNativeApp {
    settings_tab: usize,
    settings: SettingsState,
    settings_baseline: SettingsState,
    settings_origins: HashMap<String, u8>,
    gpio_on_color: egui::Color32,
    gpio_on_color_baseline: egui::Color32,
    gpio_on_color_text: String,
    dialog_instance: u64,
}

#[derive(Copy, Clone, Debug)]
enum NativeDialogState {
    Hidden = 0,
    Opening = 1,
    Visible = 2,
    Closing = 3,
    Failed = 4,
}

static NATIVE_DIALOG_STATE: OnceCell<AtomicU8> = OnceCell::new();

fn native_state_cell() -> &'static AtomicU8 {
    NATIVE_DIALOG_STATE.get_or_init(|| AtomicU8::new(NativeDialogState::Hidden as u8))
}

fn set_native_dialog_state(state: NativeDialogState) {
    native_state_cell().store(state as u8, Ordering::SeqCst);
}

impl Default for SettingsNativeApp {
    fn default() -> Self {
        let mut app = SettingsNativeApp {
            settings_tab: 0,
            settings: SettingsState::default(),
            settings_baseline: SettingsState::default(),
            settings_origins: HashMap::new(),
            gpio_on_color: egui::Color32::LIGHT_YELLOW,
            gpio_on_color_baseline: egui::Color32::LIGHT_YELLOW,
            gpio_on_color_text: String::new(),
            dialog_instance: 0,
        };
        app.reload_from_baseline();
        app
    }
}

impl SettingsNativeApp {
    fn reload_from_baseline(&mut self) {
        if let Some((s, origins, color_opt)) = get_latest_settings_baseline() {
            self.settings_baseline = s.clone();
            self.settings = s;
            self.settings_origins = origins;
            if let Some(rgb) = color_opt {
                let col = egui::Color32::from_rgb(
                    ((rgb >> 16) & 0xFF) as u8,
                    ((rgb >> 8) & 0xFF) as u8,
                    (rgb & 0xFF) as u8,
                );
                self.gpio_on_color = col;
                self.gpio_on_color_baseline = col;
                self.gpio_on_color_text = format_color_hex(col);
            }
        }
        let instance = settings_worker::next_dialog_instance();
        self.dialog_instance = instance;
        settings_worker::set_active_instance(instance);
    }
}

impl SettingsNativeApp {
    fn draw(&mut self, ctx: &egui::Context) {
        // Mark that at least one frame of the native Settings viewport rendered
        #[cfg_attr(not(windows), allow(dead_code))]
        {
            NATIVE_OPENED_ONCE
                .get_or_init(|| AtomicBool::new(false))
                .store(true, Ordering::SeqCst);
            clear_open_request_instant();
        }
        set_native_dialog_state(NativeDialogState::Visible);
        #[cfg(target_os = "linux")]
        em6809::platform::linux::csd::show_settings_header(ctx, &fl!(LOADER, "settings-title"));
        if self.gpio_on_color_text.is_empty() {
            self.gpio_on_color_text = format_color_hex(self.gpio_on_color);
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(ctx.style().visuals.window_fill())
                    .inner_margin(egui::Margin::same(6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let tabs = [
                        (0, fl!(LOADER, "tab-general")),
                        (1, fl!(LOADER, "tab-console")),
                        (2, fl!(LOADER, "tab-block")),
                        (3, fl!(LOADER, "tab-gpio")),
                        (4, fl!(LOADER, "tab-timer")),
                        (5, fl!(LOADER, "tab-mmu")),
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
                let mut render_tabs = |ui: &mut egui::Ui| {
                    match self.settings_tab {
                        0 => {
                            // General tab (protect_vectors moved to MMU tab)
                        }
                        1 => {
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.settings.use_console,
                                    fl!(LOADER, "use-console"),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "console.enable");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "console-base-hex"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.console_base_hex)
                                        .hint_text("0xFF00")
                                        .desired_width(100.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "console.base");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.settings.console_stdin,
                                    fl!(LOADER, "console-read-stdin"),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "console.stdin");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.settings.console_echo,
                                    fl!(LOADER, "console-local-echo"),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "console.echo");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.settings.console_tee_stderr,
                                    fl!(LOADER, "console-tee-stderr"),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "console.tee_stderr");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.settings.console_flush_newline,
                                    fl!(LOADER, "console-flush-newline"),
                                );
                                draw_origin_badge(
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
                                    .desired_width(80.0),
                                );
                                draw_origin_badge(
                                    ui,
                                    &self.settings_origins,
                                    "console.flush_every",
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "console-output-file"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.console_out_path)
                                        .hint_text("/tmp/console.txt")
                                        .desired_width(260.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "console.out_path");
                                if ui.button(fl!(LOADER, "browse")).clicked() {
                                    if let Some(path) =
                                        FileDialog::new().set_directory(".").pick_file()
                                    {
                                        self.settings.console_out_path =
                                            path.to_string_lossy().to_string();
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "console-rx-watermark"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.console_rx_wm)
                                        .hint_text("1")
                                        .desired_width(80.0),
                                );
                                draw_origin_badge(
                                    ui,
                                    &self.settings_origins,
                                    "console.rx_watermark",
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "irq-hold"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.console_irq_hold)
                                        .hint_text("0")
                                        .desired_width(80.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "console.irq_hold");
                            });
                            ui.separator();
                            ui.label(fl!(LOADER, "console-gui-header"));
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.settings.console_gui,
                                    fl!(LOADER, "console-gui-enable"),
                                );
                                draw_origin_badge(
                                    ui,
                                    &self.settings_origins,
                                    "console.gui_enable",
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "console-gui-title"));
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut self.settings.console_gui_title,
                                    )
                                    .hint_text("em6809 Console")
                                    .desired_width(220.0),
                                );
                                draw_origin_badge(
                                    ui,
                                    &self.settings_origins,
                                    "console.gui_title",
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "console-gui-font-size"));
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut self.settings.console_gui_font_size,
                                    )
                                    .hint_text("14")
                                    .desired_width(80.0),
                                );
                                draw_origin_badge(
                                    ui,
                                    &self.settings_origins,
                                    "console.gui_font_size",
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "console-gui-cols"));
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut self.settings.console_gui_cols,
                                    )
                                    .hint_text("80")
                                    .desired_width(80.0),
                                );
                                draw_origin_badge(
                                    ui,
                                    &self.settings_origins,
                                    "console.gui_cols",
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "console-gui-rows"));
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut self.settings.console_gui_rows,
                                    )
                                    .hint_text("24")
                                    .desired_width(80.0),
                                );
                                draw_origin_badge(
                                    ui,
                                    &self.settings_origins,
                                    "console.gui_rows",
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "console-gui-scrollback"));
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut self.settings.console_gui_scrollback,
                                    )
                                    .hint_text("1000")
                                    .desired_width(80.0),
                                );
                                draw_origin_badge(
                                    ui,
                                    &self.settings_origins,
                                    "console.gui_scrollback",
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.settings.console_gui_ctrl_keys,
                                    fl!(LOADER, "console-gui-ctrl-keys"),
                                );
                                draw_origin_badge(
                                    ui,
                                    &self.settings_origins,
                                    "console.gui_ctrl_keys",
                                );
                            });
                        }
                        2 => {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.settings.use_block, fl!(LOADER, "use-block"));
                                draw_origin_badge(ui, &self.settings_origins, "block.enable");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "block-base-hex"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.block_base_hex)
                                        .hint_text("0xFF20")
                                        .desired_width(100.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "block.base");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "block-file-path"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.block_file_path)
                                        .hint_text("/tmp/disk.img")
                                        .desired_width(260.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "block.file_path");
                                if ui.button(fl!(LOADER, "browse")).clicked() {
                                    if let Some(path) = FileDialog::new()
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
                                draw_origin_badge(ui, &self.settings_origins, "block.irq_enable");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.settings.block_firq, fl!(LOADER, "firq"));
                                draw_origin_badge(ui, &self.settings_origins, "block.firq");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "irq-hold"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.block_irq_hold)
                                        .hint_text("0")
                                        .desired_width(80.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "block.irq_hold");
                            });
                        }
                        3 => {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.settings.use_gpio, fl!(LOADER, "use-gpio"));
                                draw_origin_badge(ui, &self.settings_origins, "gpio.enable");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "gpio-base-hex"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.gpio_base_hex)
                                        .hint_text("0xFF40")
                                        .desired_width(100.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "gpio.base");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "gpio-bits"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.gpio_bits)
                                        .hint_text("8")
                                        .desired_width(60.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "gpio.bits");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "on-color"));
                                let te = egui::TextEdit::singleline(&mut self.gpio_on_color_text)
                                    .desired_width(100.0)
                                    .hint_text("0xRRGGBB");
                                let resp = ui.add(te);
                                if resp.changed() {
                                    if let Some(col) = parse_color_hex(&self.gpio_on_color_text) {
                                        self.gpio_on_color = col;
                                    }
                                }
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
                                draw_origin_badge(ui, &self.settings_origins, "gpio.on_color");
                            });
                        }
                        4 => {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.settings.use_timer, fl!(LOADER, "use-timer"));
                                draw_origin_badge(ui, &self.settings_origins, "timer.enable");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "timer-base-hex"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.timer_base_hex)
                                        .hint_text("0xFF10")
                                        .desired_width(100.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "timer.base");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "timer-reload-hex"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.timer_reload_hex)
                                        .hint_text("0x03E8")
                                        .desired_width(100.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "timer.reload");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "timer-rate-hz"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.timer_rate_hz)
                                        .hint_text("6000")
                                        .desired_width(100.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "timer.rate");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "timer-ips"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.timer_ips)
                                        .hint_text("1000000")
                                        .desired_width(120.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "timer.ips");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.settings.timer_irq_enable,
                                    fl!(LOADER, "irq"),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "timer.irq_enable");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.settings.timer_firq, fl!(LOADER, "firq"));
                                draw_origin_badge(ui, &self.settings_origins, "timer.firq");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.settings.timer_start, fl!(LOADER, "start"));
                                draw_origin_badge(ui, &self.settings_origins, "timer.start");
                            });
                        }
                        5 => {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.settings.use_mmu, fl!(LOADER, "use-mmu"));
                                draw_origin_badge(ui, &self.settings_origins, "mmu.enable");
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
                                draw_origin_badge(ui, &self.settings_origins, "mmu.regs_base");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "mmu-phys-bytes"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.mmu_phys_bytes)
                                        .hint_text("1048576")
                                        .desired_width(120.0),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "mmu.phys_bytes");
                            });
                            ui.weak(fl!(LOADER, "mmu-restart-note"));
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.settings.mmu_mode_sys,
                                    fl!(LOADER, "system-mode"),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "mmu.mode");
                                ui.label(fl!(LOADER, "task"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.settings.mmu_task)
                                        .desired_width(40.0)
                                        .hint_text("0"),
                                );
                                draw_origin_badge(ui, &self.settings_origins, "mmu.task");
                                ui.checkbox(
                                    &mut self.settings.mmu_log_maps,
                                    fl!(LOADER, "log-maps"),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "mmu-edit-target"));
                                ui.radio_value(
                                    &mut self.settings.mmu_edit_sys,
                                    true,
                                    fl!(LOADER, "system-mode"),
                                );
                                ui.radio_value(
                                    &mut self.settings.mmu_edit_sys,
                                    false,
                                    fl!(LOADER, "user-mode"),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.settings.protect_vectors,
                                    fl!(LOADER, "protect-vectors"),
                                );
                                draw_origin_badge(
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
                                draw_origin_badge(ui, &self.settings_origins, "mmu.prot");
                            });
                            ui.add_space(4.0);
                            let map_frames = if self.settings.mmu_edit_sys {
                                &mut self.settings.mmu_map_frames_sys
                            } else {
                                &mut self.settings.mmu_map_frames_user
                            };
                            egui::Grid::new("mmu_map_grid_native")
                                .num_columns(4)
                                .show(ui, |ui| {
                                    for row in 0..4 {
                                        for col in 0..4 {
                                            let p = row * 4 + col;
                                            ui.monospace(format!("P{:X}", p));
                                            ui.add(
                                                egui::TextEdit::singleline(&mut map_frames[p])
                                                .desired_width(60.0)
                                                .hint_text("0x00"),
                                            );
                                        }
                                        ui.end_row();
                                    }
                                });
                            ui.add_space(6.0);
                            ui.label(fl!(LOADER, "attributes-note"));
                            let attr_bytes = if self.settings.mmu_edit_sys {
                                &mut self.settings.mmu_attr_bytes_sys
                            } else {
                                &mut self.settings.mmu_attr_bytes_user
                            };
                            egui::Grid::new("mmu_attr_grid_native")
                                .num_columns(4)
                                .show(ui, |ui| {
                                    for row in 0..4 {
                                        for col in 0..4 {
                                            let p = row * 4 + col;
                                            ui.monospace(format!("P{:X}", p));
                                            ui.add(
                                                egui::TextEdit::singleline(&mut attr_bytes[p])
                                                .desired_width(60.0)
                                                .hint_text("0x00"),
                                            );
                                        }
                                        ui.end_row();
                                    }
                                });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "preset"));
                                let presets = em6809::config::list_presets();
                                let mut current = if self.settings.mmu_preset_name.is_empty() {
                                    presets.get(0).copied().unwrap_or("").to_string()
                                } else {
                                    self.settings.mmu_preset_name.clone()
                                };
                                egui::ComboBox::from_id_salt("mmu_preset_combo_native")
                                    .width(180.0)
                                    .selected_text(current.clone())
                                    .show_ui(ui, |ui| {
                                        for &name in presets.iter() {
                                            ui.selectable_value(
                                                &mut current,
                                                name.to_string(),
                                                name,
                                            );
                                        }
                                    });
                                if current != self.settings.mmu_preset_name {
                                    self.settings.mmu_preset_name = current;
                                }
                                if ui.button(fl!(LOADER, "apply-preset")).clicked() {
                                    let _ = push_global_command(UiCommand::ApplyMmuPreset {
                                        name: self.settings.mmu_preset_name.clone(),
                                    });
                                }
                                if ui.button(fl!(LOADER, "load-map-file")).clicked() {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("MMU map", &["txt", "cfg", "mmu"])
                                        .pick_file()
                                    {
                                        if let Ok(text) = std::fs::read_to_string(&path) {
                                            let _ = push_global_command(
                                                UiCommand::ApplyMmuMapFileText { text },
                                            );
                                        }
                                    }
                                }
                            });
                        }
                        _ => {}
                    };
                };
                let available_height = ui.available_height();
                // Reserve space for the footer row with Apply/Revert/Cancel buttons
                let footer_reserved = 70.0;
                let scroll_height = (available_height - footer_reserved).max(200.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(scroll_height)
                    .show(ui, &mut render_tabs);
                ui.add_space(4.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(fl!(LOADER, "note-apply-now"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(fl!(LOADER, "cancel")).clicked() {
                            settings_debug_log("[settings_native] cancel clicked -> close");
                            close_native_viewport(ctx);
                            request_worker_close(self.dialog_instance, CloseReason::Cancel);
                            ctx.request_repaint();
                            NATIVE_REQUESTED
                                .get_or_init(|| AtomicBool::new(false))
                                .store(false, Ordering::SeqCst);
                            NATIVE_VISIBLE
                                .get_or_init(|| AtomicBool::new(false))
                                .store(false, Ordering::SeqCst);
                            clear_open_request_instant();
                            return;
                        }
                        if ui.button(fl!(LOADER, "revert")).clicked() {
                            self.settings = self.settings_baseline.clone();
                            self.gpio_on_color = self.gpio_on_color_baseline;
                            self.gpio_on_color_text = format_color_hex(self.gpio_on_color);
                        }
                        if ui.button(fl!(LOADER, "apply")).clicked() {
                            let rgb: u32 = ((self.gpio_on_color.r() as u32) << 16)
                                | ((self.gpio_on_color.g() as u32) << 8)
                                | (self.gpio_on_color.b() as u32);
                            let payload = SettingsPayload {
                                settings: self.settings.clone(),
                                gpio_on_color: rgb,
                                instance_id: self.dialog_instance,
                            };
                            settings_worker::push_worker_command(WorkerCommand::SavePrefs(payload));
                        }
                    });
                });
            });
    }
}

impl eframe::App for SettingsNativeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.draw(ctx);
    }
}

fn request_worker_close(instance_id: u64, reason: CloseReason) {
    // Stop scheduling new frames immediately; Windows keeps emitting close_requested
    // until we stop showing the viewport.
    settings_debug_log(format!(
        "[settings_native] request_worker_close instance_id={} reason={:?}",
        instance_id, reason
    ));
    NATIVE_REQUESTED
        .get_or_init(|| AtomicBool::new(false))
        .store(false, Ordering::SeqCst);
    NATIVE_VISIBLE
        .get_or_init(|| AtomicBool::new(false))
        .store(false, Ordering::SeqCst);
    set_native_dialog_state(NativeDialogState::Closing);
    NATIVE_CLOSING
        .get_or_init(|| AtomicBool::new(false))
        .store(true, Ordering::SeqCst);
    mark_close_request_now();
    settings_worker::push_worker_command(WorkerCommand::CloseNative {
        reason,
        instance_id,
    });
}

pub fn notify_native_close_from_worker(reason: CloseReason) {
    eprintln!(
        "[settings_native] worker acknowledged close: {:?} (requested={}, visible={}, closing={})",
        reason,
        NATIVE_REQUESTED
            .get_or_init(|| AtomicBool::new(false))
            .load(Ordering::SeqCst),
        NATIVE_VISIBLE
            .get_or_init(|| AtomicBool::new(false))
            .load(Ordering::SeqCst),
        NATIVE_CLOSING
            .get_or_init(|| AtomicBool::new(false))
            .load(Ordering::SeqCst)
    );
    #[cfg(windows)]
    {
        // Windows sometimes keeps the native viewport alive after the worker
        // acks the Close request. Send a direct WM_CLOSE to force teardown.
        let _ = win_settings::force_close_settings_window(&fl!(LOADER, "settings-title"));
    }
    // Mark closed so subsequent close_requested events are ignored.
    NATIVE_CLOSED
        .get_or_init(|| AtomicBool::new(false))
        .store(true, Ordering::SeqCst);
    NATIVE_CLOSING
        .get_or_init(|| AtomicBool::new(false))
        .store(false, Ordering::SeqCst);
    clear_close_request_instant();
    NATIVE_REQUESTED
        .get_or_init(|| AtomicBool::new(false))
        .store(false, Ordering::SeqCst);
    NATIVE_VISIBLE
        .get_or_init(|| AtomicBool::new(false))
        .store(false, Ordering::SeqCst);
    PENDING_SHOW_TICKS
        .get_or_init(|| AtomicU8::new(0))
        .store(0, Ordering::SeqCst);
    // Keep closing flag/timer running so watchdog can still trigger if the OS ignores Close.
    set_native_dialog_state(NativeDialogState::Hidden);
}

// Open (or focus) a native viewport driven by the same event loop (multi-viewport)
static PENDING_SHOW_TICKS: OnceCell<AtomicU8> = OnceCell::new();
static NATIVE_OPENED_ONCE: OnceCell<AtomicBool> = OnceCell::new();
static NATIVE_REQUESTED: OnceCell<AtomicBool> = OnceCell::new();
static NATIVE_VISIBLE: OnceCell<AtomicBool> = OnceCell::new();
static NATIVE_CLOSING: OnceCell<AtomicBool> = OnceCell::new();
static NATIVE_CLOSED: OnceCell<AtomicBool> = OnceCell::new();
static OPEN_REQUEST_INSTANT: OnceCell<Mutex<Option<Instant>>> = OnceCell::new();
static CLOSE_REQUEST_INSTANT: OnceCell<Mutex<Option<Instant>>> = OnceCell::new();
static APP: OnceCell<Mutex<SettingsNativeApp>> = OnceCell::new();
static SETTINGS_LOG_ENABLED: OnceCell<bool> = OnceCell::new();
static SETTINGS_LOG_OVERRIDE: OnceCell<AtomicBool> = OnceCell::new();

pub fn settings_debug_enabled() -> bool {
    if let Some(flag) = SETTINGS_LOG_OVERRIDE.get() {
        return flag.load(Ordering::SeqCst);
    }
    *SETTINGS_LOG_ENABLED.get_or_init(|| {
        env::var("EM6809_SETTINGS_DEBUG")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

pub fn set_settings_debug_log(on: bool) {
    if let Some(flag) = SETTINGS_LOG_OVERRIDE.get() {
        flag.store(on, Ordering::SeqCst);
    } else {
        let _ = SETTINGS_LOG_OVERRIDE.set(AtomicBool::new(on));
    }
}

fn settings_debug_log(msg: impl AsRef<str>) {
    if settings_debug_enabled() {
        eprintln!("{}", msg.as_ref());
    }
}

fn native_app() -> &'static Mutex<SettingsNativeApp> {
    APP.get_or_init(|| Mutex::new(SettingsNativeApp::default()))
}

fn mark_open_request_now() {
    let slot = OPEN_REQUEST_INSTANT.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(Instant::now());
    }
}

fn clear_open_request_instant() {
    if let Ok(mut guard) = OPEN_REQUEST_INSTANT.get_or_init(|| Mutex::new(None)).lock() {
        *guard = None;
    }
}

fn open_request_elapsed() -> Option<Duration> {
    OPEN_REQUEST_INSTANT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|t| t.elapsed()))
}

fn mark_close_request_now() {
    let slot = CLOSE_REQUEST_INSTANT.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        if guard.is_none() {
            *guard = Some(Instant::now());
        }
    }
}

fn clear_close_request_instant() {
    if let Ok(mut guard) = CLOSE_REQUEST_INSTANT
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *guard = None;
    }
}

fn close_request_elapsed() -> Option<Duration> {
    CLOSE_REQUEST_INSTANT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|t| t.elapsed()))
}

fn fallback_grace_duration() -> Duration {
    #[cfg(windows)]
    {
        win_settings::fallback_grace_duration()
    }
    #[cfg(not(windows))]
    {
        Duration::from_millis(0)
    }
}

fn close_watchdog_duration() -> Duration {
    #[cfg(windows)]
    {
        win_settings::close_watchdog_duration()
    }
    #[cfg(not(windows))]
    {
        Duration::from_millis(800)
    }
}

fn initial_retry_budget_ticks() -> u8 {
    #[cfg(windows)]
    {
        win_settings::retry_budget_ticks()
    }
    #[cfg(not(windows))]
    {
        3
    }
}

fn reset_native_app_from_baseline() {
    if let Ok(mut app) = native_app().lock() {
        app.reload_from_baseline();
        if app.gpio_on_color_text.is_empty() {
            app.gpio_on_color_text = format_color_hex(app.gpio_on_color);
        }
    }
}

pub fn open_native_settings(ctx: &egui::Context) {
    NATIVE_CLOSING
        .get_or_init(|| AtomicBool::new(false))
        .store(false, Ordering::SeqCst);
    NATIVE_CLOSED
        .get_or_init(|| AtomicBool::new(false))
        .store(false, Ordering::SeqCst);
    let visible = NATIVE_VISIBLE
        .get_or_init(|| AtomicBool::new(false))
        .load(Ordering::SeqCst);
    settings_debug_log(format!(
        "[settings_native] open request (visible={}, closing={})",
        visible,
        NATIVE_CLOSING
            .get_or_init(|| AtomicBool::new(false))
            .load(Ordering::SeqCst)
    ));
    if !visible {
        reset_native_app_from_baseline();
    }
    mark_open_request_now();
    set_native_dialog_state(NativeDialogState::Opening);
    open_native_settings_viewport(ctx);
    ctx.request_repaint();
}

pub fn open_native_settings_viewport(ctx: &egui::Context) {
    let app = native_app();
    let id = egui::ViewportId::from_hash_of("settings_native");
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut builder = egui::ViewportBuilder::default()
        .with_title(fl!(LOADER, "settings-title"))
        .with_inner_size(egui::vec2(640.0, 400.0))
        .with_min_inner_size(egui::vec2(480.0, 320.0))
        .with_resizable(false)
        .with_close_button(true)
        .with_minimize_button(false)
        .with_maximize_button(false);
    #[cfg(target_os = "linux")]
    {
        builder = builder.with_decorations(false);
    }
    let instance_id = app
        .lock()
        .map(|a| a.dialog_instance)
        .unwrap_or_else(|_| settings_worker::current_instance_id());
    settings_debug_log(format!(
        "[settings_native] schedule viewport show (instance_id={})",
        instance_id
    ));
    ctx.show_viewport_deferred(id, builder, |ctx, _class| {
        ctx.send_viewport_cmd(egui::ViewportCommand::EnableButtons {
            close: true,
            minimized: false,
            maximize: false,
        });
        ctx.input(|input| {
            if input.viewport().close_requested() {
                // Immediate local teardown: stop scheduling, mark closed, and
                // force-close the window handle on Windows. This avoids waiting
                // for the worker round-trip before the viewport disappears.
                NATIVE_REQUESTED
                    .get_or_init(|| AtomicBool::new(false))
                    .store(false, Ordering::SeqCst);
                NATIVE_VISIBLE
                    .get_or_init(|| AtomicBool::new(false))
                    .store(false, Ordering::SeqCst);
                NATIVE_CLOSING
                    .get_or_init(|| AtomicBool::new(false))
                    .store(false, Ordering::SeqCst);
                NATIVE_CLOSED
                    .get_or_init(|| AtomicBool::new(false))
                    .store(true, Ordering::SeqCst);
                clear_open_request_instant();
                mark_close_request_now();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                #[cfg(windows)]
                {
                    let _ =
                        win_settings::force_close_settings_window(&fl!(LOADER, "settings-title"));
                }
                if let Ok(app_guard) = app.lock() {
                    request_worker_close(app_guard.dialog_instance, CloseReason::CloseRequested);
                }
                return;
            }
        });
        if let Ok(mut a) = app.lock() {
            a.draw(ctx);
        }
    });
    // Mark request active and (re)arm the multi-frame scheduler
    let counter = PENDING_SHOW_TICKS.get_or_init(|| AtomicU8::new(0));
    counter.store(initial_retry_budget_ticks(), Ordering::SeqCst);
    NATIVE_REQUESTED
        .get_or_init(|| AtomicBool::new(false))
        .store(true, Ordering::SeqCst);
    // Reset the opened flag for this attempt
    NATIVE_OPENED_ONCE
        .get_or_init(|| AtomicBool::new(false))
        .store(false, Ordering::SeqCst);
    NATIVE_VISIBLE
        .get_or_init(|| AtomicBool::new(false))
        .store(true, Ordering::SeqCst);
    ctx.request_repaint();
}

pub fn pump_native_settings_viewport(ctx: &egui::Context) {
    let id = egui::ViewportId::from_hash_of("settings_native");
    // If we already marked the native dialog as closed, keep telling the OS to
    // close the viewport and stop scheduling any further work for it.
    let already_closed = NATIVE_CLOSED
        .get_or_init(|| AtomicBool::new(false))
        .load(Ordering::SeqCst);
    if already_closed {
        ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Close);
        return;
    }
    #[cfg(windows)]
    {
        let req = NATIVE_REQUESTED
            .get_or_init(|| AtomicBool::new(false))
            .load(Ordering::SeqCst);
        let vis = NATIVE_VISIBLE
            .get_or_init(|| AtomicBool::new(false))
            .load(Ordering::SeqCst);
        let closing = NATIVE_CLOSING
            .get_or_init(|| AtomicBool::new(false))
            .load(Ordering::SeqCst);
        if closing {
            eprintln!(
                "[settings_native] pump (requested={}, visible={}, closing={}, elapsed_close={:?})",
                req,
                vis,
                closing,
                close_request_elapsed()
            );
        }
    }
    let counter = PENDING_SHOW_TICKS.get_or_init(|| AtomicU8::new(0));
    let requested = NATIVE_REQUESTED
        .get_or_init(|| AtomicBool::new(false))
        .load(Ordering::SeqCst);
    let visible = NATIVE_VISIBLE
        .get_or_init(|| AtomicBool::new(false))
        .load(Ordering::SeqCst);
    let closing = NATIVE_CLOSING
        .get_or_init(|| AtomicBool::new(false))
        .load(Ordering::SeqCst);
    if closing {
        // Keep nudging the OS to close until worker ack clears the flag.
        ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Close);
        ctx.request_repaint();
        // Do not attempt to render the viewport while closing to avoid blank frames/hangs.
        let elapsed = close_request_elapsed().unwrap_or_else(|| {
            // If not set (shouldn't happen), start now.
            mark_close_request_now();
            Duration::from_millis(0)
        });
        settings_debug_log(format!(
            "[settings_native] closing tick elapsed={:?} watchdog={:?}",
            elapsed,
            close_watchdog_duration()
        ));
        #[cfg(windows)]
        {
            if elapsed > Duration::from_millis(200) {
                // Last resort: send WM_CLOSE directly in case egui/winit close is ignored.
                let _ = win_settings::force_close_settings_window(&fl!(LOADER, "settings-title"));
            }
        }
        if elapsed > close_watchdog_duration() {
            settings_debug_log(format!(
                "[settings_native] close watchdog fired after {:?}, forcing fallback",
                elapsed
            ));
            // Stop requesting/rescheduling the native viewport and immediately fall back.
            NATIVE_REQUESTED
                .get_or_init(|| AtomicBool::new(false))
                .store(false, Ordering::SeqCst);
            NATIVE_VISIBLE
                .get_or_init(|| AtomicBool::new(false))
                .store(false, Ordering::SeqCst);
            NATIVE_CLOSED
                .get_or_init(|| AtomicBool::new(false))
                .store(true, Ordering::SeqCst);
            NATIVE_CLOSING
                .get_or_init(|| AtomicBool::new(false))
                .store(false, Ordering::SeqCst);
            PENDING_SHOW_TICKS
                .get_or_init(|| AtomicU8::new(0))
                .store(0, Ordering::SeqCst);
            clear_close_request_instant();
            clear_open_request_instant();
            set_native_dialog_state(NativeDialogState::Hidden);
            let _ = push_global_command(UiCommand::UiOpenSettings);
        }
        return;
    }
    let ticks = counter.load(Ordering::SeqCst);
    if requested || visible || closing {
        let app = APP.get_or_init(|| Mutex::new(SettingsNativeApp::default()));
        #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
        let mut builder = egui::ViewportBuilder::default()
            .with_title(fl!(LOADER, "settings-title"))
            .with_inner_size(egui::vec2(640.0, 400.0))
            .with_min_inner_size(egui::vec2(480.0, 320.0))
            .with_resizable(false)
            .with_close_button(true)
            .with_minimize_button(false)
            .with_maximize_button(false);
        #[cfg(target_os = "linux")]
        {
            builder = builder.with_decorations(false);
        }
        let closing_now = closing;
        ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
            ctx.send_viewport_cmd(egui::ViewportCommand::EnableButtons {
                close: true,
                minimized: false,
                maximize: false,
            });
            if closing_now {
                eprintln!("[settings_native] closing -> send Close");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            // Handle OS close [X]: stop re-showing on next frames
            if ctx.input(|i| i.viewport().close_requested()) {
                // If we are already in Closing, don't enqueue another request; just keep nudging Close.
                let closing_flag = NATIVE_CLOSING
                    .get_or_init(|| AtomicBool::new(false))
                    .load(Ordering::SeqCst);
                let closed_flag = NATIVE_CLOSED
                    .get_or_init(|| AtomicBool::new(false))
                    .load(Ordering::SeqCst);
                if closing_flag || closed_flag {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                let instance_id = app
                    .lock()
                    .map(|app| app.dialog_instance)
                    .unwrap_or_else(|_| settings_worker::current_instance_id());
                settings_debug_log(format!(
                    "[settings_native] close_requested from OS (instance_id={})",
                    instance_id
                ));
                close_native_viewport(ctx);
                request_worker_close(instance_id, CloseReason::CloseRequested);
                // Windows: sometimes the worker loop does not get a chance to drain CloseNative
                // before the native viewport stalls. Force a local ack so state is reset.
                #[cfg(windows)]
                {
                    notify_native_close_from_worker(CloseReason::CloseRequested);
                }
                ctx.request_repaint();
                NATIVE_REQUESTED
                    .get_or_init(|| AtomicBool::new(false))
                    .store(false, Ordering::SeqCst);
                NATIVE_VISIBLE
                    .get_or_init(|| AtomicBool::new(false))
                    .store(false, Ordering::SeqCst);
                clear_open_request_instant();
                return;
            }
            // macOS: disable minimize/zoom buttons while keeping Close [X]
            #[cfg(target_os = "macos")]
            {
                em6809::platform::macos::disable_minimize_zoom_for_title(&fl!(
                    LOADER,
                    "settings-title"
                ));
            }
            if let Ok(mut a) = app.lock() {
                // Keep drawing even while closing so the viewport does not go blank on Windows.
                a.draw(ctx);
            }
        });
        if ticks > 0 {
            counter.store(ticks - 1, Ordering::SeqCst);
        }
        ctx.request_repaint();
    }
    if requested && ticks == 0 {
        // Fallback: if viewport never rendered after scheduling, open in-app Settings
        let opened = NATIVE_OPENED_ONCE
            .get_or_init(|| AtomicBool::new(false))
            .load(Ordering::SeqCst);
        if !opened {
            let elapsed = open_request_elapsed();
            if let Some(elapsed) = elapsed {
                if elapsed < fallback_grace_duration() {
                    eprintln!(
                        "[settings_native] waiting for native viewport (elapsed={:?}, grace={:?})",
                        elapsed,
                        fallback_grace_duration()
                    );
                    // Keep waiting within grace period (Windows may delay first frame).
                    counter.store(1, Ordering::SeqCst);
                    ctx.request_repaint();
                    return;
                }
            }
            set_native_dialog_state(NativeDialogState::Failed);
            eprintln!(
                "[settings_native] native viewport failed to present (elapsed={:?}); falling back to in-app",
                elapsed
            );
            let _ = push_global_command(UiCommand::UiOpenSettings);
            // Clear request so we don't repeatedly re-open
            NATIVE_REQUESTED
                .get_or_init(|| AtomicBool::new(false))
                .store(false, Ordering::SeqCst);
            NATIVE_VISIBLE
                .get_or_init(|| AtomicBool::new(false))
                .store(false, Ordering::SeqCst);
            clear_open_request_instant();
            set_native_dialog_state(NativeDialogState::Hidden);
            ctx.request_repaint();
        } else {
            // Success path: stop scheduling retries; keep visible to continue drawing
            NATIVE_REQUESTED
                .get_or_init(|| AtomicBool::new(false))
                .store(false, Ordering::SeqCst);
            NATIVE_VISIBLE
                .get_or_init(|| AtomicBool::new(false))
                .store(true, Ordering::SeqCst);
            clear_open_request_instant();
            settings_debug_log(format!(
                "[settings_native] native viewport opened (ticks consumed={})",
                initial_retry_budget_ticks() - ticks
            ));
        }
    }
}

fn close_native_viewport(ctx: &egui::Context) {
    let id = egui::ViewportId::from_hash_of("settings_native");
    ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Close);
}
