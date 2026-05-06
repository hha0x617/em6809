use eframe::egui;
use i18n_embed_fl::fl;

use super::{
    format_memory_dump, parse_num_u16_any_gui, parse_num_usize_any_gui, Command, RegApp, LOADER,
};

// `Panel::show(ctx, ...)` is deprecated; see `regs_ui::render_regs_panel`
// for the deferred-migration note.
#[allow(deprecated)]
pub(super) fn render_memory_panel(app: &mut RegApp, ctx: &egui::Context) {
    egui::Panel::right("mem_panel")
        .resizable(false)
        .min_size(440.0)
        .default_size(440.0)
        .show(ctx, |ui| {
            let mut follow_pc = app.mem_follow_pc.load(std::sync::atomic::Ordering::SeqCst);
            let _win_len = app.mem_window_len.load(std::sync::atomic::Ordering::SeqCst);
            let mut start = app
                .mem_window_start
                .load(std::sync::atomic::Ordering::SeqCst) as u16;

            fn stage_cell(app: &mut crate::regview::gui::RegApp, sel_addr: u16) {
                if let Ok(v) = u8::from_str_radix(app.mem_edit_buf.trim(), 16) {
                    let cur_val = app
                        .mem_pending
                        .get(&sel_addr)
                        .copied()
                        .or_else(|| {
                            app.mem_buf.lock().ok().and_then(|g| {
                                let base = app
                                    .mem_window_start
                                    .load(std::sync::atomic::Ordering::SeqCst)
                                    as u16;
                                let offset = sel_addr.wrapping_sub(base) as usize;
                                g.1.get(offset).copied()
                            })
                        })
                        .unwrap_or(v);
                    if v != cur_val {
                        app.mem_pending.insert(sel_addr, v);
                    }
                }
            }
            ui.horizontal(|ui| {
                ui.heading(fl!(LOADER, "memory"));
                ui.separator();
                if ui
                    .checkbox(&mut follow_pc, fl!(LOADER, "follow-pc"))
                    .clicked()
                {
                    app.mem_follow_pc
                        .store(follow_pc, std::sync::atomic::Ordering::SeqCst);
                }
                if follow_pc {
                    app.mem_jump_text = format!("0x{start:04X}");
                }
                let mut utf8 = app.mem_utf8.load(std::sync::atomic::Ordering::SeqCst);
                if ui.checkbox(&mut utf8, "UTF-8").clicked() {
                    app.mem_utf8
                        .store(utf8, std::sync::atomic::Ordering::SeqCst);
                }
                let mut use_drag = app.mem_use_drag;
                if ui.checkbox(&mut use_drag, fl!(LOADER, "numeric")).clicked() {
                    app.mem_use_drag = use_drag;
                }
                ui.separator();
                let pending_count = app.mem_pending.len();
                if !app.mem_edit_mode {
                    if ui.button(fl!(LOADER, "edit")).clicked() {
                        app.mem_edit_mode = true;
                        app.mem_selected = None;
                        app.mem_edit_buf.clear();
                    }
                } else {
                    let apply_enabled = pending_count > 0 || app.mem_selected.is_some();
                    if ui
                        .add_enabled(apply_enabled, egui::Button::new(fl!(LOADER, "apply")))
                        .clicked()
                    {
                        // If a cell is currently being edited but not yet staged, try to stage it.
                        if let Some(sel) = app.mem_selected {
                            stage_cell(app, sel);
                        }
                        let mut items: Vec<(u16, u8)> =
                            app.mem_pending.iter().map(|(a, v)| (*a, *v)).collect();
                        items.sort_by_key(|(a, _)| *a);
                        if let Ok(mut q) = app.cmds.lock() {
                            q.push(Command::MemWrite(items));
                            // Always refresh listing after memory edits so hex view stays in sync.
                            q.push(Command::RefreshHexListing);
                        }
                        app.mem_pending.clear();
                        app.mem_selected = None;
                        app.mem_edit_buf.clear();
                        app.mem_edit_mode = false;
                        app.mem_last_sel_addr = None;
                        app.mem_last_at_start = false;
                        app.mem_last_at_end = false;
                    }
                    if ui.button(fl!(LOADER, "cancel")).clicked() {
                        app.mem_pending.clear();
                        app.mem_selected = None;
                        app.mem_edit_buf.clear();
                        app.mem_edit_mode = false;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(fl!(LOADER, "start"));
                if app.mem_use_drag {
                    let mut sv: u16 = app
                        .mem_window_start
                        .load(std::sync::atomic::Ordering::SeqCst)
                        as u16;
                    let resp = ui.add_enabled(
                        !follow_pc,
                        egui::DragValue::new(&mut sv)
                            .range(0u16..=0xFFFF)
                            .speed(16.0),
                    );
                    ui.monospace(format!(" (0x{sv:04X})"));
                    if resp.changed() && !follow_pc {
                        start = sv;
                        app.mem_window_start
                            .store(sv as usize, std::sync::atomic::Ordering::SeqCst);
                        app.mem_jump_text = format!("0x{sv:04X}");
                    }
                } else {
                    let start_id = egui::Id::new("mem_start_edit");
                    let start_has_focus = ctx.memory(|m| m.has_focus(start_id));
                    if !follow_pc && !start_has_focus {
                        app.mem_jump_text = format!("0x{start:04X}");
                    }
                    let te = egui::TextEdit::singleline(&mut app.mem_jump_text)
                        .desired_width(80.0)
                        .hint_text("0x0000")
                        .id(start_id);
                    let start_resp = ui.add_enabled(!follow_pc, te);
                    if !follow_pc {
                        if start_resp.has_focus() {
                            if ui.input_mut(|i| {
                                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                            }) {
                                if let Some(val) = parse_num_u16_any_gui(&app.mem_jump_text) {
                                    start = val;
                                    app.mem_window_start
                                        .store(val as usize, std::sync::atomic::Ordering::SeqCst);
                                    app.mem_jump_text = format!("0x{val:04X}");
                                }
                            }
                        } else if start_resp.lost_focus() {
                            if let Some(val) = parse_num_u16_any_gui(&app.mem_jump_text) {
                                start = val;
                                app.mem_window_start
                                    .store(val as usize, std::sync::atomic::Ordering::SeqCst);
                                app.mem_jump_text = format!("0x{val:04X}");
                            }
                        }
                    }
                }
                ui.label(fl!(LOADER, "len"));
                if app.mem_use_drag {
                    let mut lv: usize =
                        app.mem_window_len.load(std::sync::atomic::Ordering::SeqCst);
                    let old = lv;
                    let resp = ui.add(egui::DragValue::new(&mut lv).range(16..=4096).speed(1.0));
                    ui.label(fl!(LOADER, "bytes"));
                    if resp.changed() && lv != old {
                        let v = lv.clamp(16, 4096);
                        app.mem_window_len
                            .store(v, std::sync::atomic::Ordering::SeqCst);
                        app.mem_len_text = format!("{v}");
                    }
                } else {
                    let len_te = egui::TextEdit::singleline(&mut app.mem_len_text)
                        .desired_width(64.0)
                        .hint_text("256");
                    let len_resp = ui.add(len_te);
                    if len_resp.has_focus() {
                        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
                        {
                            if let Some(v) = parse_num_usize_any_gui(&app.mem_len_text) {
                                let v = v.clamp(16, 4096);
                                app.mem_window_len
                                    .store(v, std::sync::atomic::Ordering::SeqCst);
                                app.mem_len_text = format!("{v}");
                            }
                        }
                    } else if len_resp.lost_focus() {
                        if let Some(v) = parse_num_usize_any_gui(&app.mem_len_text) {
                            let v = v.clamp(16, 4096);
                            app.mem_window_len
                                .store(v, std::sync::atomic::Ordering::SeqCst);
                            app.mem_len_text = format!("{v}");
                        }
                    }
                }
                if ui.button(fl!(LOADER, "copy")).clicked() {
                    let dump = if let Ok(g) = app.mem_buf.lock() {
                        format_memory_dump((g.0, g.1.as_slice()))
                    } else {
                        String::new()
                    };
                    ui.ctx().copy_text(dump);
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let (s, bytes) = if let Ok(g) = app.mem_buf.lock() {
                        (g.0, g.1.clone())
                    } else {
                        (0u16, Vec::new())
                    };
                    let hl = app.mem_hl.lock().ok().and_then(|g| *g);
                    let (hl_start, hl_end) = if let Some((hs, hl)) = hl {
                        let end = hs.wrapping_add(hl as u16);
                        (Some(hs), Some(end))
                    } else {
                        (None, None)
                    };
                    let _hl_color = egui::Color32::RED;
                    let _utf8_mode = app.mem_utf8.load(std::sync::atomic::Ordering::SeqCst);
                    let mut off: usize = 0;
                    while off < bytes.len() {
                        let chunk = &bytes[off..bytes.len().min(off + 16)];
                        let addr = s.wrapping_add(off as u16);
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.horizontal(|ui| {
                                ui.monospace(format!("{addr:04X}: "));
                                for (i, b) in chunk.iter().enumerate() {
                                    let a = addr.wrapping_add(i as u16);
                                    let in_hl = if let (Some(hs), Some(he)) = (hl_start, hl_end) {
                                        if hs <= he {
                                            a >= hs && a < he
                                        } else {
                                            a >= hs || a < he
                                        }
                                    } else {
                                        false
                                    };
                                    let shown = if let Some(v) = app.mem_pending.get(&a) {
                                        *v
                                    } else {
                                        *b
                                    };
                                    if app.mem_edit_mode && app.mem_selected == Some(a) {
                                        if app.mem_edit_buf.is_empty() {
                                            app.mem_edit_buf = format!("{shown:02X}");
                                        }
                                        let cell_id = egui::Id::new(("mem_hex_cell", a));
                                        let te = egui::TextEdit::singleline(&mut app.mem_edit_buf)
                                            .desired_width(24.0)
                                            .id_source(cell_id)
                                            .cursor_at_end(true);
                                        let resp = ui.add(te);
                                        if app.mem_focus_next {
                                            resp.request_focus();
                                            app.mem_focus_next = false;
                                        }
                                        if resp.gained_focus() {
                                            if let Some(mut state) =
                                                egui::text_edit::TextEditState::load(
                                                    ui.ctx(),
                                                    resp.id,
                                                )
                                            {
                                                let len = app.mem_edit_buf.len();
                                                let range = egui::text::CCursorRange::two(
                                                    egui::text::CCursor::new(0),
                                                    egui::text::CCursor::new(len),
                                                );
                                                state.cursor.set_char_range(Some(range));
                                                state.store(ui.ctx(), resp.id);
                                            }
                                        }
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
                                            let tab_plain_c = i
                                                .consume_key(egui::Modifiers::NONE, egui::Key::Tab);
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
                                        let (at_start_after, at_end_after) = {
                                            if let Some(state) =
                                                egui::text_edit::TextEditState::load(
                                                    ui.ctx(),
                                                    resp.id,
                                                )
                                            {
                                                if let Some(r) = state.cursor.char_range() {
                                                    let len = app.mem_edit_buf.len();
                                                    let collapsed = r.primary == r.secondary;
                                                    let idx = r.primary.index;
                                                    (collapsed && idx == 0, collapsed && idx == len)
                                                } else {
                                                    (false, false)
                                                }
                                            } else {
                                                (false, false)
                                            }
                                        };
                                        let now_at_start = at_start_after;
                                        let now_at_end = at_end_after;
                                        let same_cell = app.mem_last_sel_addr == Some(a);
                                        let move_prev = same_cell
                                            && left_pressed
                                            && app.mem_last_at_start
                                            && now_at_start;
                                        let move_next = same_cell
                                            && right_pressed
                                            && app.mem_last_at_end
                                            && now_at_end;
                                        let caret_home = home_key;
                                        let caret_end = end_key;
                                        app.mem_last_sel_addr = Some(a);
                                        app.mem_last_at_start = now_at_start;
                                        app.mem_last_at_end = now_at_end;
                                        let enter_commit = enter;
                                        let explicit_nav_prev =
                                            move_prev || shift_tab || caret_home;
                                        let explicit_nav_next = move_next || tab_plain || caret_end;
                                        let _maybe_stage_current = |sel_addr: u16| {
                                            if let Ok(v) =
                                                u8::from_str_radix(app.mem_edit_buf.trim(), 16)
                                            {
                                                let cur_val = app
                                                    .mem_pending
                                                    .get(&sel_addr)
                                                    .copied()
                                                    .or_else(|| {
                                                        app.mem_buf.lock().ok().and_then(|g| {
                                                            let base = app.mem_window_start.load(
                                                                std::sync::atomic::Ordering::SeqCst,
                                                            )
                                                                as u16;
                                                            let offset = sel_addr.wrapping_sub(base)
                                                                as usize;
                                                            g.1.get(offset).copied()
                                                        })
                                                    })
                                                    .unwrap_or(v);
                                                if v != cur_val {
                                                    app.mem_pending.insert(sel_addr, v);
                                                }
                                            }
                                        };
                                        if enter_commit {
                                            stage_cell(app, a);
                                            // Commit immediately when Enter is pressed
                                            let mut items: Vec<(u16, u8)> = app
                                                .mem_pending
                                                .iter()
                                                .map(|(addr, val)| (*addr, *val))
                                                .collect();
                                            items.sort_by_key(|(addr, _)| *addr);
                                            if let Ok(mut q) = app.cmds.lock() {
                                                q.push(Command::MemWrite(items));
                                                if app
                                                    .use_hex
                                                    .load(std::sync::atomic::Ordering::SeqCst)
                                                {
                                                    q.push(Command::RefreshHexListing);
                                                }
                                            }
                                            app.mem_pending.clear();
                                            app.mem_selected = None;
                                            app.mem_edit_buf.clear();
                                            app.mem_edit_mode = false;
                                            app.mem_last_sel_addr = None;
                                            app.mem_last_at_start = false;
                                            app.mem_last_at_end = false;
                                            app.mem_focus_next = false;
                                        }
                                        if explicit_nav_prev {
                                            stage_cell(app, a);
                                            let prev = a.wrapping_sub(1);
                                            app.mem_selected = Some(prev);
                                            app.mem_edit_buf.clear();
                                            app.mem_focus_next = true;
                                        } else if explicit_nav_next {
                                            stage_cell(app, a);
                                            let next = a.wrapping_add(1);
                                            app.mem_selected = Some(next);
                                            app.mem_edit_buf.clear();
                                            app.mem_focus_next = true;
                                        } else if arrow_up {
                                            stage_cell(app, a);
                                            let prev_row = a.wrapping_sub(16);
                                            app.mem_selected = Some(prev_row);
                                            app.mem_edit_buf.clear();
                                            app.mem_focus_next = true;
                                        } else if arrow_down {
                                            stage_cell(app, a);
                                            let next_row = a.wrapping_add(16);
                                            app.mem_selected = Some(next_row);
                                            app.mem_edit_buf.clear();
                                            app.mem_focus_next = true;
                                        } else if resp.lost_focus() && resp.hovered() == false {
                                            // Lost focus by clicking outside: stage current edit
                                            stage_cell(app, a);
                                        }
                                    } else {
                                        let mut label =
                                            egui::RichText::new(format!("{:02X}", shown));
                                        if let Some(&pending_val) = app.mem_pending.get(&a) {
                                            label =
                                                egui::RichText::new(format!("{:02X}", pending_val))
                                                    .background_color(egui::Color32::from_rgb(
                                                        255, 235, 170,
                                                    ))
                                                    .color(egui::Color32::BLACK);
                                        }
                                        if app.mem_edit_mode && app.mem_selected == Some(a) {
                                            label = label
                                                .background_color(egui::Color32::from_rgb(
                                                    255, 220, 160,
                                                ))
                                                .color(egui::Color32::BLACK);
                                        }
                                        let resp = ui.add(
                                            egui::Label::new(label).sense(egui::Sense::click()),
                                        );
                                        if in_hl {
                                            // keep highlight within line height (exclude spacing below)
                                            let mut rect = resp.rect;
                                            let line_height = resp.rect.height();
                                            rect.max.y = rect.min.y + line_height * 0.75;
                                            rect.min.y -= 1.0;
                                            ui.painter().rect_filled(
                                                rect,
                                                egui::CornerRadius::same(0),
                                                egui::Color32::from_rgb(255, 60, 60),
                                            );
                                            ui.painter().text(
                                                rect.left_top(),
                                                egui::Align2::LEFT_TOP,
                                                format!("{:02X}", shown),
                                                egui::FontId::monospace(14.0),
                                                egui::Color32::BLACK,
                                            );
                                        }
                                        if app.mem_edit_mode && resp.clicked() {
                                            app.mem_selected = Some(a);
                                            app.mem_edit_buf.clear();
                                            app.mem_focus_next = true;
                                        }
                                        ui.add_space(4.0);
                                    }
                                }
                                ui.add_space(8.0);
                                let _ascii_start = ui.cursor().min;
                                ui.monospace("|");
                                ui.add_space(4.0);
                                // ASCII representation, with per-character highlight for the address range
                                let mut ascii = String::with_capacity(16);
                                for i in 0..16 {
                                    if i < chunk.len() {
                                        let b = chunk[i];
                                        let c = if (0x20..=0x7E).contains(&b) {
                                            b as char
                                        } else {
                                            '.'
                                        };
                                        ascii.push(c);
                                    } else {
                                        ascii.push(' ');
                                    }
                                }
                                let ascii_resp = ui.monospace(ascii.clone());
                                if let (Some(hs), Some(he)) = (hl_start, hl_end) {
                                    // Draw background per character for the highlighted range
                                    let char_width = 8.0;
                                    let rect = ascii_resp.rect;
                                    let top = rect.top();
                                    let bottom = rect.bottom();
                                    for i in 0..16 {
                                        let a = addr.wrapping_add(i as u16);
                                        let in_hl = if hs <= he {
                                            a >= hs && a < he
                                        } else {
                                            a >= hs || a < he
                                        };
                                        if in_hl {
                                            let left = rect.left() + char_width * (i as f32);
                                            let r = egui::Rect::from_min_max(
                                                egui::pos2(left, top),
                                                egui::pos2(left + char_width, bottom),
                                            );
                                            ui.painter().rect_filled(
                                                r,
                                                egui::CornerRadius::same(0),
                                                egui::Color32::from_rgb(255, 60, 60),
                                            );
                                            // Re-draw character in black for contrast
                                            let ch = ascii.chars().nth(i).unwrap_or(' ');
                                            ui.painter().text(
                                                r.left_top(),
                                                egui::Align2::LEFT_TOP,
                                                ch,
                                                egui::FontId::monospace(14.0),
                                                egui::Color32::BLACK,
                                            );
                                        }
                                    }
                                }
                            });
                        });
                        off += 16;
                    }
                });
        });
}
