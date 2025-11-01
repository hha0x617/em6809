#![allow(dead_code)]

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

#[cfg(feature = "gui")]
mod gui {
    use super::*;
    use eframe::{egui, App};
    use egui_extras::{Column, TableBuilder};
    use memmap2::Mmap;
    use std::fs::File;
    use std::path::PathBuf;

    #[derive(Clone, Copy, Debug)]
    pub enum Command {
        SetPc(u16),
        RunUntil(u16),
    }

    pub struct RegApp {
        pub snap: Arc<Mutex<RegSnapshot>>,
        pub paused: Arc<AtomicBool>,
        pub step_requests: Arc<AtomicUsize>,
        pub lst_mmap: Option<Arc<Mmap>>,
        pub lst_text: Option<Arc<Mutex<String>>>,
        pub use_hex: Arc<AtomicBool>,
        pub follow_pc_flag: Arc<AtomicBool>,
        pub span_policy: Arc<AtomicUsize>, // 0=FullSpan, 1=PcWindow
        pub window_len: Arc<AtomicUsize>,  // bytes for PcWindow
        pub cmds: Arc<Mutex<Vec<Command>>>,
        pub ja_font: Option<Arc<Vec<u8>>>,
        pub fonts_applied: bool,
        pub use_japanese: bool,
        pub last_pc_for_scroll: u16,
        // Memory view state
        pub mem_follow_pc: Arc<AtomicBool>,
        pub mem_window_len: Arc<AtomicUsize>,
        pub mem_window_start: Arc<AtomicUsize>,  // u16 widened
        pub mem_buf: Arc<Mutex<(u16, Vec<u8>)>>, // current window snapshot
        pub mem_hl: Arc<Mutex<Option<(u16, usize)>>>, // highlight range
        pub mem_utf8: Arc<AtomicBool>,
        pub mem_jump_text: String,
        pub mem_len_text: String,
    }

    impl App for RegApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

