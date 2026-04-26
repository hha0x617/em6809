use std::sync::atomic::Ordering;

use eframe::egui;
use i18n_embed_fl::fl;

use super::{BpUiState, Command, RegApp, LOADER};

/// Look up `addr` in the BP-state snapshot. The slice is small in
/// practice (a handful of breakpoints), so a linear scan is cheaper
/// than building a HashMap each frame.
fn bp_state_for(states: &[(u16, BpUiState)], addr: u16) -> Option<BpUiState> {
    for (a, s) in states {
        if *a == addr {
            return Some(*s);
        }
    }
    None
}

/// Single character drawn at the start of each row to convey BP state.
/// `●` means an enabled breakpoint sits here, `○` means a disabled one,
/// and a space keeps unmarked rows column-aligned with marked ones.
fn bp_marker_glyph(s: Option<BpUiState>) -> &'static str {
    match s {
        Some(BpUiState::Enabled) => "●",
        Some(BpUiState::Disabled) => "○",
        None => " ",
    }
}

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
                        if ui
                            .checkbox(&mut follow_pc, fl!(LOADER, "follow-pc"))
                            .clicked()
                        {
                            app.follow_pc_flag.store(follow_pc, Ordering::SeqCst);
                            // Toggling on should immediately re-center on the
                            // current PC even if it hasn't changed since the
                            // last paint — the user just expressed intent.
                            if follow_pc {
                                app.scroll_to_pc_req.store(true, Ordering::SeqCst);
                            }
                        }
                        if have_lst
                            && have_hex
                            && ui
                                .checkbox(&mut use_hex_view, fl!(LOADER, "hex-view"))
                                .clicked()
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
                    egui::ScrollArea::horizontal()
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                            let cur_pc = app.snap.lock().map(|g| g.pc).unwrap_or(0);
                            let need_scroll = app.scroll_to_pc_req.load(Ordering::SeqCst)
                                || (cur_pc != app.last_pc_for_scroll);
                            let text_style = egui::TextStyle::Monospace;
                            let row_height = ui.text_style_height(&text_style);
                            let row_height_with_spacing = row_height + ui.spacing().item_spacing.y;

                            let mut pc_line_index: Option<usize> = None;
                            let mut highlight_rect: Option<egui::Rect> = None;
                            let mut inner_rect: Option<egui::Rect> = None;

                            // Snapshot the BP-state list once; each
                            // render_listing_lines call below borrows it.
                            // Cloning is cheap (small Vec) and cleaner
                            // than threading a MutexGuard down the
                            // call chain.
                            let bp_states_snap: Vec<(u16, BpUiState)> =
                                app.bp_states.lock().map(|g| g.clone()).unwrap_or_default();
                            if use_hex_view {
                                if let Some(txt) = &app.lst_text {
                                    if let Ok(s) = txt.lock() {
                                        let rev = app.hex_rev.load(Ordering::SeqCst);
                                        if app.hex_cache.rev != rev {
                                            app.hex_cache.line_offsets = build_line_offsets(&s);
                                            app.hex_cache.line_count =
                                                app.hex_cache.line_offsets.len();
                                            app.hex_cache.lossy_text = None;
                                            app.hex_cache.rev = rev;
                                        }
                                        let total_rows = app.hex_cache.line_count;
                                        if follow_pc && need_scroll {
                                            pc_line_index = find_pc_line_index(
                                                &s,
                                                &app.hex_cache.line_offsets,
                                                cur_pc,
                                            );
                                        }
                                        let scroll_target = if follow_pc && need_scroll {
                                            pc_line_index
                                        } else {
                                            None
                                        };
                                        let (highlight, inner) = render_listing_lines(
                                            ui,
                                            &s,
                                            &app.hex_cache.line_offsets,
                                            row_height,
                                            total_rows,
                                            cur_pc,
                                            &app.cmds,
                                            scroll_target,
                                            &bp_states_snap,
                                        );
                                        highlight_rect = highlight;
                                        inner_rect = inner;
                                    }
                                }
                            } else if let Some(m) = &app.lst_mmap {
                                let rev = app.lst_rev.load(Ordering::SeqCst);
                                let text_ref = match std::str::from_utf8(&m[..]) {
                                    Ok(s) => {
                                        if app.lst_cache.rev != rev {
                                            app.lst_cache.line_offsets = build_line_offsets(s);
                                            app.lst_cache.line_count =
                                                app.lst_cache.line_offsets.len();
                                            app.lst_cache.lossy_text = None;
                                            app.lst_cache.rev = rev;
                                        }
                                        Some(s)
                                    }
                                    Err(_) => {
                                        if app.lst_cache.rev != rev {
                                            let cow = String::from_utf8_lossy(&m[..]);
                                            app.lst_cache.lossy_text = Some(cow.to_string());
                                            if let Some(text) = app.lst_cache.lossy_text.as_deref()
                                            {
                                                app.lst_cache.line_offsets =
                                                    build_line_offsets(text);
                                                app.lst_cache.line_count =
                                                    app.lst_cache.line_offsets.len();
                                            }
                                            app.lst_cache.rev = rev;
                                        }
                                        app.lst_cache.lossy_text.as_deref()
                                    }
                                };
                                if let Some(text) = text_ref {
                                    let total_rows = app.lst_cache.line_count;
                                    if follow_pc && need_scroll {
                                        pc_line_index = find_pc_line_index(
                                            text,
                                            &app.lst_cache.line_offsets,
                                            cur_pc,
                                        );
                                    }
                                    let scroll_target = if follow_pc && need_scroll {
                                        pc_line_index
                                    } else {
                                        None
                                    };
                                    let (highlight, inner) = render_listing_lines(
                                        ui,
                                        text,
                                        &app.lst_cache.line_offsets,
                                        row_height,
                                        total_rows,
                                        cur_pc,
                                        &app.cmds,
                                        scroll_target,
                                        &bp_states_snap,
                                    );
                                    highlight_rect = highlight;
                                    inner_rect = inner;
                                }
                            }

                            // The actual scroll-to-PC happens inside
                            // render_listing_lines (it has to, because
                            // `scroll_to_rect` needs to fire from
                            // *inside* the vertical ScrollArea closure).
                            // Out here we just acknowledge the request.
                            if follow_pc && need_scroll {
                                if pc_line_index.is_some() || highlight_rect.is_some() {
                                    app.scroll_to_pc_req.store(false, Ordering::SeqCst);
                                    app.last_pc_for_scroll = cur_pc;
                                }
                            }
                            // `inner_rect` and `row_height_with_spacing` are
                            // no longer used after the move; silence dead-code
                            // warnings without losing the locals (other code
                            // may reference inner_rect later).
                            let _ = inner_rect;
                            let _ = row_height_with_spacing;
                        });
                });
        });
}

