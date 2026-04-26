// Call-stack panel — read-only view of `Cpu::shadow_stack`. Rendered
// newest-first (current frame on top) so the most relevant context
// for a paused program sits where the eye lands first. Frames carry
// kind / call_site / target / return_addr / sp_at_call; the worker
// thread converts `em6809::debug::CallFrame` into the UI-side
// `CallFrameInfo` mirror before pushing here, so this module
// stays free of any `em6809::debug` dependency.

use eframe::egui;

use super::{CallFrameInfo, RegApp};

pub(super) fn render_call_stack_panel(app: &mut RegApp, ui: &mut egui::Ui) {
    let frames: Vec<CallFrameInfo> = app.call_stack.lock().map(|g| g.clone()).unwrap_or_default();

    if frames.is_empty() {
        ui.weak("No active calls.");
        ui.add_space(4.0);
        ui.label(
            "Frames are pushed by BSR/JSR/SWI and by IRQ/FIRQ/NMI \
             entry, popped by RTS/RTI.",
        );
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            // Column header.
            ui.horizontal(|ui| {
                ui.add_sized(
                    [28.0, 0.0],
                    egui::Label::new(egui::RichText::new("#").monospace().strong()),
                );
                ui.add_sized(
                    [46.0, 0.0],
                    egui::Label::new(egui::RichText::new("Kind").monospace().strong()),
                );
                ui.add_sized(
                    [62.0, 0.0],
                    egui::Label::new(egui::RichText::new("Call").monospace().strong()),
                );
                ui.add_sized([22.0, 0.0], egui::Label::new("")); // arrow
                ui.add_sized(
                    [62.0, 0.0],
                    egui::Label::new(egui::RichText::new("Target").monospace().strong()),
                );
                ui.add_sized(
                    [62.0, 0.0],
                    egui::Label::new(egui::RichText::new("Return").monospace().strong()),
                );
                ui.add_sized(
                    [62.0, 0.0],
                    egui::Label::new(egui::RichText::new("S").monospace().strong()),
                );
            });
            ui.separator();

            // Newest-first: current frame at the top of the panel.
            // The numbering keeps the original push index so users can
            // tell "we are 3 calls deep" without counting rows.
            let total = frames.len();
            for (rev_idx, frame) in frames.iter().rev().enumerate() {
                let depth = total - rev_idx; // 1-based depth, top = total
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [28.0, 0.0],
                        egui::Label::new(egui::RichText::new(format!("{}", depth)).monospace()),
                    );
                    ui.add_sized(
                        [46.0, 0.0],
                        egui::Label::new(
                            egui::RichText::new(frame.kind.label()).monospace().strong(),
                        ),
                    );
                    ui.add_sized(
                        [62.0, 0.0],
                        egui::Label::new(
                            egui::RichText::new(format!("${:04X}", frame.call_site)).monospace(),
                        ),
                    );
                    ui.add_sized([22.0, 0.0], egui::Label::new("→"));
                    ui.add_sized(
                        [62.0, 0.0],
                        egui::Label::new(
                            egui::RichText::new(format!("${:04X}", frame.target)).monospace(),
                        ),
                    );
                    ui.add_sized(
                        [62.0, 0.0],
                        egui::Label::new(
                            egui::RichText::new(format!("${:04X}", frame.return_addr)).monospace(),
                        ),
                    );
                    ui.add_sized(
                        [62.0, 0.0],
                        egui::Label::new(
                            egui::RichText::new(format!("${:04X}", frame.sp_at_call)).monospace(),
                        ),
                    );
                });
            }
        });
}
