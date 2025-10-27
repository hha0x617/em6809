#![allow(dead_code)]

use std::sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, Arc, Mutex};

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
    pub fn with_ab(mut self, a: u8, b: u8) -> Self { self.a = a; self.b = b; self }
    pub fn with_d(mut self, d: u16) -> Self { self.d = d; self }
    pub fn with_xy(mut self, x: u16, y: u16) -> Self { self.x = x; self.y = y; self }
    pub fn with_us(mut self, u: u16, s: u16) -> Self { self.u = u; self.s = s; self }
    pub fn with_dp_pc(mut self, dp: u8, pc: u16) -> Self { self.dp = dp; self.pc = pc; self }
    pub fn with_cc(mut self, cc: u8) -> Self { self.cc = cc; self }
}

#[cfg(feature = "gui")]
mod gui {
    use super::*;
    use eframe::{egui, App};
    use egui_extras::{Column, TableBuilder};
    use memmap2::Mmap;
    use std::fs::File;
    use std::path::PathBuf;

    pub struct RegApp {
        pub snap: Arc<Mutex<RegSnapshot>>,
        pub paused: Arc<AtomicBool>,
        pub step_requests: Arc<AtomicUsize>,
        pub lst_mmap: Option<Arc<Mmap>>,
    }

    impl App for RegApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            // Right side: listing in its own scroll area and panel (so only listing scrolls)
            if let Some(m) = &self.lst_mmap {
                egui::SidePanel::right("lst_panel")
                    .resizable(true)
                    .default_width(877.0) // ~3/4 of 1170px window width (1:3 ratio)
                    .show(ctx, |ui| {
                        ui.heading("Listing (.lst)");
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .stick_to_right(true)
                            .show(ui, |ui| {
                                let paused = self.paused.load(Ordering::SeqCst);
                                let cur_pc = self.snap.lock().map(|g| g.pc).unwrap_or(0);
                                let pc_hex = format!("{:04X}", cur_pc);
                                // Iterate lines and highlight the one whose address token matches PC when paused.
                                let render_lines = |ui: &mut egui::Ui, text: &str| {
                                    for line in text.lines() {
                                        if paused {
                                            let ltrim = line.trim_start();
                                            let mut token = ltrim.split_whitespace().next().unwrap_or("");
                                            if token.ends_with(':') { token = &token[..token.len()-1]; }
                                            if token.eq_ignore_ascii_case(&pc_hex) {
                                                ui.label(
                                                    egui::RichText::new(line)
                                                        .monospace()
                                                        .background_color(egui::Color32::from_rgb(255, 0, 0)),
                                                );
                                                continue;
                                            }
                                        }
                                        ui.label(egui::RichText::new(line).monospace());
                                    }
                                };
                                match std::str::from_utf8(&m[..]) {
                                    Ok(s) => render_lines(ui, s),
                                    Err(_) => {
                                        let cow = String::from_utf8_lossy(&m[..]);
                                        render_lines(ui, cow.as_ref());
                                    }
                                }
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
                            let stroke = egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
                            ui.painter().rect_stroke(rect, egui::Rounding::same(0.0), stroke);
                        });
                        header.col(|ui| {
                            ui.strong("Value");
                            let rect = ui.max_rect();
                            let stroke = egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
                            ui.painter().rect_stroke(rect, egui::Rounding::same(0.0), stroke);
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
                                    let stroke = egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
                                    ui.painter().rect_stroke(rect, egui::Rounding::same(0.0), stroke);
                                });
                                row.col(|ui| {
                                    ui.monospace(val);
                                    let rect = ui.max_rect();
                                    let stroke = egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
                                    ui.painter().rect_stroke(rect, egui::Rounding::same(0.0), stroke);
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
                            if i > 0 { cc_text.push(' '); }
                            if (s.cc & mask) != 0 { cc_text.push(*ch); } else { cc_text.push(ch.to_ascii_lowercase()); }
                        }
                        body.row(20.0, |mut row| {
                            row.col(|ui| {
                                ui.monospace("CC");
                                let rect = ui.max_rect();
                                let stroke = egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
                                ui.painter().rect_stroke(rect, egui::Rounding::same(0.0), stroke);
                            });
                            row.col(|ui| {
                                ui.monospace(cc_text);
                                let rect = ui.max_rect();
                                let stroke = egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
                                ui.painter().rect_stroke(rect, egui::Rounding::same(0.0), stroke);
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
        _handle: std::thread::JoinHandle<()>,
    }

    impl RegView {
        pub fn spawn(title: &str, lst_path: Option<PathBuf>) -> Self {
            let snapshot = Arc::new(Mutex::new(RegSnapshot::default()));
            let paused = Arc::new(AtomicBool::new(false));
            let step_requests = Arc::new(AtomicUsize::new(0));
            let lst_mmap: Option<Arc<Mmap>> = lst_path.and_then(|p| {
                if let Ok(f) = File::open(p) {
                    unsafe { Mmap::map(&f).ok() }.map(Arc::new)
                } else { None }
            });

            let snap_clone = Arc::clone(&snapshot);
            let paused_clone = Arc::clone(&paused);
            let step_requests_clone = Arc::clone(&step_requests);
            let lst_clone = lst_mmap.as_ref().map(Arc::clone);
            let title = title.to_string();

            let handle = std::thread::spawn(move || {
                let mut native_options = eframe::NativeOptions::default();
                // Make the window compact and appropriate for the register grid
                native_options.viewport = eframe::egui::ViewportBuilder::default()
                    .with_inner_size(eframe::egui::vec2(1170.0, 600.0))
                    .with_min_inner_size(eframe::egui::vec2(780.0, 360.0))
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
                let app = RegApp { snap: snap_clone, paused: paused_clone, step_requests: step_requests_clone, lst_mmap: lst_clone };
                let _ = eframe::run_native(&title, native_options, Box::new(|_| Box::new(app)));
            });

            Self { snapshot, paused, step_requests, lst_mmap, _handle: handle }
        }

        pub fn push(&self, s: RegSnapshot) {
            if let Ok(mut guard) = self.snapshot.lock() {
                *guard = s;
            }
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
}

#[cfg(not(feature = "gui"))]
mod nogui {
    use super::*;
    use std::path::PathBuf;

    pub struct Viewer {
        // No-op placeholder for non-GUI builds
        _p: (),
    }

    impl Viewer {
        pub fn spawn(_title: &str, _lst_path: Option<PathBuf>) -> Self { Self { _p: () } }
        pub fn push(&self, _s: RegSnapshot) {}
        pub fn is_paused(&self) -> bool { false }
        pub fn should_pause(&self) -> bool { false }
        pub fn set_paused(&self, _v: bool) {}
    }
}

// Re-export the viewer type depending on feature
#[cfg(feature = "gui")]
pub use gui::Viewer;
#[cfg(not(feature = "gui"))]
pub use nogui::Viewer;
