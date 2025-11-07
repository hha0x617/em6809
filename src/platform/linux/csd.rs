#[cfg(target_os = "linux")]
use eframe::egui;

/// Draw a client-side decorated header bar on Linux, handling drag and window buttons.
#[cfg(target_os = "linux")]
pub fn show_header(ctx: &egui::Context, about_open: &mut bool, csd_maximized: &mut bool) {
    eframe::egui::TopBottomPanel::top("menu_bar_csd")
        .frame(
            eframe::egui::Frame::default()
                .fill(ctx.style().visuals.panel_fill)
                .inner_margin(egui::Margin {
                    left: 8.0,
                    right: 8.0,
                    top: 6.0,
                    bottom: 6.0,
                }),
        )
        .show(ctx, |ui| {
            eframe::egui::menu::bar(ui, |ui| {
                // Left: small icon + title (drag area)
                ui.horizontal(|ui| {
                    let desired = egui::vec2(18.0, 18.0);
                    let (icon_rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
                    let painter = ui.painter_at(icon_rect);
                    let base = egui::Color32::from_rgb(120, 128, 140);
                    painter.rect_filled(icon_rect, 4.0, base);
                    let e_pos = egui::pos2(icon_rect.center().x, icon_rect.center().y - 1.0);
                    painter.text(
                        e_pos,
                        egui::Align2::CENTER_CENTER,
                        "e",
                        egui::FontId::proportional(12.0),
                        egui::Color32::from_rgb(30, 30, 30),
                    );
                    let title_resp = ui.label("em6809");
                    let drag_rect = icon_rect.union(title_resp.rect);
                    let drag_resp = ui.interact(
                        drag_rect,
                        ui.id().with("csd_title_drag"),
                        egui::Sense::drag(),
                    );
                    if drag_resp.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                });

                // Middle: Help menu
                ui.menu_button("Help", |ui| {
                    if ui.button("About em6809…").clicked() {
                        *about_open = true;
                        ui.close_menu();
                    }
                });

                // Right: control buttons (flat, menu-like)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let flat_btn = |ui: &mut egui::Ui, label: &str, tip: &str| -> egui::Response {
                        let desired = egui::vec2(28.0, 20.0);
                        let (rect, resp0) = ui.allocate_exact_size(desired, egui::Sense::click());
                        let painter = ui.painter_at(rect);
                        if resp0.hovered() {
                            let c = ui.visuals().widgets.hovered.bg_fill;
                            painter.rect_filled(rect.shrink(3.0), 4.0, c);
                        }
                        let text_color = ui.visuals().widgets.inactive.fg_stroke.color;
                        let font = egui::FontId::proportional(14.0);
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            label,
                            font,
                            text_color,
                        );
                        if tip.is_empty() {
                            resp0
                        } else {
                            resp0.on_hover_text(tip)
                        }
                    };

                    // Close
                    if flat_btn(ui, "×", "Close").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    // Maximize / Restore
                    let show_restore = *csd_maximized;
                    let (label, tip, to_max) = if show_restore {
                        ("❐", "Restore", false)
                    } else {
                        ("□", "Maximize", true)
                    };
                    if flat_btn(ui, label, tip).clicked() {
                        *csd_maximized = to_max;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(to_max));
                    }
                    // Minimize
                    if flat_btn(ui, "–", "Minimize").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                });
            });
        });
}