            // Memory pane (placed to the left of the listing pane by ordering two right panels)
            egui::SidePanel::right("mem_panel")
                .resizable(true)
                .min_width(560.0)
                .default_width(620.0)
                .show(ctx, |ui| {
                    let mut follow_pc = self.mem_follow_pc.load(Ordering::SeqCst);
                    let _win_len = self.mem_window_len.load(Ordering::SeqCst);
                    let mut start = self.mem_window_start.load(Ordering::SeqCst) as u16;
                    ui.horizontal(|ui| {
                        ui.heading(if self.use_japanese {
                            "メモリ"
                        } else {
                            "Memory"
                        });
                        ui.separator();
                        if ui
                            .checkbox(
                                &mut follow_pc,
                                if self.use_japanese {
                                    "PC追従"
                                } else {
                                    "Follow PC"
                                },
                            )
                            .clicked()
                        {
                            self.mem_follow_pc.store(follow_pc, Ordering::SeqCst);
                        }
                        let mut utf8 = self.mem_utf8.load(Ordering::SeqCst);
                        if ui.checkbox(&mut utf8, "UTF-8").clicked() {
                            self.mem_utf8.store(utf8, Ordering::SeqCst);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(if self.use_japanese { "開始" } else { "Start" });
                        let te = egui::TextEdit::singleline(&mut self.mem_jump_text)
                            .desired_width(80.0)
                            .hint_text("0x0000");
                        // Disable editing of Start when following PC
                        let start_resp = ui.add_enabled(!follow_pc, te);
                        ui.label(if self.use_japanese { "長さ" } else { "Len" });
                        let len_te = egui::TextEdit::singleline(&mut self.mem_len_text)
                            .desired_width(64.0)
                            .hint_text("256");
                        let len_resp = ui.add(len_te);
                        // Commit Start: only when not following PC, and either Enter while focused or focus lost
                        if !follow_pc {
                            if start_resp.has_focus() {
                                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                                    if let Some(val) = parse_num_u16_any_gui(&self.mem_jump_text) {
                                        start = val;
                                        self.mem_window_start.store(val as usize, Ordering::SeqCst);
                                        self.mem_jump_text = format!("0x{:04X}", val);
                                    }
                                }
                            } else if start_resp.lost_focus() {
                                if let Some(val) = parse_num_u16_any_gui(&self.mem_jump_text) {
                                    start = val;
                                    self.mem_window_start.store(val as usize, Ordering::SeqCst);
                                    self.mem_jump_text = format!("0x{:04X}", val);
                                }
                            }
                        }
                        // Commit Len: Enter while focused or on focus loss
                        if len_resp.has_focus() {
                            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                                if let Some(v) = parse_num_usize_any_gui(&self.mem_len_text) {
                                    let v = v.clamp(16, 4096);
                                    self.mem_window_len.store(v, Ordering::SeqCst);
                                    self.mem_len_text = format!("{}", v);
                                }
                            }
                        } else if len_resp.lost_focus() {
                            if let Some(v) = parse_num_usize_any_gui(&self.mem_len_text) {
                                let v = v.clamp(16, 4096);
                                self.mem_window_len.store(v, Ordering::SeqCst);
                                self.mem_len_text = format!("{}", v);
                            }
                        }
                        // Jump button removed; Enter/focus-out on Start commits address
                        if ui
                            .button(if self.use_japanese {
                                "コピー"
                            } else {
                                "Copy"
                            })
                            .clicked()
                        {
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
                            let utf8_mode = self.mem_utf8.load(Ordering::SeqCst);
                            let mut off: usize = 0;
                            while off < bytes.len() {
                                let chunk = &bytes[off..bytes.len().min(off + 16)];
                                let addr = s.wrapping_add(off as u16);
                                ui.scope(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    ui.horizontal(|ui| {
                                        ui.monospace(format!("{:04X}: ", addr));
                                        // hex bytes
                                        for (i, b) in chunk.iter().enumerate() {
                                            let a = addr.wrapping_add(i as u16);
                                            let in_hl = if let (Some(hs), Some(he)) = (hl_start, hl_end) {
                                                if hs <= he { a >= hs && a < he } else { a >= hs || a < he }
                                            } else { false };
                                            let mut rt = egui::RichText::new(format!("{:02X}", b)).monospace();
                                            if in_hl { rt = rt.background_color(hl_color); }
                                            ui.add(egui::Label::new(rt));
                                            if i != chunk.len() - 1 { ui.monospace(" "); }
                                        }
                                        // pad if fewer than 16 bytes
                                        for _ in chunk.len()..16 { ui.monospace("   "); }
                                        ui.monospace("| ");
                                        // ascii / utf-8 view without spaces between characters
                                        if utf8_mode {
                                            let s = String::from_utf8_lossy(chunk);
                                            ui.add(egui::Label::new(egui::RichText::new(s).monospace()));
                                        } else {
                                            let mut s = String::with_capacity(16);
                                            for b in chunk.iter() {
                                                let ch = if (0x20..=0x7E).contains(b) { *b as char } else { '.' };
                                                s.push(ch);
                                            }
                                            ui.monospace(s);
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
                    .resizable(true)
                    .default_width(820.0)
                    .show(ctx, |ui| {
                        let have_lst = self.lst_mmap.is_some();
                        let have_hex = self.lst_text.is_some();
                        let mut use_hex_view = self.use_hex.load(Ordering::SeqCst);
                        let mut follow_pc = self.follow_pc_flag.load(Ordering::SeqCst);
                        let mut policy = self.span_policy.load(Ordering::SeqCst);
                        let mut win_len = self.window_len.load(Ordering::SeqCst);
                        ui.horizontal(|ui| {
                            ui.heading(if use_hex_view { "Listing (hex disasm)" } else { "Listing (.lst)" });
                            ui.separator();
                            if have_lst && have_hex {
                                if ui.checkbox(&mut use_hex_view, "Hex view").clicked() {
                                    self.use_hex.store(use_hex_view, Ordering::SeqCst);
                                }
                            }
                            if ui.checkbox(&mut follow_pc, "Follow PC").clicked() {
                                self.follow_pc_flag.store(follow_pc, Ordering::SeqCst);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Span Mode:");
                            let full = policy == 0;
                            let select_full = full;
                            if ui.radio(select_full, "Full span").clicked() {
                                policy = 0;
                                self.span_policy.store(0, Ordering::SeqCst);
                            }
                            let select_win = !full;
                            if ui.radio(select_win, "PC window").clicked() {
                                policy = 1;
                                self.span_policy.store(1, Ordering::SeqCst);
                            }
                            ui.add(egui::DragValue::new(&mut win_len).clamp_range(16..=4096).speed(1));
                            ui.label("bytes");
                            if self.window_len.load(Ordering::SeqCst) != win_len {
                                self.window_len.store(win_len, Ordering::SeqCst);
                            }
                        });
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .stick_to_right(true)
                            .show(ui, |ui| {
                                let _paused = self.paused.load(Ordering::SeqCst);
                                let cur_pc = self.snap.lock().map(|g| g.pc).unwrap_or(0);
                                let pc_hex = format!("{:04X}", cur_pc);
                                // Iterate lines and highlight the one whose address token matches PC when paused.
                                let mut highlight_rect: Option<egui::Rect> = None;
                                let render_lines = |ui: &mut egui::Ui, text: &str, highlight_rect: &mut Option<egui::Rect>| {
                                    for line in text.lines() {
                                        let ltrim = line.trim_start();
                                        let mut token = ltrim.split_whitespace().next().unwrap_or("");
                                        if token.ends_with(':') { token = &token[..token.len()-1]; }
                                        let addr_for_line: Option<u16> = if token.len() == 4 { u16::from_str_radix(token, 16).ok() } else { None };
                                        let (label_run_until, label_set_pc, label_cancel) = if self.use_japanese { ("ここまで実行", "PCの値を現在のアドレスに変更", "キャンセル") } else { ("Run to here", "Set PC to this address", "Cancel") };
                                        if token.eq_ignore_ascii_case(&pc_hex) {
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
                                                        if let Ok(mut q) = self.cmds.lock() { q.push(Command::RunUntil(addr)); }
                                                        ui.close_menu();
                                                    }
                                                    if ui.button(label_set_pc).clicked() {
                                                        if let Ok(mut q) = self.cmds.lock() { q.push(Command::SetPc(addr)); }
                                                        ui.close_menu();
                                                    }
                                                    if ui.button(label_cancel).clicked() { ui.close_menu(); }
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
                                                        if let Ok(mut q) = self.cmds.lock() { q.push(Command::RunUntil(addr)); }
                                                        ui.close_menu();
                                                    }
                                                    if ui.button(label_set_pc).clicked() {
                                                        if let Ok(mut q) = self.cmds.lock() { q.push(Command::SetPc(addr)); }
                                                        ui.close_menu();
                                                    }
                                                    if ui.button(label_cancel).clicked() { ui.close_menu(); }
                                                });
                                            }
                                            // Hover highlight for the line under mouse cursor (non‑PC lines)
                                            if resp.hovered() {
                                                // Use a translucent fill so text remains readable, plus a subtle stroke
                                                let fill = egui::Color32::from_rgba_unmultiplied(128, 128, 128, 40);
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
                                        if let Ok(s) = txt.lock() { render_lines(ui, s.as_str(), &mut highlight_rect); }
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
                                // Auto-scroll when PC changes and we have a highlighted row
                                if follow_pc && cur_pc != self.last_pc_for_scroll {
                                    if let Some(rect) = highlight_rect {
                                        ui.scroll_to_rect(rect, Some(egui::Align::Center));
                                    }
                                }
                                self.last_pc_for_scroll = cur_pc;
                            });
                    });
            }

            // Left/center: registers and controls (no scrolling)
            egui::CentralPanel::default().show(ctx, |ui| {
                let s = *self.snap.lock().unwrap();
                ui.heading("MC6809 Registers");
                ui.horizontal(|ui| {
                    if self.paused.load(Ordering::SeqCst) {
                        if ui.button("Resume").clicked() {
                            self.paused.store(false, Ordering::SeqCst);
                        }
                        if ui.button("Step").clicked() {
                            self.paused.store(true, Ordering::SeqCst);
                            self.step_requests.fetch_add(1, Ordering::SeqCst);
                        }
                    } else {
                        if ui.button("Pause").clicked() {
                            self.paused.store(true, Ordering::SeqCst);
                        }
                        if ui.button("Step").clicked() {
                            self.paused.store(true, Ordering::SeqCst);
                            self.step_requests.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                });
                ui.separator();
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::auto().at_least(40.0))
                    .column(Column::remainder())
                    .header(22.0, |mut header| {
                        header.col(|ui| {
                            ui.strong("Reg");
                            let rect = ui.max_rect();
                            let stroke = egui::Stroke::new(
                                1.0,
                                ui.visuals().widgets.noninteractive.bg_stroke.color,
                            );
                            ui.painter()
                                .rect_stroke(rect, egui::Rounding::same(0.0), stroke);
                        });
                        header.col(|ui| {
                            ui.strong("Value");
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
                        let rows = [
                            ("A", format!("${:02X}", s.a)),
                            ("B", format!("${:02X}", s.b)),
                            ("D", format!("${:04X}", s.d)),
                            ("X", format!("${:04X}", s.x)),
                            ("Y", format!("${:04X}", s.y)),
                            ("U", format!("${:04X}", s.u)),
                            ("S", format!("${:04X}", s.s)),
                            ("DP", format!("${:02X}", s.dp)),
                            ("PC", format!("${:04X}", s.pc)),
                        ];
                        for (name, val) in rows {
                            body.row(20.0, |mut row| {
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
                                    ui.monospace(val);
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
                        }

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
                        body.row(20.0, |mut row| {
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
                                ui.monospace(cc_text);
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

                ctx.request_repaint_after(std::time::Duration::from_millis(16));
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
        follow_pc_flag: Arc<AtomicBool>,
        span_policy: Arc<AtomicUsize>,
        window_len: Arc<AtomicUsize>,
        cmds: Arc<Mutex<Vec<Command>>>,
        ja_font: Option<Arc<Vec<u8>>>,
        _handle: std::thread::JoinHandle<()>,
        // Memory view shared state
        mem_follow_pc: Arc<AtomicBool>,
        mem_window_len: Arc<AtomicUsize>,
        mem_window_start: Arc<AtomicUsize>,
        mem_buf: Arc<Mutex<(u16, Vec<u8>)>>,
        mem_hl: Arc<Mutex<Option<(u16, usize)>>>,
        mem_utf8: Arc<AtomicBool>,
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
            let default_use_hex_view: bool = lst_text.is_some() && lst_mmap.is_none();
            let use_hex = Arc::new(AtomicBool::new(default_use_hex_view));
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
            let handle = std::thread::spawn(move || {
                let mut native_options = eframe::NativeOptions::default();
                // Make the window compact and appropriate for the register grid
                native_options.viewport = eframe::egui::ViewportBuilder::default()
                    .with_inner_size(eframe::egui::vec2(1800.0, 700.0))
                    .with_min_inner_size(eframe::egui::vec2(900.0, 400.0))
                    .with_resizable(true);
                // On Windows, winit requires the event loop to be created on the main thread by default.
                // Allow creating it on this background thread to avoid a panic.
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
                    follow_pc_flag: follow_clone,
                    span_policy: span_policy_clone,
                    window_len: window_len_clone,
                    cmds: cmds_clone,
                    ja_font: ja_font_clone,
                    fonts_applied: false,
                    use_japanese,
                    last_pc_for_scroll: 0,
                    mem_follow_pc: mem_follow_pc_thread.clone(),
                    mem_window_len: mem_window_len_thread.clone(),
                    mem_window_start: mem_window_start_thread.clone(),
                    mem_buf: mem_buf_thread.clone(),
                    mem_hl: mem_hl_thread.clone(),
                    mem_utf8: mem_utf8_thread.clone(),
                    mem_jump_text: format!("0x{:04X}", initial_start),
                    mem_len_text: format!("{}", mem_window_len_thread.load(Ordering::SeqCst)),
                };
                let _ = eframe::run_native(&title, native_options, Box::new(|_| Box::new(app)));
            });

            Self {
                snapshot,
                paused,
                step_requests,
                lst_mmap,
                lst_text,
                use_hex,
                follow_pc_flag,
                span_policy,
                window_len,
                cmds,
                ja_font,
                _handle: handle,
                mem_follow_pc,
                mem_window_len,
                mem_window_start,
                mem_buf,
                mem_hl,
                mem_utf8,
            }
        }

        pub fn push(&self, s: RegSnapshot) {
            if let Ok(mut guard) = self.snapshot.lock() {
                *guard = s;
            }
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
        }
        // Memory view controls
        pub fn get_memory_window(&self) -> (u16, usize, bool) {
            (
                self.mem_window_start.load(Ordering::SeqCst) as u16,
                self.mem_window_len.load(Ordering::SeqCst),
                self.mem_follow_pc.load(Ordering::SeqCst),
            )
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

    fn format_memory_line(addr: u16, bytes: &[u8], _pc: u16) -> String {
        use core::fmt::Write as _;
        let mut s = String::with_capacity(80);
        let _ = write!(&mut s, "{:04X}: ", addr);
        for i in 0..16 {
            if i < bytes.len() {
                let _ = write!(&mut s, "{:02X} ", bytes[i]);
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
                let _ = write!(&mut s, "{}", c);
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

#[cfg(not(feature = "gui"))]
mod nogui {
    use super::*;
    use std::path::PathBuf;

    #[derive(Clone, Copy, Debug)]
    pub enum Command {
        SetPc(u16),
        RunUntil(u16),
    }

    pub struct Viewer {
        // No-op placeholder for non-GUI builds
        _p: (),
    }

    impl Viewer {
        pub fn spawn(
            _title: &str,
            _lst_path: Option<PathBuf>,
            _lst_fallback_text: Option<String>,
            _initial_start: u16,
        ) -> Self {
            Self { _p: () }
        }
        pub fn push(&self, _s: RegSnapshot) {}
        pub fn is_paused(&self) -> bool {
            false
        }
        pub fn should_pause(&self) -> bool {
            false
        }
        pub fn set_paused(&self, _v: bool) {}
        pub fn update_hex_listing(&self, _s: String) {}
        pub fn update_hex_lines(&self, _items: Vec<(u16, String)>) {}
        pub fn get_span_policy(&self) -> (usize, usize) {
            (0, 256)
        }
        pub fn force_hex_view(&self) {}
        pub fn get_memory_window(&self) -> (u16, usize, bool) {
            (0, 256, true)
        }
        pub fn update_memory_window(&self, _start: u16, _bytes: Vec<u8>) {}
        pub fn set_memory_highlight(&self, _start: u16, _len: usize) {}
        pub fn take_command(&self) -> Option<Command> {
            None
        }
    }
}

// Re-export the viewer type depending on feature
#[cfg(feature = "gui")]
pub use gui::Command;
#[cfg(feature = "gui")]
pub use gui::Viewer;
#[cfg(not(feature = "gui"))]
pub use nogui::Command;
#[cfg(not(feature = "gui"))]
pub use nogui::Viewer;
