// Breakpoints panel — read-only listing of `cpu.breakpoints` plus
// per-row controls (enable checkbox + delete button) that turn into
// `Command::SetBreakpointEnabled` / `Command::RemoveBreakpoint` and
// flow through the worker's command queue. The worker mutates the
// real `BreakpointSet` and pushes a fresh snapshot back via
// `Viewer::update_bp_list`, so the next paint shows the new state
// without the GUI side ever holding the worker's lock.

use eframe::egui;

use super::{BpInfo, Command, RegApp};

pub(super) fn render_breakpoints_panel(app: &mut RegApp, ui: &mut egui::Ui) {
    // Snapshot once per paint — cheaper than holding the Mutex across
    // each row's button/checkbox interactions.
    let bps: Vec<BpInfo> = app.bp_list.lock().map(|g| g.clone()).unwrap_or_default();

    if bps.is_empty() {
        ui.weak("No breakpoints set.");
        ui.add_space(4.0);
        ui.label("Tip: double-click a row in the listing pane to add one.");
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            // Header row, monospaced so columns line up with the data
            // rows below.
            ui.horizontal(|ui| {
                ui.add_sized([22.0, 0.0], egui::Label::new("")); // enable checkbox
                ui.add_sized(
                    [60.0, 0.0],
                    egui::Label::new(egui::RichText::new("Addr").monospace().strong()),
                );
                ui.add_sized(
                    [60.0, 0.0],
                    egui::Label::new(egui::RichText::new("Hits").monospace().strong()),
                );
                ui.add_sized(
                    [60.0, 0.0],
                    egui::Label::new(egui::RichText::new("Skip").monospace().strong()),
                );
                ui.add_sized([24.0, 0.0], egui::Label::new("")); // delete button
                ui.add_sized(
                    [120.0, 0.0],
                    egui::Label::new(egui::RichText::new("Condition").monospace().strong()),
                );
            });
            ui.separator();

            for bp in bps.iter() {
                ui.horizontal(|ui| {
                    let mut enabled = bp.enabled;
                    if ui
                        .add_sized([22.0, 0.0], egui::Checkbox::without_text(&mut enabled))
                        .changed()
                    {
                        if let Ok(mut q) = app.cmds.lock() {
                            q.push(Command::SetBreakpointEnabled(bp.id, enabled));
                        }
                    }
                    ui.add_sized(
                        [60.0, 0.0],
                        egui::Label::new(
                            egui::RichText::new(format!("${:04X}", bp.addr)).monospace(),
                        ),
                    );
                    ui.add_sized(
                        [60.0, 0.0],
                        egui::Label::new(
                            egui::RichText::new(format!("{}", bp.hit_count)).monospace(),
                        ),
                    );
                    ui.add_sized(
                        [60.0, 0.0],
                        egui::Label::new(
                            egui::RichText::new(format!("{}", bp.ignore_count)).monospace(),
                        ),
                    );
                    let del = ui.add_sized([24.0, 0.0], egui::Button::new("✕"));
                    if del.clicked() {
                        if let Ok(mut q) = app.cmds.lock() {
                            q.push(Command::RemoveBreakpoint(bp.id));
                        }
                    }
                    let cond_text = bp.condition.as_deref().unwrap_or("(none)");
                    ui.add(
                        egui::Label::new(egui::RichText::new(cond_text).monospace())
                            .wrap_mode(egui::TextWrapMode::Truncate),
                    );
                });
            }
        });
}
