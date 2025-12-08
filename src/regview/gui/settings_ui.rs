use crate::i18n::LOADER;
use crate::settings_worker::{self, SettingsPayload, WorkerCommand};
use eframe::egui;
use i18n_embed_fl::fl;
use rfd::FileDialog;

use super::{draw_origin_badge, format_color_hex, parse_color_hex, parse_u8_hex, Command, RegApp};

pub(super) fn render_settings_dialog(app: &mut RegApp, ctx: &egui::Context) {
    if !app.settings_open {
        return;
    }
    let mut close_req = false;
    egui::Window::new(fl!(LOADER, "settings-title"))
        .collapsible(false)
        .resizable(false)
        .fixed_size(egui::vec2(640.0, 400.0))
        .min_size(egui::vec2(480.0, 400.0))
        .max_size(egui::vec2(640.0, 400.0))
        .id(egui::Id::new(("settings", app.settings_instance)))
        .open(&mut app.settings_open)
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
                    let selected = app.settings_tab == idx;
                    if ui.selectable_label(selected, label).clicked() {
                        app.settings_tab = idx;
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
                .show(ui, |ui| match app.settings_tab {
                    0 => {
                        // General tab (currently empty; protect_vectors is under MMU)
                    }
                    1 => {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut app.settings.use_console,
                                    fl!(LOADER, "use-console"),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "console.enable");
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "console-base-hex"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.console_base_hex)
                                        .hint_text("0xFF00")
                                        .desired_width(160.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "console.base");
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut app.settings.console_stdin,
                                    fl!(LOADER, "console-read-stdin"),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "console.stdin");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut app.settings.console_echo,
                                    fl!(LOADER, "console-local-echo"),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "console.echo");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut app.settings.console_tee_stderr,
                                    fl!(LOADER, "console-tee-stderr"),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "console.tee_stderr");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut app.settings.console_flush_newline,
                                    fl!(LOADER, "console-flush-newline"),
                                );
                                draw_origin_badge(
                                    ui,
                                    &app.settings_origins,
                                    "console.flush_newline",
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "console-flush-every"));
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut app.settings.console_flush_every,
                                    )
                                    .hint_text("1")
                                    .desired_width(120.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "console.flush_every");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "console-output-file"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.console_out_path)
                                        .hint_text("/tmp/console.txt")
                                        .desired_width(260.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "console.out_path");
                                if ui.button(fl!(LOADER, "browse")).clicked() {
                                    if let Some(path) =
                                        FileDialog::new().set_directory(".").pick_file()
                                    {
                                        app.settings.console_out_path =
                                            path.to_string_lossy().to_string();
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "console-rx-watermark"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.console_rx_wm)
                                        .hint_text("1")
                                        .desired_width(120.0),
                                );
                                draw_origin_badge(
                                    ui,
                                    &app.settings_origins,
                                    "console.rx_watermark",
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "irq-hold"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.console_irq_hold)
                                        .hint_text("0")
                                        .desired_width(120.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "console.irq_hold");
                            });
                        });
                    }
                    2 => {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut app.settings.use_block, fl!(LOADER, "use-block"));
                                draw_origin_badge(ui, &app.settings_origins, "block.enable");
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "block-base-hex"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.block_base_hex)
                                        .hint_text("0xFF20")
                                        .desired_width(100.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "block.base");
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "block-file-path"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.block_file_path)
                                        .hint_text("/tmp/disk.img")
                                        .desired_width(260.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "block.file_path");
                                if ui.button(fl!(LOADER, "browse")).clicked() {
                                    if let Some(path) = FileDialog::new()
                                        .add_filter("Disk image", &["img", "bin", "raw"])
                                        .pick_file()
                                    {
                                        app.settings.block_file_path =
                                            path.to_string_lossy().to_string();
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut app.settings.block_irq_enable, fl!(LOADER, "irq"));
                                draw_origin_badge(ui, &app.settings_origins, "block.irq_enable");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut app.settings.block_firq, fl!(LOADER, "firq"));
                                draw_origin_badge(ui, &app.settings_origins, "block.firq");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "irq-hold"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.block_irq_hold)
                                        .hint_text("0")
                                        .desired_width(80.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "block.irq_hold");
                            });
                        });
                    }
                    3 => {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut app.settings.use_gpio, fl!(LOADER, "use-gpio"));
                                draw_origin_badge(ui, &app.settings_origins, "gpio.enable");
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "gpio-base-hex"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.gpio_base_hex)
                                        .hint_text("0xFF40")
                                        .desired_width(100.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "gpio.base");
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "gpio-bits"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.gpio_bits)
                                        .hint_text("8")
                                        .desired_width(60.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "gpio.bits");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "on-color"));
                                let te = egui::TextEdit::singleline(&mut app.gpio_on_color_text)
                                    .desired_width(100.0)
                                    .hint_text("0xRRGGBB");
                                let resp = ui.add(te);
                                if resp.changed() {
                                    if let Some(col) = parse_color_hex(&app.gpio_on_color_text) {
                                        app.gpio_on_color = col;
                                    }
                                }
                                let mut col = app.gpio_on_color;
                                if egui::widgets::color_picker::color_edit_button_srgba(
                                    ui,
                                    &mut col,
                                    egui::widgets::color_picker::Alpha::Opaque,
                                )
                                .changed()
                                {
                                    app.gpio_on_color = col;
                                    app.gpio_on_color_text = format_color_hex(col);
                                }
                                draw_origin_badge(ui, &app.settings_origins, "gpio.on_color");
                            });
                        });
                    }
                    4 => {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut app.settings.use_timer, fl!(LOADER, "use-timer"));
                                draw_origin_badge(ui, &app.settings_origins, "timer.enable");
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "timer-base-hex"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.timer_base_hex)
                                        .hint_text("0xFF10")
                                        .desired_width(100.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "timer.base");
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "timer-reload-hex"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.timer_reload_hex)
                                        .hint_text("0x03E8")
                                        .desired_width(100.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "timer.reload");
                            });
                            ui.horizontal(|ui| {
                                ui.label(fl!(LOADER, "timer-rate-hz"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.timer_rate_hz)
                                        .hint_text("6000")
                                        .desired_width(100.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "timer.rate");
                                ui.label(fl!(LOADER, "timer-ips"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.settings.timer_ips)
                                        .hint_text("1000000")
                                        .desired_width(120.0),
                                );
                                draw_origin_badge(ui, &app.settings_origins, "timer.ips");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut app.settings.timer_irq_enable, fl!(LOADER, "irq"));
                                draw_origin_badge(ui, &app.settings_origins, "timer.irq_enable");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut app.settings.timer_firq, fl!(LOADER, "firq"));
                                draw_origin_badge(ui, &app.settings_origins, "timer.firq");
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut app.settings.timer_start, fl!(LOADER, "start"));
                                draw_origin_badge(ui, &app.settings_origins, "timer.start");
                            });
                        });
                    }
                    5 => {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut app.settings.use_mmu, fl!(LOADER, "use-mmu"));
                            draw_origin_badge(ui, &app.settings_origins, "mmu.enable");
                        });
                        ui.horizontal(|ui| {
                            ui.label(fl!(LOADER, "mmu-regs-base-hex"));
                            ui.add(
                                egui::TextEdit::singleline(&mut app.settings.mmu_regs_base_hex)
                                    .hint_text("0xFFA0")
                                    .desired_width(100.0),
                            );
                            draw_origin_badge(ui, &app.settings_origins, "mmu.regs_base");
                        });
                        ui.horizontal(|ui| {
                            ui.label(fl!(LOADER, "mmu-phys-bytes"));
                            ui.add(
                                egui::TextEdit::singleline(&mut app.settings.mmu_phys_bytes)
                                    .hint_text("1048576")
                                    .desired_width(120.0),
                            );
                            draw_origin_badge(ui, &app.settings_origins, "mmu.phys_bytes");
                        });
                        ui.weak(fl!(LOADER, "mmu-restart-note"));
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut app.settings.mmu_mode_sys, fl!(LOADER, "system-mode"));
                            draw_origin_badge(ui, &app.settings_origins, "mmu.mode");
                            ui.label(fl!(LOADER, "task"));
                            ui.add(
                                egui::TextEdit::singleline(&mut app.settings.mmu_task)
                                    .desired_width(40.0)
                                    .hint_text("0"),
                            );
                            draw_origin_badge(ui, &app.settings_origins, "mmu.task");
                            ui.checkbox(&mut app.settings.mmu_log_maps, fl!(LOADER, "log-maps"));
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(
                                &mut app.settings.protect_vectors,
                                fl!(LOADER, "protect-vectors"),
                            );
                            draw_origin_badge(ui, &app.settings_origins, "mmu.protect_vectors");
                        });
                        ui.horizontal(|ui| {
                            ui.label(fl!(LOADER, "prot-hex"));
                            ui.add(
                                egui::TextEdit::singleline(&mut app.settings.mmu_prot_hex)
                                    .desired_width(70.0)
                                    .hint_text("0x03"),
                            );
                            draw_origin_badge(ui, &app.settings_origins, "mmu.prot");
                            if ui.button(fl!(LOADER, "idmap-current")).clicked() {
                                if let Ok(mut q) = app.cmds.lock() {
                                    q.push(Command::MmuIdentityMap);
                                }
                            }
                        });
                        ui.add_space(4.0);
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
                                                let h = ui
                                                    .text_style_height(&egui::TextStyle::Monospace);
                                                let resp = ui.add_sized(
                                                    (map_width, h + 6.0),
                                                    egui::TextEdit::singleline(
                                                        &mut app.settings.mmu_map_frames[p],
                                                    )
                                                    .hint_text("0x00")
                                                    .char_limit(4),
                                                );
                                                resp
                                            });
                                            if cell_resp.response.changed() {
                                                let txt = app.settings.mmu_map_frames[p].trim();
                                                if let Some(v) = parse_u8_hex(txt) {
                                                    app.settings.mmu_map_frames[p] =
                                                        format!("{:02X}", v);
                                                } else if !txt.is_empty() {
                                                    app.settings.mmu_map_frames[p].clear();
                                                }
                                            }
                                        }
                                        ui.end_row();
                                    }
                                });
                        });
                        ui.add_space(6.0);
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
                                                let h = ui
                                                    .text_style_height(&egui::TextStyle::Monospace);
                                                ui.add_sized(
                                                    (attr_width, h + 6.0),
                                                    egui::TextEdit::singleline(
                                                        &mut app.settings.mmu_attr_bytes[p],
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
                        ui.horizontal(|ui| {
                            ui.label(fl!(LOADER, "preset"));
                            let presets = em6809::config::list_presets();
                            let mut current = if !app.settings.mmu_preset_name.is_empty() {
                                app.settings.mmu_preset_name.clone()
                            } else if !app.settings_baseline.mmu_preset_name.is_empty() {
                                app.settings_baseline.mmu_preset_name.clone()
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
                                        ui.selectable_value(&mut current, name.to_string(), name);
                                    }
                                });
                            if current == "__custom__" {
                                app.settings.mmu_preset_name.clear();
                            } else if current != app.settings.mmu_preset_name {
                                app.settings.mmu_preset_name = current;
                            }
                            if ui.button(fl!(LOADER, "apply-preset")).clicked() {
                                if let Ok(mut q) = app.cmds.lock() {
                                    q.push(Command::ApplyMmuPreset {
                                        name: app.settings.mmu_preset_name.clone(),
                                    });
                                }
                            }
                            if ui.button(fl!(LOADER, "load-map-file")).clicked() {
                                if let Some(path) = FileDialog::new()
                                    .add_filter("MMU map", &["txt", "cfg", "mmu"])
                                    .pick_file()
                                {
                                    if let Ok(text) = std::fs::read_to_string(&path) {
                                        if let Ok(mut q) = app.cmds.lock() {
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
                                    &mut app.settings.debug_settings_log,
                                    fl!(LOADER, "debug-settings-log"),
                                )
                                .changed();
                            changed |= ui
                                .checkbox(
                                    &mut app.settings.debug_show_origins,
                                    fl!(LOADER, "debug-show-origins"),
                                )
                                .changed();
                            ui.separator();
                            changed |= ui
                                .checkbox(
                                    &mut app.settings.debug_console_log,
                                    fl!(LOADER, "debug-console-log"),
                                )
                                .changed();
                            changed |= ui
                                .checkbox(
                                    &mut app.settings.debug_block_log,
                                    fl!(LOADER, "debug-block-log"),
                                )
                                .changed();
                            changed |= ui
                                .checkbox(
                                    &mut app.settings.debug_gpio_log,
                                    fl!(LOADER, "debug-gpio-log"),
                                )
                                .changed();
                            changed |= ui
                                .checkbox(
                                    &mut app.settings.debug_mmu_log,
                                    fl!(LOADER, "debug-mmu-log"),
                                )
                                .changed();
                        });
                        if changed {
                            let payload = SettingsPayload {
                                settings: app.settings.clone(),
                                gpio_on_color: ((app.gpio_on_color.r() as u32) << 16)
                                    | ((app.gpio_on_color.g() as u32) << 8)
                                    | (app.gpio_on_color.b() as u32),
                                instance_id: app.settings_instance,
                            };
                            settings_worker::push_worker_command(WorkerCommand::Apply(payload));
                            app.settings_busy = true;
                            app.settings_busy_since = Some(std::time::Instant::now());
                        }
                    }
                    _ => {}
                });
            ui.separator();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(fl!(LOADER, "cancel")).clicked() {
                        close_req = true;
                    }
                    if ui.button(fl!(LOADER, "revert")).clicked() {
                        app.settings = app.settings_baseline.clone();
                        app.gpio_on_color = app.gpio_on_color_baseline;
                        app.gpio_on_color_text = format_color_hex(app.gpio_on_color);
                    }
                    let apply_clicked = ui
                        .add_enabled(!app.settings_busy, egui::Button::new(fl!(LOADER, "apply")))
                        .clicked();
                    if apply_clicked {
                        let payload = SettingsPayload {
                            settings: app.settings.clone(),
                            gpio_on_color: ((app.gpio_on_color.r() as u32) << 16)
                                | ((app.gpio_on_color.g() as u32) << 8)
                                | (app.gpio_on_color.b() as u32),
                            instance_id: app.settings_instance,
                        };
                        settings_worker::push_worker_command(WorkerCommand::SavePrefs(payload));
                        if let Ok(mut q) = app.cmds.lock() {
                            q.push(Command::SavePrefs {
                                settings: app.settings.clone(),
                                gpio_on_color: ((app.gpio_on_color.r() as u32) << 16)
                                    | ((app.gpio_on_color.g() as u32) << 8)
                                    | (app.gpio_on_color.b() as u32),
                                instance_id: app.settings_instance,
                            });
                        }
                        app.settings_busy = true;
                        app.settings_busy_since = Some(std::time::Instant::now());
                    }
                    ui.label(fl!(LOADER, "note-apply-now"));
                    if app.settings_busy {
                        ui.add(egui::Spinner::new().size(16.0));
                    }
                    ui.add_space(6.0);
                });
            });
        });
    if close_req {
        app.settings_open = false;
    }
}
