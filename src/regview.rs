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

    pub struct RegApp {
        pub snap: Arc<Mutex<RegSnapshot>>,
        pub paused: Arc<AtomicBool>,
        pub step_requests: Arc<AtomicUsize>,
    }

    impl App for RegApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("MC6809 Registers");

                let s = *self.snap.lock().unwrap();

                ui.horizontal(|ui| {
                    if self.paused.load(Ordering::SeqCst) {
                        if ui.button("Resume").clicked() {
                            self.paused.store(false, Ordering::SeqCst);
                        }
                        if ui.button("Step").clicked() {
                            // Ensure paused and request a single step
                            self.paused.store(true, Ordering::SeqCst);
                            self.step_requests.fetch_add(1, Ordering::SeqCst);
                        }
                    } else {
                        if ui.button("Pause").clicked() {
                            self.paused.store(true, Ordering::SeqCst);
                        }
                        if ui.button("Step").clicked() {
                            // Enter paused state and request one step
                            self.paused.store(true, Ordering::SeqCst);
                            self.step_requests.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                });

                ui.separator();
                // Table-like layout for registers
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::auto().at_least(40.0)) // Reg
                    .column(Column::remainder()) // Value
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

                        // CC flags as a single row, with lowercase for cleared bits
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

                // Refresh at ~60 FPS for smooth updates
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            });
        }
    }

    pub struct RegView {
        pub snapshot: Arc<Mutex<RegSnapshot>>,
        pub paused: Arc<AtomicBool>,
        pub step_requests: Arc<AtomicUsize>,
        _handle: std::thread::JoinHandle<()>,
    }

    impl RegView {
        pub fn spawn(title: &str) -> Self {
            let snapshot = Arc::new(Mutex::new(RegSnapshot::default()));
            let paused = Arc::new(AtomicBool::new(false));
            let step_requests = Arc::new(AtomicUsize::new(0));

            let snap_clone = Arc::clone(&snapshot);
            let paused_clone = Arc::clone(&paused);
            let step_requests_clone = Arc::clone(&step_requests);
            let title = title.to_string();

            let handle = std::thread::spawn(move || {
                let mut native_options = eframe::NativeOptions::default();
                // Make the window compact and appropriate for the register grid
                native_options.viewport = eframe::egui::ViewportBuilder::default()
                    .with_inner_size(eframe::egui::vec2(360.0, 340.0))
                    .with_min_inner_size(eframe::egui::vec2(320.0, 300.0))
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
                let app = RegApp { snap: snap_clone, paused: paused_clone, step_requests: step_requests_clone };
                let _ = eframe::run_native(&title, native_options, Box::new(|_| Box::new(app)));
            });

            Self { snapshot, paused, step_requests, _handle: handle }
        }

        pub fn push(&self, s: RegSnapshot) {
            if let Ok(mut guard) = self.snapshot.lock() {
                *guard = s;
            }
        }

        pub fn is_paused(&self) -> bool {
            self.paused.load(Ordering::SeqCst)
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

    pub struct Viewer {
        // No-op placeholder for non-GUI builds
        _p: (),
    }

    impl Viewer {
        pub fn spawn(_title: &str) -> Self { Self { _p: () } }
        pub fn push(&self, _s: RegSnapshot) {}
        pub fn is_paused(&self) -> bool { false }
        pub fn should_pause(&self) -> bool { false }
    }
}

// Re-export the viewer type depending on feature
#[cfg(feature = "gui")]
pub use gui::Viewer;
#[cfg(not(feature = "gui"))]
pub use nogui::Viewer;
