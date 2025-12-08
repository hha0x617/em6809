use core::fmt::Write as FmtWrite;
use eframe::egui;
use once_cell::sync::OnceCell;
use std::sync::atomic::Ordering;

use super::SHOW_ORIGINS_OVERRIDE;

pub fn parse_num_u16_any_gui(s: &str) -> Option<u16> {
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
    u32::from_str_radix(digits, base)
        .ok()
        .and_then(|v| if v <= 0xFFFF { Some(v as u16) } else { None })
}

pub fn parse_num_usize_any_gui(s: &str) -> Option<usize> {
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

pub fn parse_u16_any(s: &str) -> Option<u16> {
    let sl = s.trim();
    if sl.is_empty() {
        return None;
    }
    let (base, digits) = if let Some(rest) = sl.strip_prefix("0x").or(sl.strip_prefix("0X")) {
        (16, rest)
    } else if let Some(rest) = sl.strip_prefix("0o").or(sl.strip_prefix("0O")) {
        (8, rest)
    } else if let Some(rest) = sl.strip_prefix("0b").or(sl.strip_prefix("0B")) {
        (2, rest)
    } else {
        (10, sl)
    };
    u32::from_str_radix(digits, base)
        .ok()
        .and_then(|v| if v <= 0xFFFF { Some(v as u16) } else { None })
}

pub fn format_color_hex(c: egui::Color32) -> String {
    format!("0x{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

pub fn parse_color_hex(s: &str) -> Option<egui::Color32> {
    let sl = s.trim();
    let digits = if let Some(rest) = sl.strip_prefix("0x").or(sl.strip_prefix("0X")) {
        rest
    } else if let Some(rest) = sl.strip_prefix('#') {
        rest
    } else {
        sl
    };
    if digits.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&digits[0..2], 16).ok()?;
    let g = u8::from_str_radix(&digits[2..4], 16).ok()?;
    let b = u8::from_str_radix(&digits[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}

pub fn settings_origin_badges_enabled() -> bool {
    static SHOW: OnceCell<bool> = OnceCell::new();
    if let Some(flag) = SHOW_ORIGINS_OVERRIDE.get() {
        return flag.load(Ordering::SeqCst);
    }
    *SHOW.get_or_init(|| {
        let env_enabled = std::env::var("EM6809_SHOW_SETTINGS_ORIGINS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let cli_enabled = std::env::args().any(|arg| arg == "--show-settings-origins");
        env_enabled || cli_enabled
    })
}

pub fn draw_origin_badge(
    ui: &mut egui::Ui,
    origins: &std::collections::HashMap<String, u8>,
    key: &str,
) {
    if !settings_origin_badges_enabled() {
        return;
    }
    use egui::{Color32, Rounding, Stroke, TextStyle, Vec2};
    let (label, bg) = match origins.get(key).copied().unwrap_or(0) {
        2 => ("CLI", Color32::from_rgb(220, 80, 60)),
        1 => ("Saved", Color32::from_rgb(70, 130, 200)),
        _ => ("Default", Color32::from_gray(110)),
    };
    let text_color = Color32::WHITE;
    let padding = Vec2::new(6.0, 2.0);
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        TextStyle::Small.resolve(ui.style()),
        text_color,
    );
    let size = galley.size() + padding * 2.0;
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, Rounding::same(6.0), bg);
    let border = Stroke {
        width: 1.0,
        color: bg.gamma_multiply(0.8),
    };
    ui.painter().rect_stroke(rect, Rounding::same(6.0), border);
    ui.painter()
        .galley(rect.left_top() + padding, galley, egui::Color32::WHITE);
}

pub fn format_memory_line(addr: u16, bytes: &[u8]) -> String {
    let mut s = String::with_capacity(80);
    let _ = write!(&mut s, "{addr:04X}: ");
    for i in 0..16 {
        if i < bytes.len() {
            let b = bytes[i];
            let _ = write!(&mut s, "{b:02X} ");
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
            let _ = write!(&mut s, "{c}");
        } else {
            let _ = write!(&mut s, " ");
        }
    }
    s
}

pub fn format_memory_dump((start, bytes): (u16, &[u8])) -> String {
    let mut out = String::new();
    let mut off = 0usize;
    while off < bytes.len() {
        let chunk = &bytes[off..bytes.len().min(off + 16)];
        let line = format_memory_line(start.wrapping_add(off as u16), chunk);
        out.push_str(&line);
        out.push('\n');
        off += 16;
    }
    out
}
