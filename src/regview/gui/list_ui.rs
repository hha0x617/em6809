use std::sync::atomic::Ordering;

use eframe::egui;
use i18n_embed_fl::fl;

use super::{Command, RegApp, LOADER};

pub(super) fn render_listing_panel(app: &mut RegApp, ctx: &egui::Context) {
    if app.lst_mmap.is_none() && app.lst_text.is_none() {
        return;
    }

    egui::SidePanel::right("lst_panel")
        .resizable(false)
        .min_width(400.0)
        .default_width(600.0)
        .show(ctx, |ui| {
            egui::ScrollArea::horizontal()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_min_width(400.0);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                    let have_lst = app.lst_mmap.is_some();
                    let have_hex = app.lst_text.is_some();
                    let mut use_hex_view = app.use_hex.load(Ordering::SeqCst);
                    let mut auto_hex = app.auto_hex_on_edit.load(Ordering::SeqCst);
                    let mut follow_pc = app.follow_pc_flag.load(Ordering::SeqCst);
                    let mut policy = app.span_policy.load(Ordering::SeqCst);
                    let mut win_len = app.window_len.load(Ordering::SeqCst);
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
                        if ui.checkbox(&mut follow_pc, fl!(LOADER, "follow-pc")).clicked() {
                            app.follow_pc_flag.store(follow_pc, Ordering::SeqCst);
                        }
                        if have_lst
                            && have_hex
                            && ui.checkbox(&mut use_hex_view, fl!(LOADER, "hex-view")).clicked()
                        {
                            app.use_hex.store(use_hex_view, Ordering::SeqCst);
                            if follow_pc {
                                app.scroll_to_pc_req.store(true, Ordering::SeqCst);
                            }
                            if use_hex_view {
                                if let Ok(mut q) = app.cmds.lock() {
                                    q.push(Command::RefreshHexListing);
                                }
                            }
                        }
                        if ui
                            .checkbox(&mut auto_hex, fl!(LOADER, "auto-hex-on-edit"))
                            .clicked()
                        {
                            app.auto_hex_on_edit.store(auto_hex, Ordering::SeqCst);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(fl!(LOADER, "span-mode"));
                        let full = policy == 0;
                        if ui.radio(full, fl!(LOADER, "full-span")).clicked() {
                            policy = 0;
                            app.span_policy.store(0, Ordering::SeqCst);
                        }
                        if ui.radio(!full, fl!(LOADER, "pc-window")).clicked() {
                            policy = 1;
                            app.span_policy.store(1, Ordering::SeqCst);
                        }
                        ui.add(egui::DragValue::new(&mut win_len).range(16..=4096).speed(1));
                        ui.label(fl!(LOADER, "bytes"));
                        if app.window_len.load(Ordering::SeqCst) != win_len {
                            app.window_len.store(win_len, Ordering::SeqCst);
                        }
                    });
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                            let cur_pc = app.snap.lock().map(|g| g.pc).unwrap_or(0);
                            let pc_hex = format!("{cur_pc:04X}");
                            let mut highlight_rect: Option<egui::Rect> = None;
                            let render_lines = |ui: &mut egui::Ui,
                                                text: &str,
                                                highlight_rect: &mut Option<egui::Rect>| {
                                for line in text.lines() {
                                    let ltrim = line.trim_start();
                                    let mut token = ltrim.split_whitespace().next().unwrap_or("");
                                    if token.ends_with(':') {
                                        token = &token[..token.len() - 1];
                                    }
                                    let addr_for_line: Option<u16> =
                                        if token.len() == 4 { u16::from_str_radix(token, 16).ok() } else { None };
                                    let upper = ltrim.to_ascii_uppercase();
                                    let is_macro_line = upper.contains(" MACRO")
                                        || upper.contains(" ENDM")
                                        || upper.starts_with("MACRO ");
                                    let is_pc_addr_token = addr_for_line.is_some()
                                        && token.eq_ignore_ascii_case(&pc_hex)
                                        && !is_macro_line;
                                    let (label_run_until, label_set_pc, label_cancel) = (
                                        fl!(LOADER, "run-to-here"),
                                        fl!(LOADER, "set-pc-to-here"),
                                        fl!(LOADER, "cancel"),
                                    );
                                    if is_pc_addr_token {
                                        let mut rt = egui::RichText::new(line).monospace();
                                        rt = rt
                                            .background_color(egui::Color32::from_rgb(255, 60, 60))
                                            .color(egui::Color32::BLACK);
                                        let resp = ui.add(
                                            egui::Label::new(rt).sense(egui::Sense::hover()),
                                        );
                                        if let Some(addr) = addr_for_line {
                                            resp.context_menu(|ui| {
                                                if ui.button(label_run_until).clicked() {
                                                    if let Ok(mut q) = app.cmds.lock() {
                                                        q.push(Command::RunUntil(addr));
                                                    }
                                                    ui.close_menu();
                                                }
                                                if ui.button(label_set_pc).clicked() {
                                                    if let Ok(mut q) = app.cmds.lock() {
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
                                            egui::Label::new(egui::RichText::new(line).monospace())
                                                .sense(egui::Sense::hover()),
                                        );
                                        if let Some(addr) = addr_for_line {
                                            resp.context_menu(|ui| {
                                                if ui.button(label_run_until).clicked() {
                                                    if let Ok(mut q) = app.cmds.lock() {
                                                        q.push(Command::RunUntil(addr));
                                                    }
                                                    ui.close_menu();
                                                }
                                                if ui.button(label_set_pc).clicked() {
                                                    if let Ok(mut q) = app.cmds.lock() {
                                                        q.push(Command::SetPc(addr));
                                                    }
                                                    ui.close_menu();
                                                }
                                                if ui.button(label_cancel).clicked() {
                                                    ui.close_menu();
                                                }
                                            });
                                        }
                                        if resp.hovered() {
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
                                if let Some(txt) = &app.lst_text {
                                    if let Ok(s) = txt.lock() {
                                        render_lines(ui, s.as_str(), &mut highlight_rect);
                                    }
                                }
                            } else if let Some(m) = &app.lst_mmap {
                                match std::str::from_utf8(&m[..]) {
                                    Ok(s) => render_lines(ui, s, &mut highlight_rect),
                                    Err(_) => {
                                        let cow = String::from_utf8_lossy(&m[..]);
                                        render_lines(ui, cow.as_ref(), &mut highlight_rect);
                                    }
                                }
                            }
                            let need_scroll = app.scroll_to_pc_req.load(Ordering::SeqCst)
                                || (cur_pc != app.last_pc_for_scroll);
                            if follow_pc && need_scroll {
                                if let Some(rect) = highlight_rect {
                                    ui.scroll_to_rect(rect, None);
                                    app.scroll_to_pc_req.store(false, Ordering::SeqCst);
                                    app.last_pc_for_scroll = cur_pc;
                                }
                            }
                        });
                });
        });
}
