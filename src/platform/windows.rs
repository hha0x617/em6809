#![allow(unused_imports)]

#[cfg(windows)]
pub mod native_settings;

#[cfg(windows)]
use eframe::egui;

/// Capture the current window's HICON and convert to an egui texture.
#[cfg(windows)]
pub fn ensure_about_icon(ctx: &egui::Context, tex_slot: &mut Option<egui::TextureHandle>) {
    if tex_slot.is_some() {
        return;
    }
    unsafe fn load_hicon_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
        use windows_sys::Win32::Graphics::Gdi::*;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut hicon: HICON = SendMessageW(hwnd, WM_GETICON, ICON_SMALL as usize, 0) as HICON;
        if hicon.is_null() {
            hicon = SendMessageW(hwnd, WM_GETICON, ICON_BIG as usize, 0) as HICON;
        }
        if hicon.is_null() {
            hicon = GetClassLongPtrW(hwnd, GCLP_HICONSM) as HICON;
        }
        if hicon.is_null() {
            hicon = GetClassLongPtrW(hwnd, GCLP_HICON) as HICON;
        }
        if hicon.is_null() {
            return None;
        }
        let size: i32 = 32;
        let mut bmi: BITMAPINFO = core::mem::zeroed();
        bmi.bmiHeader.biSize = core::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = size;
        bmi.bmiHeader.biHeight = -size; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;
        let hdc = CreateCompatibleDC(core::ptr::null_mut());
        if hdc.is_null() {
            return None;
        }
        let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
        let hbm = CreateDIBSection(
            hdc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            core::ptr::null_mut(),
            0,
        );
        if hbm.is_null() || bits.is_null() {
            unsafe {
                DeleteDC(hdc);
            }
            return None;
        }
        let old = unsafe { SelectObject(hdc, hbm as HGDIOBJ) };
        let ok = unsafe {
            DrawIconEx(
                hdc,
                0,
                0,
                hicon,
                size,
                size,
                0,
                core::ptr::null_mut(),
                DI_NORMAL,
            )
        } != 0;
        let mut pixels = vec![egui::Color32::TRANSPARENT; (size * size) as usize];
        if ok {
            let slice = unsafe {
                core::slice::from_raw_parts(bits as *const u8, (size * size * 4) as usize)
            };
            for i in 0..(size * size) as usize {
                let b = slice[i * 4];
                let g = slice[i * 4 + 1];
                let r = slice[i * 4 + 2];
                let a = slice[i * 4 + 3];
                pixels[i] = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
            }
        }
        unsafe {
            SelectObject(hdc, old);
            DeleteObject(hbm as HGDIOBJ);
            DeleteDC(hdc);
        }
        if ok {
            let img = egui::ColorImage {
                size: [size as usize, size as usize],
                pixels,
            };
            let tex = ctx.load_texture(
                "about_em6809_icon_win",
                egui::ImageData::Color(img.into()),
                egui::TextureOptions::LINEAR,
            );
            Some(tex)
        } else {
            None
        }
    }
    *tex_slot = unsafe { load_hicon_texture(ctx) };
}

/// Show the Windows About window with optional icon texture.
#[cfg(windows)]
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
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.horizontal(|ui| {
                if let Some(tex) = tex {
                    let img = egui::Image::new(tex).fit_to_exact_size(egui::vec2(32.0, 32.0));
                    ui.add(img);
                }
                ui.heading("em6809");
            });
            ui.separator();
            ui.add(egui::Label::new(format!("Version: {}", version)));
            ui.add_space(8.0);
            ui.add(egui::Label::new("Copyright (c) 2025 hha0x617"));
            ui.add_space(8.0);
            ui.add(egui::Label::new("Dual-licensed: MIT OR Apache-2.0"));
            ui.add_space(8.0);
            ui.add(
                egui::Label::new("See LICENSE-MIT, LICENSE-APACHE, and NOTICE (if present)"),
            );
            ui.add_space(8.0);
            ui.add(
                egui::Label::new("This software is provided \"AS IS\" without warranty."),
            );
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("OK").clicked() {
                    *about_open = false;
                }
            });
        });
}