fn build_line_offsets(text: &str) -> Vec<usize> {
    if text.is_empty() {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let mut offsets = Vec::new();
    offsets.push(0);
    for (idx, b) in bytes.iter().enumerate() {
        if *b == b'\n' && idx + 1 < bytes.len() {
            offsets.push(idx + 1);
        }
    }
    offsets
}

fn line_at<'a>(text: &'a str, offsets: &[usize], idx: usize) -> &'a str {
    if idx >= offsets.len() {
        return "";
    }
    let start = offsets[idx];
    let mut end = if idx + 1 < offsets.len() {
        offsets[idx + 1].saturating_sub(1)
    } else {
        text.len()
    };
    if end > start && text.as_bytes()[end - 1] == b'\r' {
        end = end.saturating_sub(1);
    }
    &text[start..end]
}

fn render_listing_lines(
    ui: &mut egui::Ui,
    text: &str,
    offsets: &[usize],
    row_height: f32,
    total_rows: usize,
    cur_pc: u16,
    cmds: &std::sync::Arc<std::sync::Mutex<Vec<Command>>>,
    // When `Some(idx)`, the inner ScrollArea will scroll so that
    // line `idx` lands centered in the viewport. Must be done from
    // *inside* the ScrollArea closure — calling `scroll_to_rect`
    // from outside it routes the request to the wrong parent and
    // is what made Follow PC look totally unresponsive.
    scroll_target_idx: Option<usize>,
    // Snapshot of breakpoint states keyed by address. Used to draw a
    // ● / ○ marker at the start of each addressable row, and unaffected
    // by GC / mutation outside this paint (the snapshot is owned by the
    // caller and only read here).
    bp_states: &[(u16, BpUiState)],
) -> (Option<egui::Rect>, Option<egui::Rect>) {
    let mut highlight_rect: Option<egui::Rect> = None;
    let total_rows = total_rows.min(offsets.len());
    if total_rows == 0 {
        return (None, None);
    }
    let output = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_height, total_rows, |ui, row_range| {
            let row_h_with_spacing = row_height + ui.spacing().item_spacing.y;
            // Sticky-scroll Follow PC: only re-center the viewport when
            // the target row has actually drifted out of the visible
            // window. As long as PC stays within the rows currently
            // rendered by `show_rows`, leave the scroll position alone
            // — the per-row PC highlight in the loop below is enough
            // visual feedback, and re-centering on every step would
            // make the listing flicker in a way that's harder to read.
            //
            // Note `row_range` is the half-open `[start, end)` range
            // of rows `show_rows` is rendering this frame. Rows at
            // the edges of that range may be only partially visible,
            // but treating them as in-view matches how a user would
            // perceive "PC is still on screen".
            //
            // (See render_listing_lines docstring above for the
            //  reason this scroll fires from inside the closure.)
            if let Some(target_idx) = scroll_target_idx {
                let in_view = target_idx >= row_range.start && target_idx < row_range.end;
                if !in_view {
                    let visible_top = ui.min_rect().top();
                    let content_origin_y =
                        visible_top - row_range.start as f32 * row_h_with_spacing;
                    let y = content_origin_y + target_idx as f32 * row_h_with_spacing;
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(ui.min_rect().left(), y),
                        egui::vec2(ui.available_width().max(64.0), row_height),
                    );
                    ui.scroll_to_rect(rect, Some(egui::Align::Center));
                }
            }
            let end = row_range.end.min(offsets.len());
            for idx in row_range.start..end {
                let line = line_at(text, offsets, idx);
                let ltrim = line.trim_start();
                let mut token = ltrim.split_whitespace().next().unwrap_or("");
                if token.ends_with(':') {
                    token = &token[..token.len() - 1];
                }
                let addr_for_line: Option<u16> = if token.len() == 4 {
                    u16::from_str_radix(token, 16).ok()
                } else {
                    None
                };
                let upper = ltrim.to_ascii_uppercase();
                let is_macro_line = upper.contains(" MACRO")
                    || upper.contains(" ENDM")
                    || upper.starts_with("MACRO ");
                let is_pc_addr_token = addr_for_line == Some(cur_pc) && !is_macro_line;
                let (label_run_until, label_set_pc, label_cancel, label_toggle_bp) = (
                    fl!(LOADER, "run-to-here"),
                    fl!(LOADER, "set-pc-to-here"),
                    fl!(LOADER, "cancel"),
                    fl!(LOADER, "toggle-breakpoint"),
                );

                // Prepend a one-character BP marker so the gutter
                // column is always aligned regardless of whether a row
                // has a breakpoint. Done at the string level so the
                // PC-highlight background still spans the full row.
                let marker =
                    bp_marker_glyph(addr_for_line.and_then(|a| bp_state_for(bp_states, a)));
                let row_text = format!("{} {}", marker, line);

                if is_pc_addr_token {
                    let mut rt = egui::RichText::new(&row_text).monospace();
                    rt = rt
                        .background_color(egui::Color32::from_rgb(255, 60, 60))
                        .color(egui::Color32::BLACK);
                    // Sense::click() so we get .double_clicked() events;
                    // the existing hover/context-menu behaviour still
                    // works under click sense.
                    let resp = ui.add(egui::Label::new(rt).sense(egui::Sense::click()));
                    if let Some(addr) = addr_for_line {
                        if resp.double_clicked() {
                            if let Ok(mut q) = cmds.lock() {
                                q.push(Command::CycleBreakpointAtAddr(addr));
                            }
                        }
                        resp.context_menu(|ui| {
                            if ui.button(label_toggle_bp).clicked() {
                                if let Ok(mut q) = cmds.lock() {
                                    q.push(Command::CycleBreakpointAtAddr(addr));
                                }
                                ui.close_menu();
                            }
                            if ui.button(label_run_until).clicked() {
                                if let Ok(mut q) = cmds.lock() {
                                    q.push(Command::RunUntil(addr));
                                }
                                ui.close_menu();
                            }
                            if ui.button(label_set_pc).clicked() {
                                if let Ok(mut q) = cmds.lock() {
                                    q.push(Command::SetPc(addr));
                                }
                                ui.close_menu();
                            }
                            if ui.button(label_cancel).clicked() {
                                ui.close_menu();
                            }
                        });
                    }
                    highlight_rect = Some(resp.rect);
                } else {
                    let resp = ui.add(
                        egui::Label::new(egui::RichText::new(&row_text).monospace())
                            .sense(egui::Sense::click()),
                    );
                    if let Some(addr) = addr_for_line {
                        if resp.double_clicked() {
                            if let Ok(mut q) = cmds.lock() {
                                q.push(Command::CycleBreakpointAtAddr(addr));
                            }
                        }
                        resp.context_menu(|ui| {
                            if ui.button(label_toggle_bp).clicked() {
                                if let Ok(mut q) = cmds.lock() {
                                    q.push(Command::CycleBreakpointAtAddr(addr));
                                }
                                ui.close_menu();
                            }
                            if ui.button(label_run_until).clicked() {
                                if let Ok(mut q) = cmds.lock() {
                                    q.push(Command::RunUntil(addr));
                                }
                                ui.close_menu();
                            }
                            if ui.button(label_set_pc).clicked() {
                                if let Ok(mut q) = cmds.lock() {
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
                        let fill = egui::Color32::from_rgba_unmultiplied(128, 128, 128, 40);
                        let stroke_col = ui.visuals().widgets.hovered.fg_stroke.color;
                        ui.painter()
                            .rect_filled(resp.rect, egui::Rounding::same(0.0), fill);
                        ui.painter().rect_stroke(
                            resp.rect,
                            egui::Rounding::same(0.0),
                            egui::Stroke::new(1.0, stroke_col),
                        );
                    }
                }
            }
        });
    (highlight_rect, Some(output.inner_rect))
}

fn find_pc_line_index(text: &str, offsets: &[usize], pc: u16) -> Option<usize> {
    for (idx, _) in offsets.iter().enumerate() {
        let line = line_at(text, offsets, idx);
        let ltrim = line.trim_start();
        let mut token = ltrim.split_whitespace().next().unwrap_or("");
        if token.ends_with(':') {
            token = &token[..token.len() - 1];
        }
        let addr_for_line: Option<u16> = if token.len() == 4 {
            u16::from_str_radix(token, 16).ok()
        } else {
            None
        };
        if addr_for_line != Some(pc) {
            continue;
        }
        let upper = ltrim.to_ascii_uppercase();
        let is_macro_line =
            upper.contains(" MACRO") || upper.contains(" ENDM") || upper.starts_with("MACRO ");
        if !is_macro_line {
            return Some(idx);
        }
    }
    None
}
