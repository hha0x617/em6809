#[cfg(target_os = "linux")]
use eframe::egui;

/// Draw a client-side decorated header bar on Linux, handling drag and window buttons.
#[cfg(target_os = "linux")]
pub fn show_header<F>(ctx: &egui::Context, csd_maximized: &mut bool, mut render_menus: F)
where
    F: FnMut(&mut egui::Ui),
{
    eframe::egui::TopBottomPanel::top("menu_bar_csd")
        .frame(
            eframe::egui::Frame::default()
                .fill(ctx.style().visuals.panel_fill)
                .inner_margin(egui::Margin {
                    left: 8,
                    right: 8,
                    top: 6,
                    bottom: 6,
                }),
        )
        .show(ctx, |ui| {
            eframe::egui::MenuBar::new().ui(ui, |ui| {
                // Left: small icon + title（タイトルは選択不可・描画のみ）
                let desired = egui::vec2(18.0, 18.0);
                let (icon_rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
                let painter_icon = ui.painter_at(icon_rect);
                let base = egui::Color32::from_rgb(120, 128, 140);
                painter_icon.rect_filled(icon_rect, 4.0, base);
                let e_pos = egui::pos2(icon_rect.center().x, icon_rect.center().y - 1.0);
                painter_icon.text(
                    e_pos,
                    egui::Align2::CENTER_CENTER,
                    "e",
                    egui::FontId::proportional(12.0),
                    egui::Color32::from_rgb(30, 30, 30),
                );
                ui.add_space(6.0);
                // Measure and draw title text without making it selectable/interactable
                let title_font = egui::FontId::proportional(14.0);
                let title_color = ui.visuals().text_color();
                let galley = ui.painter().layout_no_wrap(
                    "em6809".to_owned(),
                    title_font.clone(),
                    title_color,
                );
                let (title_rect, _) =
                    ui.allocate_exact_size(egui::vec2(galley.size().x, 18.0), egui::Sense::hover());
                let title_pos = egui::pos2(title_rect.left(), icon_rect.center().y);
                ui.painter().text(
                    title_pos,
                    egui::Align2::LEFT_CENTER,
                    "em6809",
                    title_font,
                    title_color,
                );
                // Allow dragging by grabbing the icon+title area as well
                let title_drag_rect = icon_rect.union(title_rect);
                let drag_left = ui.interact(
                    title_drag_rect,
                    ui.id().with("csd_title_drag_left"),
                    egui::Sense::drag(),
                );
                if drag_left.is_pointer_button_down_on() || drag_left.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                ui.add_space(8.0);

                // Middle: caller-provided menus (e.g., File, Help)
                render_menus(ui);

                // Drag spacer between menus and right-side window buttons only
                // Reserve approx width for three right controls (×, □/❐, –) and margins
                let reserve_right = 96.0f32;
                let avail = ui.available_size_before_wrap().x;
                if avail > reserve_right + 4.0 {
                    let spacer_w = avail - reserve_right;
                    let (_rect, resp) =
                        ui.allocate_exact_size(egui::vec2(spacer_w, 22.0), egui::Sense::drag());
                    if resp.is_pointer_button_down_on() || resp.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                }

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

#[cfg(target_os = "linux")]
pub fn show_settings_header(ctx: &egui::Context, title: &str) {
    use eframe::egui::{Align2, FontId, Layout, Sense};

    eframe::egui::TopBottomPanel::top("settings_native_header")
        .frame(
            eframe::egui::Frame::default()
                .fill(ctx.style().visuals.panel_fill)
                .inner_margin(egui::Margin {
                    left: 8,
                    right: 8,
                    top: 6,
                    bottom: 6,
                }),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Icon placeholder + title
                let desired = egui::vec2(16.0, 16.0);
                let (icon_rect, _) = ui.allocate_exact_size(desired, Sense::hover());
                let painter_icon = ui.painter_at(icon_rect);
                let base = egui::Color32::from_rgb(120, 128, 140);
                painter_icon.rect_filled(icon_rect, 4.0, base);
                let e_pos = egui::pos2(icon_rect.center().x, icon_rect.center().y - 1.0);
                painter_icon.text(
                    e_pos,
                    Align2::CENTER_CENTER,
                    "e",
                    FontId::proportional(11.0),
                    egui::Color32::from_rgb(30, 30, 30),
                );
                ui.add_space(6.0);
                let title_font = FontId::proportional(14.0);
                let title_color = ui.visuals().text_color();
                ui.label(
                    egui::RichText::new(title)
                        .font(title_font.clone())
                        .color(title_color),
                );
                // Drag area occupies remaining space before close button
                let reserve = 36.0;
                let avail = (ui.available_width() - reserve).max(0.0);
                if avail > 4.0 {
                    let (_drag_rect, drag_resp) =
                        ui.allocate_exact_size(egui::vec2(avail, 20.0), Sense::drag());
                    if drag_resp.is_pointer_button_down_on() || drag_resp.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                }
                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn = |ui: &mut egui::Ui, label: &str| {
                        let desired = egui::vec2(28.0, 20.0);
                        let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
                        if resp.hovered() {
                            let c = ui.visuals().widgets.hovered.bg_fill;
                            ui.painter().rect_filled(rect.shrink(3.0), 4.0, c);
                        }
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            label,
                            FontId::proportional(14.0),
                            ui.visuals().widgets.inactive.fg_stroke.color,
                        );
                        resp
                    };
                    if btn(ui, "×").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });
}
