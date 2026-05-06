use eframe::egui;
use egui_extras::{Column, TableBuilder};
use i18n_embed_fl::fl;

use super::{
    console_ui, parse_num_u16_any_gui, parse_u16_any, Command, DeviceTab, RegApp, RegSnapshot,
    LOADER,
};

pub(super) fn render_regs_panel(app: &mut RegApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        // Sized for the worst-case toolbar row when both Edit mode and
        // pause are active: Resume / Step / Step Over / Step Out / sep
        // / Apply / Cancel. ja-JP labels (ステップオーバー / 適用 / …)
        // are a touch wider than en-US, so the minimum is locale-aware:
        // pinning ja-JP at 520 leaves en-US needlessly wide and wastes
        // the dead-space the other panels can't grow into.
        // OS-aware headroom: Windows' system font lays the toolbar out
        // wider than Linux's Noto Sans / macOS' Helvetica, so the floor
        // is higher on Windows in both locales to keep the rightmost
        // button (Cancel / キャンセル) fully visible without a
        // horizontal scroll.
        let toolbar_min = match (app.use_japanese, cfg!(windows)) {
            (true, true) => 520.0,
            (true, false) => 480.0,
            (false, true) => 420.0,
            (false, false) => 340.0,
        };
        ui.set_min_width(toolbar_min);
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                let s = *app.snap.lock().unwrap();
                ui.heading(fl!(LOADER, "registers-heading"));
                ui.horizontal(|ui| {
                    if app.paused.load(std::sync::atomic::Ordering::SeqCst) {
                        if ui.button(fl!(LOADER, "resume")).clicked() {
                            app.paused.store(false, std::sync::atomic::Ordering::SeqCst);
                        }
                        if ui.button(fl!(LOADER, "step")).clicked() {
                            app.paused.store(true, std::sync::atomic::Ordering::SeqCst);
                            app.step_requests
                                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                        if ui.button(fl!(LOADER, "step-over")).clicked() {
                            app.paused.store(true, std::sync::atomic::Ordering::SeqCst);
                            app.step_over_requests
                                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                        if ui.button(fl!(LOADER, "step-out")).clicked() {
                            app.paused.store(true, std::sync::atomic::Ordering::SeqCst);
                            app.step_out_requests
                                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                    } else {
                        if ui.button(fl!(LOADER, "pause")).clicked() {
                            app.paused.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                        if ui.button(fl!(LOADER, "step")).clicked() {
                            app.paused.store(true, std::sync::atomic::Ordering::SeqCst);
                            app.step_requests
                                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                        if ui.button(fl!(LOADER, "step-over")).clicked() {
                            app.paused.store(true, std::sync::atomic::Ordering::SeqCst);
                            app.step_over_requests
                                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                        if ui.button(fl!(LOADER, "step-out")).clicked() {
                            app.paused.store(true, std::sync::atomic::Ordering::SeqCst);
                            app.step_out_requests
                                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                    ui.separator();
                    if app.edit_mode {
                        if ui.button(fl!(LOADER, "apply")).clicked() {
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
                            let _orig_a = s.a;
                            let _orig_b = s.b;
                            let orig_d = s.d;
                            let a_opt = parse8(&app.edit_a);
                            let b_opt = parse8(&app.edit_b);
                            let d_opt = parse16(&app.edit_d);
                            let x_opt = parse16(&app.edit_x);
                            let y_opt = parse16(&app.edit_y);
                            let u_opt = parse16(&app.edit_u);
                            let s_opt = parse16(&app.edit_s);
                            let dp_opt = parse8(&app.edit_dp);
                            let pc_opt = parse16(&app.edit_pc);
                            let cc_val: u8 = (if app.cc_e { 0x80 } else { 0 })
                                | (if app.cc_f { 0x40 } else { 0 })
                                | (if app.cc_h { 0x20 } else { 0 })
                                | (if app.cc_i { 0x10 } else { 0 })
                                | (if app.cc_n { 0x08 } else { 0 })
                                | (if app.cc_z { 0x04 } else { 0 })
                                | (if app.cc_v { 0x02 } else { 0 })
                                | (if app.cc_c { 0x01 } else { 0 });
                            if let (
                                Some(a_in),
                                Some(b_in),
                                Some(x),
                                Some(y),
                                Some(u),
                                Some(sp),
                                Some(dp),
                                Some(pc),
                            ) = (a_opt, b_opt, x_opt, y_opt, u_opt, s_opt, dp_opt, pc_opt)
                            {
                                let d_changed = d_opt.map(|dv| dv != orig_d).unwrap_or(false);
                                let (a, b, d) = if d_changed {
                                    let dv = d_opt.unwrap();
                                    ((dv >> 8) as u8, dv as u8, dv)
                                } else {
                                    let af = a_in;
                                    let bf = b_in;
                                    let dv = ((af as u16) << 8) | (bf as u16);
                                    (af, bf, dv)
                                };
                                let snap = RegSnapshot {
                                    a,
                                    b,
                                    d,
                                    x,
                                    y,
                                    u,
                                    s: sp,
                                    dp,
                                    pc,
                                    cc: cc_val,
                                };
                                if let Ok(mut q) = app.cmds.lock() {
                                    q.push(Command::ApplyRegs(snap));
                                }
                                app.input_error = None;
                                app.edit_mode = false;
                                app.edit_initialized = false;
                            } else {
                                app.input_error = Some(fl!(LOADER, "invalid-reg-values"));
                            }
                        }
                        if ui.button(fl!(LOADER, "cancel")).clicked() {
                            if let Ok(s2) = app.snap.lock() {
                                app.edit_a = format!("0x{:02X}", s2.a);
                                app.edit_b = format!("0x{:02X}", s2.b);
                                app.edit_d = format!("0x{:04X}", s2.d);
                                app.edit_x = format!("0x{:04X}", s2.x);
                                app.edit_y = format!("0x{:04X}", s2.y);
                                app.edit_u = format!("0x{:04X}", s2.u);
                                app.edit_s = format!("0x{:04X}", s2.s);
                                app.edit_dp = format!("0x{:02X}", s2.dp);
                                app.edit_pc = format!("0x{:04X}", s2.pc);
                                app.cc_e = (s2.cc & 0x80) != 0;
                                app.cc_f = (s2.cc & 0x40) != 0;
                                app.cc_h = (s2.cc & 0x20) != 0;
                                app.cc_i = (s2.cc & 0x10) != 0;
                                app.cc_n = (s2.cc & 0x08) != 0;
                                app.cc_z = (s2.cc & 0x04) != 0;
                                app.cc_v = (s2.cc & 0x02) != 0;
                                app.cc_c = (s2.cc & 0x01) != 0;
                            }
                            app.input_error = None;
                            app.edit_mode = false;
                            app.edit_initialized = false;
                        }
                    } else if ui.button(fl!(LOADER, "edit")).clicked() {
                        app.edit_mode = true;
                        app.edit_initialized = false;
                    }
                });
                if let Some(err) = &app.input_error {
                    ui.colored_label(egui::Color32::RED, err);
                }
                ui.separator();
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::auto().at_least(40.0))
                    // Value column was `Column::remainder()`, which made
                    // each cell stretch to fill the central panel — at
                    // 580 px wide that's an absurd amount of whitespace
                    // around `0x00` / `0x0000`. Cap at 160 px (room for
                    // the CC flag row "E F H I N Z V C" in monospace
                    // plus the 100-px TextEdit used in edit mode).
                    .column(Column::auto().at_least(160.0))
                    .header(22.0, |mut header| {
                        header.col(|ui| {
                            ui.strong(fl!(LOADER, "reg"));
                            let rect = ui.max_rect();
                            let stroke = egui::Stroke::new(
                                1.0,
                                ui.visuals().widgets.noninteractive.bg_stroke.color,
                            );
                            ui.painter().rect_stroke(
                                rect,
                                egui::Rounding::same(0),
                                stroke,
                                egui::StrokeKind::Inside,
                            );
                        });
                        header.col(|ui| {
                            ui.strong(fl!(LOADER, "value"));
                            let rect = ui.max_rect();
                            let stroke = egui::Stroke::new(
                                1.0,
                                ui.visuals().widgets.noninteractive.bg_stroke.color,
                            );
                            ui.painter().rect_stroke(
                                rect,
                                egui::Rounding::same(0),
                                stroke,
                                egui::StrokeKind::Inside,
                            );
                        });
                    })
                    .body(|mut body| {
                        let editing = app.edit_mode;
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
                                            egui::Rounding::same(0),
                                            stroke,
                                            egui::StrokeKind::Inside,
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
                                            egui::Rounding::same(0),
                                            stroke,
                                            egui::StrokeKind::Inside,
                                        );
                                    });
                                });
                            };
                        add_row("A", &mut app.edit_a, "0x00", format!("${:02X}", s.a));
                        add_row("B", &mut app.edit_b, "0x00", format!("${:02X}", s.b));
                        add_row("D", &mut app.edit_d, "0x0000", format!("${:04X}", s.d));
                        add_row("X", &mut app.edit_x, "0x0000", format!("${:04X}", s.x));
                        add_row("Y", &mut app.edit_y, "0x0000", format!("${:04X}", s.y));
                        add_row("U", &mut app.edit_u, "0x0000", format!("${:04X}", s.u));
                        add_row("S", &mut app.edit_s, "0x0000", format!("${:04X}", s.s));
                        add_row("DP", &mut app.edit_dp, "0x00", format!("${:02X}", s.dp));
                        add_row("PC", &mut app.edit_pc, "0x0000", format!("${:04X}", s.pc));

                        body.row(24.0, |mut row| {
                            row.col(|ui| {
                                ui.monospace("CC");
                                let rect = ui.max_rect();
                                let stroke = egui::Stroke::new(
                                    1.0,
                                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                                );
                                ui.painter().rect_stroke(
                                    rect,
                                    egui::Rounding::same(0),
                                    stroke,
                                    egui::StrokeKind::Inside,
                                );
                            });
                            row.col(|ui| {
                                if editing {
                                    ui.horizontal(|ui| {
                                        ui.toggle_value(&mut app.cc_e, "E");
                                        ui.toggle_value(&mut app.cc_f, "F");
                                        ui.toggle_value(&mut app.cc_h, "H");
                                        ui.toggle_value(&mut app.cc_i, "I");
                                        ui.toggle_value(&mut app.cc_n, "N");
                                        ui.toggle_value(&mut app.cc_z, "Z");
                                        ui.toggle_value(&mut app.cc_v, "V");
                                        ui.toggle_value(&mut app.cc_c, "C");
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
                                ui.painter().rect_stroke(
                                    rect,
                                    egui::Rounding::same(0),
                                    stroke,
                                    egui::StrokeKind::Inside,
                                );
                            });
                        });
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    let mut tab_button = |label: &str, tab: DeviceTab| {
                        if ui.selectable_label(app.dev_tab == tab, label).clicked() {
                            app.dev_tab = tab;
                        }
                    };
                    tab_button("Console", DeviceTab::Console);
                    tab_button("Block", DeviceTab::Block);
                    tab_button("GPIO", DeviceTab::Gpio);
                    tab_button("Timer", DeviceTab::Timer);
                    tab_button("MMU", DeviceTab::Mmu);
                });
                ui.separator();
                match app.dev_tab {
                    DeviceTab::Console => {
                        if app.console_enabled {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.monospace("Console:");
                                    if let Some(base) = app.console_base {
                                        ui.monospace(format!("@${:04X}", base));
                                    }
                                });
                                ui.horizontal(|ui| {
                                    let vt100_enabled = app.settings_baseline.console_gui;
                                    let mut vt100_visible = app.console_gui_open;
                                    let resp = ui.add_enabled(
                                        vt100_enabled,
                                        egui::Checkbox::new(&mut vt100_visible, "VT100 window"),
                                    );
                                    if resp.clicked() && vt100_enabled {
                                        app.console_gui_open = vt100_visible;
                                        console_ui::set_console_visible(
                                            ui.ctx(),
                                            vt100_visible && app.settings_baseline.console_gui,
                                        );
                                    }
                                });
                                ui.horizontal_wrapped(|ui| {
                                    let (label, color) = if app.settings_baseline.console_stdin {
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
                        if app.block_present {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.monospace("Block device:");
                                    if let Some(base) = app.block_base {
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
                                    let irq_on = app.settings_baseline.block_irq_enable;
                                    let firq_on = app.settings_baseline.block_firq;
                                    badge(ui, "IRQ", irq_on);
                                    ui.add_space(6.0);
                                    badge(ui, "FIRQ", firq_on);
                                });
                                ui.horizontal_wrapped(|ui| {
                                    let (label, color) = if app.block_backing_online {
                                        ("ONLINE", egui::Color32::from_rgb(0x4C, 0xAF, 0x50))
                                    } else if app.block_backing_path.is_some() {
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
                                    if let Some(path) = &app.block_backing_path {
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
                                        app.block_last_cmd, app.block_last_data, app.block_status
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
                        if app.gpio_present {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label("GPIO LEDs:");
                                    if let Some(base) =
                                        parse_u16_any(&app.settings_baseline.gpio_base_hex)
                                    {
                                        ui.monospace(format!("@${base:04X}"));
                                    }
                                });
                                let bits = app.gpio_bits as usize;
                                let mut new_out = app.gpio_out;
                                let rows = (bits + 7) / 8;
                                for r in 0..rows {
                                    let start = r * 8;
                                    let end = usize::min(bits, start + 8);
                                    ui.horizontal(|ui| {
                                        for i in start..end {
                                            let is_out = ((app.gpio_dir >> i) & 1) != 0;
                                            let on = ((new_out >> i) & 1) != 0;
                                            let color = if is_out {
                                                if on {
                                                    app.gpio_on_color
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
                                if new_out != app.gpio_out {
                                    app.gpio_out = new_out;
                                    if let Ok(mut q) = app.cmds.lock() {
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
                        if app.timer_present && app.settings_baseline.use_timer {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.monospace("Timer:");
                                    if let Some(base) =
                                        parse_u16_any(&app.settings_baseline.timer_base_hex)
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
                                    badge(ui, "IRQ", app.settings_baseline.timer_irq_enable);
                                    ui.add_space(6.0);
                                    badge(ui, "FIRQ", app.settings_baseline.timer_firq);
                                    ui.add_space(6.0);
                                    badge(ui, "START", app.settings_baseline.timer_start);
                                });
                                ui.horizontal(|ui| {
                                    let run_color = if app.timer_running {
                                        egui::Color32::LIGHT_GREEN
                                    } else {
                                        egui::Color32::DARK_GRAY
                                    };
                                    let (rrect, _) = ui.allocate_exact_size(
                                        egui::vec2(50.0, 18.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(rrect.shrink(3.0), 6.0, run_color);
                                    let run_label = if app.timer_running { "RUN" } else { "stop" };
                                    ui.painter().text(
                                        rrect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        run_label,
                                        egui::TextStyle::Monospace.resolve(ui.style()),
                                        egui::Color32::BLACK,
                                    );
                                    let color = if app.timer_pending {
                                        egui::Color32::LIGHT_RED
                                    } else {
                                        egui::Color32::DARK_GREEN
                                    };
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(60.0, 18.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(rect.shrink(3.0), 6.0, color);
                                    let label = if app.timer_pending { "PENDING" } else { "idle" };
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
                        if app.mmu_present {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label("MMU:");
                                    if app.mmu_sys_mode {
                                        ui.monospace("SYS");
                                    } else {
                                        ui.monospace("USR");
                                    }
                                    ui.monospace(format!(" TASK {}", app.mmu_task));
                                });
                                ui.horizontal(|ui| {
                                    ui.label("View:");
                                    ui.radio_value(&mut app.mmu_view_sys, true, "SYS");
                                    ui.radio_value(&mut app.mmu_view_sys, false, "USR");
                                });
                                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                                let (map_cur, map_prev) = if app.mmu_view_sys {
                                    (app.mmu_map_cur_sys, app.mmu_map_prev_sys)
                                } else {
                                    (app.mmu_map_cur_user, app.mmu_map_prev_user)
                                };
                                egui::Grid::new("mmu_map_grid")
                                    .min_col_width(20.0)
                                    .spacing(egui::vec2(10.0, 8.0))
                                    .show(ui, |ui| {
                                        for i in 0..16 {
                                            // map_cur packs frame in low byte, attr in high byte
                                            let frame = (map_cur[i] & 0x00FF) as u8;
                                            let mut attr = (map_cur[i] >> 8) as u8;
                                            if attr == 0 {
                                                let attr_source = if app.mmu_view_sys {
                                                    &app.settings.mmu_attr_bytes_sys
                                                } else {
                                                    &app.settings.mmu_attr_bytes_user
                                                };
                                                if let Some(v) = parse_u16_any(&attr_source[i]) {
                                                    attr = (v & 0xFF) as u8;
                                                }
                                            }
                                            let changed = map_prev[i] != map_cur[i];
                                            let wp = if (attr & 0x01) != 0 { 'W' } else { '_' };
                                            let rp = if (attr & 0x02) != 0 { 'R' } else { '_' };
                                            let nx = if (attr & 0x04) != 0 { 'X' } else { '_' };

                                            ui.monospace(format!("P{:X}", i));

                                            let mut rt = egui::RichText::new(format!(
                                                "${:02X}:{}{}{}",
                                                frame, wp, rp, nx
                                            ))
                                            .monospace()
                                            .background_color(
                                                ui.visuals().widgets.noninteractive.bg_fill,
                                            )
                                            .color(ui.visuals().strong_text_color());
                                            if changed {
                                                // Use higher-contrast color when changed
                                                rt = rt
                                                    .background_color(egui::Color32::from_rgb(
                                                        255, 210, 120,
                                                    ))
                                                    .color(egui::Color32::BLACK);
                                            }
                                            let resp = ui.add(egui::Label::new(rt));
                                            let stroke_col =
                                                ui.visuals().widgets.noninteractive.bg_stroke.color;
                                            ui.painter().rect_stroke(
                                                resp.rect.expand(2.0),
                                                egui::Rounding::same(6),
                                                egui::Stroke::new(1.0, stroke_col),
                                                egui::StrokeKind::Inside,
                                            );
                                            if (i + 1) % 4 == 0 {
                                                ui.end_row();
                                            }
                                        }
                                    });
                            });
                        } else {
                            ui.horizontal_wrapped(|ui| {
                                ui.monospace("MMU:");
                                ui.small("disabled");
                            });
                        }
                    }
                }
            });
    });
}
