#[cfg(target_os = "linux")]
use eframe::egui;

#[cfg(target_os = "linux")]
use std::path::Path;

#[cfg(target_os = "linux")]
fn load_icon_png(ctx: &egui::Context, path: &str) -> Option<egui::TextureHandle> {
    if !Path::new(path).exists() {
        return None;
    }
    match std::fs::read(path) {
        Ok(bytes) => {
            if let Ok(img) = image::load_from_memory(&bytes) {
                let mut rgba = img.to_rgba8();
                if rgba.width() != 32 || rgba.height() != 32 {
                    rgba = image::imageops::resize(
                        &rgba,
                        32,
                        32,
                        image::imageops::FilterType::Lanczos3,
                    );
                }
                let (w, h) = (rgba.width() as usize, rgba.height() as usize);
                let mut pixels = Vec::with_capacity(w * h);
                for chunk in rgba.as_raw().chunks_exact(4) {
                    pixels.push(egui::Color32::from_rgba_unmultiplied(
                        chunk[0], chunk[1], chunk[2], chunk[3],
                    ));
                }
                let color_img = egui::ColorImage {
                    size: [w, h],
                    pixels,
                };
                let tex = ctx.load_texture(
                    "about_em6809_icon_linux",
                    egui::ImageData::Color(color_img.into()),
                    egui::TextureOptions::LINEAR,
                );
                Some(tex)
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Ensure the About icon texture is prepared on Linux.
#[cfg(target_os = "linux")]
pub fn ensure_about_icon(ctx: &egui::Context, tex_slot: &mut Option<egui::TextureHandle>) {
    if tex_slot.is_some() {
        return;
    }
    if let Ok(p) = std::env::var("EM6809_ABOUT_ICON") {
        if !p.is_empty() {
            if let Some(tex) = load_icon_png(ctx, &p) {
                *tex_slot = Some(tex);
            }
        }
    }
    if tex_slot.is_none() {
        for cand in [
            "assets/app_icon_32.png",
            "assets/icon32.png",
            "assets/icon.png",
        ] {
            if let Some(tex) = load_icon_png(ctx, cand) {
                *tex_slot = Some(tex);
                break;
            }
        }
    }
}

/// Show the About dialog on Linux, drawing a vector fallback icon when no texture is available.
#[cfg(target_os = "linux")]
pub fn show_about_dialog(
    ctx: &egui::Context,
    about_open: &mut bool,
    tex: &Option<egui::TextureHandle>,
) {
    if !*about_open {
        return;
    }
    let version = env!("CARGO_PKG_VERSION");
    eframe::egui::Window::new("About em6809")
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .default_height(180.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(tex) = tex {
                    let img = egui::Image::new(tex).fit_to_exact_size(egui::vec2(32.0, 32.0));
                    ui.add(img);
                } else {
                    // Vector fallback: mid-gray rounded-rect background + centered lowercase 'e'
                    let desired = egui::vec2(32.0, 32.0);
                    let (rect, _resp) = ui.allocate_exact_size(desired, egui::Sense::hover());
                    let base = egui::Color32::from_rgb(120, 128, 140);
                    let fg = egui::Color32::from_rgb(30, 30, 30);
                    let rounding = 10.0;
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, rounding, base);
                    let font = egui::FontId::proportional(26.0);
                    let pos = egui::pos2(rect.center().x, rect.center().y - 3.0);
                    painter.text(pos, egui::Align2::CENTER_CENTER, "e", font, fg);
                }
                ui.heading("em6809");
            });
            ui.separator();
            ui.add(egui::Label::new(format!("Version: {}", version)).wrap());
            ui.add_space(8.0);
            ui.add(egui::Label::new("Copyright (c) 2025 hha0x617").wrap());
            ui.add_space(8.0);
            ui.add(egui::Label::new("Dual-licensed: MIT OR Apache-2.0").wrap());
            ui.add_space(8.0);
            ui.add(
                egui::Label::new("See LICENSE-MIT, LICENSE-APACHE, and NOTICE (if present)").wrap(),
            );
            ui.add_space(8.0);
            ui.add(
                egui::Label::new("This software is provided \"AS IS\" without warranty.").wrap(),
            );
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("OK").clicked() {
                    *about_open = false;
                }
            });
        });
}
