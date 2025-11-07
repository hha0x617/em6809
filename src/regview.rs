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

mod gui {
    use super::*;
    use eframe::{egui, App};
    use egui_extras::{Column, TableBuilder};
    use memmap2::Mmap;
    use std::fs::File;
    use std::path::PathBuf;
    use std::collections::HashMap;
    use i18n_embed_fl::fl;
    use crate::i18n::LOADER;
    
    #[cfg(target_os = "macos")]
    #[allow(unexpected_cfgs)]
    fn set_macos_app_menu_title(name: &str) {
        unsafe {
            use objc::{class, msg_send, sel, sel_impl};
            use objc::runtime::Object;
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            if app.is_null() { return; }
            let main_menu: *mut Object = msg_send![app, mainMenu];
            if main_menu.is_null() { return; }
            let item0: *mut Object = msg_send![main_menu, itemAtIndex: 0i64];
            if item0.is_null() { return; }
            let cstr = std::ffi::CString::new(name).unwrap_or_else(|_| std::ffi::CString::new("em6809").unwrap());
            let ns_name: *mut Object = msg_send![class!(NSString), stringWithUTF8String: cstr.as_ptr()];
            let _: () = msg_send![item0, setTitle: ns_name];
        }
    }

    #[cfg(target_os = "macos")]
    #[allow(unexpected_cfgs)]
    mod macos_about {
        use objc::{class, msg_send, sel, sel_impl};
        use objc::runtime::{Object, Sel, Class};
        use objc::declare::ClassDecl;
        use std::sync::atomic::{AtomicBool, Ordering};

        static INSTALLED: AtomicBool = AtomicBool::new(false);
        static mut HELPER_INSTANCE: *mut Object = std::ptr::null_mut();

        unsafe fn nsstring(s: &str) -> *mut Object {
            let c = std::ffi::CString::new(s).unwrap_or_else(|_| std::ffi::CString::new("").unwrap());
            let ns: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()];
            ns
        }

        extern "C" fn show_about(_this: &Object, _sel: Sel, _sender: *mut Object) {
            unsafe {
                // Show a simple modal About dialog via NSAlert with custom credits
                let pool: *mut Object = msg_send![class!(NSAutoreleasePool), new];
                let alert: *mut Object = msg_send![class!(NSAlert), alloc];
                let alert: *mut Object = msg_send![alert, init];
                let _: () = msg_send![alert, setMessageText: nsstring("em6809")];
                let version = env!("CARGO_PKG_VERSION");
                let info = format!(
                    "Version: {ver}\n\nCopyright (c) 2025 hha0x617\n\nDual-licensed: MIT OR Apache-2.0\n\nSee LICENSE-MIT, LICENSE-APACHE, and NOTICE (if present)\n\nThis software is provided \"AS IS\" without warranty.",
                    ver = version
                );
                let _: () = msg_send![alert, setInformativeText: nsstring(&info)];
                let _: () = msg_send![alert, addButtonWithTitle: nsstring("OK")];
                // Set app icon if available
                let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
                let icon: *mut Object = msg_send![app, applicationIconImage];
                if !icon.is_null() { let _: () = msg_send![alert, setIcon: icon]; }
                let _: i64 = msg_send![alert, runModal];
                let _: () = msg_send![alert, release];
                let _: () = msg_send![pool, drain];
            }
        }

        unsafe fn ensure_helper_class() -> &'static Class {
            let name = "Em6809AboutHelper";
            if let Some(cls) = Class::get(name) { return cls; }
            let superclass = Class::get("NSObject").unwrap();
            let mut decl = ClassDecl::new(name, superclass).unwrap();
            decl.add_method(sel!(showAbout:), show_about as extern "C" fn(&Object, Sel, *mut Object));
            decl.register()
        }

        unsafe fn ensure_helper_instance() -> *mut Object {
            if HELPER_INSTANCE.is_null() {
                let cls = ensure_helper_class();
                let obj: *mut Object = msg_send![cls, alloc];
                let obj: *mut Object = msg_send![obj, init];
                HELPER_INSTANCE = obj;
            }
            HELPER_INSTANCE
        }

        pub fn install_about_handler_once() {
            if INSTALLED.swap(true, Ordering::SeqCst) { return; }
            unsafe {
                let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
                let main_menu: *mut Object = msg_send![app, mainMenu];
                if main_menu.is_null() { return; }
                let apple_item: *mut Object = msg_send![main_menu, itemAtIndex: 0i64];
                if apple_item.is_null() { return; }
                let submenu: *mut Object = msg_send![apple_item, submenu];
                if submenu.is_null() { return; }
                let count: i64 = msg_send![submenu, numberOfItems];
                let target = ensure_helper_instance();
                for i in 0..count {
                    let item: *mut Object = msg_send![submenu, itemAtIndex: i];
                    if item.is_null() { continue; }
                    let action: Sel = msg_send![item, action];
                    if action == sel!(orderFrontStandardAboutPanel:) {
                        let _: () = msg_send![item, setTarget: target];
                        let _: () = msg_send![item, setAction: sel!(showAbout:)];
                        break;
                    }
                }
            }
        }
    }

    #[derive(Clone, Debug)]
    pub enum Command {
        SetPc(u16),
        RunUntil(u16),
        ApplyRegs(RegSnapshot),
        MemWrite(Vec<(u16, u8)>),
        RefreshHexListing,
        GpioSetOut(u32),
        UiSetGpio(bool, u8, u32, u32),
        UiSetMmu(bool, u8, [u16; 16]),
        UiSetTimer(bool, bool),
        UiSetGpioOnColor(u32),
    }

    pub struct RegApp {
        pub snap: Arc<Mutex<RegSnapshot>>,
        pub paused: Arc<AtomicBool>,
        pub step_requests: Arc<AtomicUsize>,
        pub lst_mmap: Option<Arc<Mmap>>,
        pub lst_text: Option<Arc<Mutex<String>>>,
        pub use_hex: Arc<AtomicBool>,
        pub auto_hex_on_edit: Arc<AtomicBool>,
        pub scroll_to_pc_req: Arc<AtomicBool>,
        pub follow_pc_flag: Arc<AtomicBool>,
        pub span_policy: Arc<AtomicUsize>, // 0=FullSpan, 1=PcWindow
        pub window_len: Arc<AtomicUsize>,  // bytes for PcWindow
        pub cmds: Arc<Mutex<Vec<Command>>>,
        pub ja_font: Option<Arc<Vec<u8>>>,
        pub fonts_applied: bool,
        pub menu_fixed: bool,
        // Windows-only: previously used to override taskbar title; removed to keep caption unchanged
        pub use_japanese: bool,
        pub last_pc_for_scroll: u16,
        #[cfg(windows)]
        pub about_open: bool,
        #[cfg(windows)]
        pub about_icon_tex: Option<egui::TextureHandle>,
        #[cfg(target_os = "linux")]
        pub about_open: bool,
        #[cfg(target_os = "linux")]
        pub about_icon_tex: Option<egui::TextureHandle>,
        // Memory view state
        pub mem_follow_pc: Arc<AtomicBool>,
        pub mem_window_len: Arc<AtomicUsize>,
        pub mem_window_start: Arc<AtomicUsize>,  // u16 widened
        pub mem_buf: Arc<Mutex<(u16, Vec<u8>)>>, // current window snapshot
        pub mem_hl: Arc<Mutex<Option<(u16, usize)>>>, // highlight range
        pub mem_utf8: Arc<AtomicBool>,
        pub mem_jump_text: String,
        // GPIO LED color when ON
        pub gpio_on_color: egui::Color32,
        pub mem_len_text: String,
        pub quit_flag: Arc<AtomicBool>,
        // Numeric UX toggle (DragValue vs TextEdit)
        pub mem_use_drag: bool,
        // Memory edit (phase 1)
        pub mem_edit_mode: bool,
        pub mem_pending: HashMap<u16, u8>,
        pub mem_selected: Option<u16>,
        pub mem_edit_buf: String,
        pub mem_focus_next: bool,
        // Caret boundary tracking for neighbor navigation
        pub mem_last_sel_addr: Option<u16>,
        pub mem_last_at_start: bool,
        pub mem_last_at_end: bool,

        // Editing state for registers and CC flags
        pub edit_mode: bool,
        pub edit_initialized: bool,
        pub edit_a: String,
        pub edit_b: String,
        pub edit_d: String,
        pub edit_x: String,
        pub edit_y: String,
        pub edit_u: String,
        pub edit_s: String,
        pub edit_dp: String,
        pub edit_pc: String,
        pub cc_e: bool,
        pub cc_f: bool,
        pub cc_h: bool,
        pub cc_i: bool,
        pub cc_n: bool,
        pub cc_z: bool,
        pub cc_v: bool,
        pub cc_c: bool,
        pub input_error: Option<String>,
        // GPIO view state (mirrors device snapshot)
        pub gpio_present: bool,
        pub gpio_bits: u8,
        pub gpio_out: u32,
        pub gpio_dir: u32,
        // Emulator -> GUI sync (shared across threads)
        pub gpio_sync: Arc<Mutex<Option<(bool, u8, u32, u32)>>>,
        pub mmu_sync: Arc<Mutex<Option<(bool, u8, [u16; 16])>>>,
        // MMU grid
        pub mmu_present: bool,
        pub mmu_sys_mode: bool,
        pub mmu_task: u8,
        pub mmu_map_prev: [u16; 16],
        pub mmu_map_cur: [u16; 16],
        pub timer_present: bool,
        pub timer_pending: bool,
    }

    impl App for RegApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            // Drain UI sync commands injected by emulator thread
            if let Ok(mut q) = self.cmds.lock() {
                let mut rest: Vec<Command> = Vec::new();
                while let Some(cmd) = q.pop() {
                    match cmd {
                        Command::UiSetGpio(present, bits, out, dir) => {
                            self.gpio_present = present;
                            self.gpio_bits = bits;
                            self.gpio_out = out;
                            self.gpio_dir = dir;
                        }
                        Command::UiSetGpioOnColor(rgb) => {
                            self.gpio_on_color = egui::Color32::from_rgb(((rgb>>16)&0xFF) as u8, ((rgb>>8)&0xFF) as u8, (rgb&0xFF) as u8);
                        }
                        Command::UiSetMmu(sys, task, map) => {
                            self.mmu_present = true;
                            self.mmu_sys_mode = sys;
                            self.mmu_task = task;
                            self.mmu_map_prev = self.mmu_map_cur;
                            self.mmu_map_cur = map;
                        }
                        Command::UiSetTimer(present, pending) => {
                            self.timer_present = present;
                            self.timer_pending = pending;
                        }
                        other => rest.push(other),
                    }
                }
                *q = rest;
            }
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

            // macOS: ensure the application menu (leftmost menu title) shows just "em6809"
            #[cfg(target_os = "macos")]
            if !self.menu_fixed {
                set_macos_app_menu_title("em6809");
                self.menu_fixed = true;
            }

            // macOS: install custom About handler once (shows credits/version/license)
            #[cfg(target_os = "macos")]
            {
                macos_about::install_about_handler_once();
            }

            // (timer indicator top strip removed; shown below MMU panel)

            // Windows: simple menu bar with About
            #[cfg(windows)]
            {
                eframe::egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
                    eframe::egui::menu::bar(ui, |ui| {
                        ui.menu_button("Help", |ui| {
                            if ui.button("About em6809…").clicked() {
                                self.about_open = true;
                                ui.close_menu();
                            }
                        });
                    });
                });
                if self.about_open {
                    if self.about_icon_tex.is_none() {
                        // Try to capture the window titlebar icon (HICON) so About uses the same icon
                        #[allow(unsafe_code)]
                        unsafe fn load_hicon_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
                            use windows_sys::Win32::Graphics::Gdi::*;
                            use windows_sys::Win32::UI::WindowsAndMessaging::*;
                            let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
                            if hwnd == core::ptr::null_mut() { return None; }
                            let mut hicon: HICON = SendMessageW(hwnd, WM_GETICON, ICON_SMALL as usize, 0) as HICON;
                            if hicon == core::ptr::null_mut() { hicon = SendMessageW(hwnd, WM_GETICON, ICON_BIG as usize, 0) as HICON; }
                            if hicon == core::ptr::null_mut() { hicon = GetClassLongPtrW(hwnd, GCLP_HICONSM) as HICON; }
                            if hicon == core::ptr::null_mut() { hicon = GetClassLongPtrW(hwnd, GCLP_HICON) as HICON; }
                            if hicon == core::ptr::null_mut() { return None; }
                            let size: i32 = 32;
                            let mut bmi: BITMAPINFO = core::mem::zeroed();
                            bmi.bmiHeader.biSize = core::mem::size_of::<BITMAPINFOHEADER>() as u32;
                            bmi.bmiHeader.biWidth = size;
                            bmi.bmiHeader.biHeight = -size; // top-down
                            bmi.bmiHeader.biPlanes = 1;
                            bmi.bmiHeader.biBitCount = 32;
                            bmi.bmiHeader.biCompression = BI_RGB as u32;
                            let hdc = CreateCompatibleDC(core::ptr::null_mut());
                            if hdc == core::ptr::null_mut() { return None; }
                            let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
                            let hbm = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, core::ptr::null_mut(), 0);
                            if hbm == core::ptr::null_mut() || bits.is_null() { DeleteDC(hdc); return None; }
                            let old = SelectObject(hdc, hbm as HGDIOBJ);
                            let ok = DrawIconEx(hdc, 0, 0, hicon, size, size, 0, core::ptr::null_mut(), DI_NORMAL) != 0;
                            let mut pixels = vec![egui::Color32::TRANSPARENT; (size * size) as usize];
                            if ok {
                                let slice = core::slice::from_raw_parts(bits as *const u8, (size * size * 4) as usize);
                                for i in 0..(size * size) as usize {
                                    let b = slice[i * 4 + 0];
                                    let g = slice[i * 4 + 1];
                                    let r = slice[i * 4 + 2];
                                    let a = slice[i * 4 + 3];
                                    pixels[i] = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
                                }
                            }
                            SelectObject(hdc, old);
                            DeleteObject(hbm as HGDIOBJ);
                            DeleteDC(hdc);
                            if ok {
                                let img = egui::ColorImage { size: [size as usize, size as usize], pixels };
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
                        self.about_icon_tex = unsafe { load_hicon_texture(ctx) };
                    }
                    let version = env!("CARGO_PKG_VERSION");
                    eframe::egui::Window::new("About em6809")
                        .collapsible(false)
                        .resizable(false)
                        .default_width(420.0)
                        .default_height(180.0)
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                if let Some(tex) = &self.about_icon_tex {
                                    let img = egui::Image::new(tex)
                                        .fit_to_exact_size(egui::vec2(32.0, 32.0));
                                    ui.add(img);
                                }
                                ui.heading("em6809");
                            });
                            ui.separator();
                            ui.add(egui::Label::new(format!("Version: {}", version)).wrap(false));
                            ui.add_space(8.0);
                            ui.add(egui::Label::new("Copyright (c) 2025 hha0x617").wrap(false));
                            ui.add_space(8.0);
                            ui.add(egui::Label::new("Dual-licensed: MIT OR Apache-2.0").wrap(false));
                            ui.add_space(8.0);
                            ui.add(egui::Label::new("See LICENSE-MIT, LICENSE-APACHE, and NOTICE (if present)").wrap(false));
                            ui.add_space(8.0);
                            ui.add(egui::Label::new("This software is provided \"AS IS\" without warranty.").wrap(false));
                            ui.add_space(12.0);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("OK").clicked() { self.about_open = false; }
                            });
                        });
                }
            }

            // Linux: simple menu bar with About (mirrors Windows content, no icon capture)
            #[cfg(target_os = "linux")]
            {
                eframe::egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
                    eframe::egui::menu::bar(ui, |ui| {
                        ui.menu_button("Help", |ui| {
                            if ui.button("About em6809…").clicked() {
                                self.about_open = true;
                                ui.close_menu();
                            }
                        });
                    });
                });
                if self.about_open {
                    if self.about_icon_tex.is_none() {
                        // Try to load an external PNG so Linux can match the Windows About icon.
                        // Priority: EM6809_ABOUT_ICON env -> assets/app_icon_32.png -> assets/icon32.png -> assets/icon.png
                        use std::path::Path;
                        let mut loaded = false;
                        fn load_icon_png(ctx: &egui::Context, path: &str) -> Option<egui::TextureHandle> {
                            if !Path::new(path).exists() { return None; }
                            match std::fs::read(path) {
                                Ok(bytes) => {
                                    if let Ok(img) = image::load_from_memory(&bytes) {
                                        let mut rgba = img.to_rgba8();
                                        // Resize to 32x32 for consistency with Windows About small icon
                                        if rgba.width() != 32 || rgba.height() != 32 {
                                            rgba = image::imageops::resize(&rgba, 32, 32, image::imageops::FilterType::Lanczos3);
                                        }
                                        let (w, h) = (rgba.width() as usize, rgba.height() as usize);
                                        let mut pixels = Vec::with_capacity(w * h);
                                        for chunk in rgba.as_raw().chunks_exact(4) {
                                            pixels.push(egui::Color32::from_rgba_unmultiplied(chunk[0], chunk[1], chunk[2], chunk[3]));
                                        }
                                        let color_img = egui::ColorImage { size: [w, h], pixels };
                                        let tex = ctx.load_texture(
                                            "about_em6809_icon_linux",
                                            egui::ImageData::Color(color_img.into()),
                                            egui::TextureOptions::LINEAR,
                                        );
                                        Some(tex)
                                    } else { None }
                                }
                                Err(_) => None,
                            }
                        }
                        if let Ok(p) = std::env::var("EM6809_ABOUT_ICON") {
                            if !p.is_empty() {
                                if let Some(tex) = load_icon_png(ctx, &p) { self.about_icon_tex = Some(tex); loaded = true; }
                            }
                        }
                        if !loaded {
                            for cand in [
                                "assets/app_icon_32.png",
                                "assets/icon32.png",
                                "assets/icon.png",
                            ] {
                                if let Some(tex) = load_icon_png(ctx, cand) { self.about_icon_tex = Some(tex); loaded = true; break; }
                            }
                        }
                        // If not loaded, we'll draw a vector fallback (rounded-rect + 'e') directly in the UI below.
                    }
                    let version = env!("CARGO_PKG_VERSION");
                    eframe::egui::Window::new("About em6809")
                        .collapsible(false)
                        .resizable(false)
                        .default_width(420.0)
                        .default_height(180.0)
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                if let Some(tex) = &self.about_icon_tex {
                                    let img = egui::Image::new(tex)
                                        .fit_to_exact_size(egui::vec2(32.0, 32.0));
                                    ui.add(img);
                                } else {
                                    // Vector fallback: mid-gray rounded-rect background + centered lowercase 'e'
                                    let desired = egui::vec2(32.0, 32.0);
                                    let (rect, _resp) = ui.allocate_exact_size(desired, egui::Sense::hover());
                                    let base = egui::Color32::from_rgb(120, 128, 140); // slightly darker mid-gray (step 2)
                                    let fg = egui::Color32::from_rgb(30, 30, 30);
                                    let rounding = 10.0;

                                    let painter = ui.painter_at(rect);
                                    painter.rect_filled(rect, rounding, base);
                                    let font = egui::FontId::proportional(26.0); // larger 'e'
                                    let pos = egui::pos2(rect.center().x, rect.center().y - 2.0); // shift slightly upward
                                    painter.text(
                                        pos,
                                        egui::Align2::CENTER_CENTER,
                                        "e",
                                        font,
                                        fg,
                                    );
                                }
                                ui.heading("em6809");
                            });
                            ui.separator();
                            ui.add(egui::Label::new(format!("Version: {}", version)).wrap(false));
                            ui.add_space(8.0);
                            ui.add(egui::Label::new("Copyright (c) 2025 hha0x617").wrap(false));
                            ui.add_space(8.0);
                            ui.add(egui::Label::new("Dual-licensed: MIT OR Apache-2.0").wrap(false));
                            ui.add_space(8.0);
                            ui.add(egui::Label::new("See LICENSE-MIT, LICENSE-APACHE, and NOTICE (if present)").wrap(false));
                            ui.add_space(8.0);
                            ui.add(egui::Label::new("This software is provided \"AS IS\" without warranty.").wrap(false));
                            ui.add_space(12.0);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("OK").clicked() { self.about_open = false; }
                            });
                        });
                }
            }

            // Windows: no title override to keep window caption unchanged

            // If external quit is requested (e.g., Ctrl+C), close the window
            if self.quit_flag.load(Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            // Editing is user-controlled via Edit/Apply toggle (not tied to pause)
            let editing = self.edit_mode;
            // Initialize edit buffers from current snapshot the first time we enter editing mode
            if editing {
                if !self.edit_initialized {
                    if let Ok(s) = self.snap.lock() {
                        self.edit_a = format!("0x{:02X}", s.a);
                        self.edit_b = format!("0x{:02X}", s.b);
                        self.edit_d = format!("0x{:04X}", s.d);
                        self.edit_x = format!("0x{:04X}", s.x);
                        self.edit_y = format!("0x{:04X}", s.y);
                        self.edit_u = format!("0x{:04X}", s.u);
                        self.edit_s = format!("0x{:04X}", s.s);
                        self.edit_dp = format!("0x{:02X}", s.dp);
                        self.edit_pc = format!("0x{:04X}", s.pc);
                        self.cc_e = (s.cc & 0x80) != 0;
                        self.cc_f = (s.cc & 0x40) != 0;
                        self.cc_h = (s.cc & 0x20) != 0;
                        self.cc_i = (s.cc & 0x10) != 0;
                        self.cc_n = (s.cc & 0x08) != 0;
                        self.cc_z = (s.cc & 0x04) != 0;
                        self.cc_v = (s.cc & 0x02) != 0;
                        self.cc_c = (s.cc & 0x01) != 0;
                    }
                    self.input_error = None;
                    self.edit_initialized = true;
                }
            } else if self.edit_initialized {
                // When leaving editing mode, reset so we sync from latest snapshot next pause
                self.edit_initialized = false;
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
                ui.heading(fl!(LOADER, "memory"));
                ui.separator();
                if ui
                    .checkbox(
                        &mut follow_pc,
                        fl!(LOADER, "follow-pc"),
                    )
                    .clicked()
                {
                    self.mem_follow_pc.store(follow_pc, Ordering::SeqCst);
                }
                let mut utf8 = self.mem_utf8.load(Ordering::SeqCst);
                if ui.checkbox(&mut utf8, "UTF-8").clicked() {
                    self.mem_utf8.store(utf8, Ordering::SeqCst);
                }
                let mut use_drag = self.mem_use_drag;
                if ui.checkbox(&mut use_drag, fl!(LOADER, "numeric")).clicked() {
                    self.mem_use_drag = use_drag;
                }
                ui.separator();
                // Edit mode controls: in normal mode show only "Edit"; in edit mode show "Apply" and "Cancel".
                let pending_count = self.mem_pending.len();
                if !self.mem_edit_mode {
                    if ui.button(fl!(LOADER, "edit")).clicked() {
                        self.mem_edit_mode = true;
                        self.mem_selected = None;
                        self.mem_edit_buf.clear();
                    }
                } else {
                    let apply_enabled = pending_count > 0;
                    if ui.add_enabled(apply_enabled, egui::Button::new(fl!(LOADER, "apply"))).clicked() {
                        let mut items: Vec<(u16, u8)> = self.mem_pending.iter().map(|(a, v)| (*a, *v)).collect();
                        items.sort_by_key(|(a, _)| *a);
                        if let Ok(mut q) = self.cmds.lock() { q.push(Command::MemWrite(items)); }
                        self.mem_pending.clear();
                        self.mem_selected = None;
                        self.mem_edit_buf.clear();
                        // Exit edit mode after apply as requested
                        self.mem_edit_mode = false;
                        // Reset caret boundary tracking
                        self.mem_last_sel_addr = None;
                        self.mem_last_at_start = false;
                        self.mem_last_at_end = false;
                    }
                    if ui.button(fl!(LOADER, "cancel")).clicked() {
                        // Cancel discards pending edits and exits edit mode
                        self.mem_pending.clear();
                        self.mem_selected = None;
                        self.mem_edit_buf.clear();
                        self.mem_edit_mode = false;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(fl!(LOADER, "start"));
                if self.mem_use_drag {
                    let mut sv: u16 = self.mem_window_start.load(Ordering::SeqCst) as u16;
                            let resp = ui.add_enabled(!follow_pc, egui::DragValue::new(&mut sv).clamp_range(0u16..=0xFFFF).speed(16.0));
                            ui.monospace(format!(" (0x{:04X})", sv));
                            if resp.changed() && !follow_pc {
                                start = sv;
                                self.mem_window_start.store(sv as usize, Ordering::SeqCst);
                                self.mem_jump_text = format!("0x{:04X}", sv);
                            }
                        } else {
                            let te = egui::TextEdit::singleline(&mut self.mem_jump_text)
                                .desired_width(80.0)
                                .hint_text("0x0000");
                            // Disable editing of Start when following PC
                            let start_resp = ui.add_enabled(!follow_pc, te);
                            // Commit Start: Enter while focused or on focus loss
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
                        }
                        ui.label(fl!(LOADER, "len"));
                        if self.mem_use_drag {
                            let mut lv: usize = self.mem_window_len.load(Ordering::SeqCst);
                            let old = lv;
                            let resp = ui.add(egui::DragValue::new(&mut lv).clamp_range(16..=4096).speed(1.0));
                            ui.label(fl!(LOADER, "bytes"));
                            if resp.changed() && lv != old {
                                let v = lv.clamp(16, 4096);
                                self.mem_window_len.store(v, Ordering::SeqCst);
                                self.mem_len_text = format!("{}", v);
                            }
                        } else {
                            let len_te = egui::TextEdit::singleline(&mut self.mem_len_text)
                                .desired_width(64.0)
                                .hint_text("256");
                            let len_resp = ui.add(len_te);
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
                        }
                        // Jump button removed; Enter/focus-out on Start commits address
                        if ui
                            .button(fl!(LOADER, "copy"))
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
                                            let shown = if let Some(v) = self.mem_pending.get(&a) { *v } else { *b };
                                            // Edit mode: selected cell becomes TextEdit, others clickable labels
                                            if self.mem_edit_mode && self.mem_selected == Some(a) {
                                                // ensure buffer contains current value
                                                if self.mem_edit_buf.is_empty() { self.mem_edit_buf = format!("{:02X}", shown); }
                                                let cell_id = egui::Id::new(("mem_hex_cell", a));
                                                // Revert overwrite mode: no need to snapshot buffer/caret pre-state
                                                let te = egui::TextEdit::singleline(&mut self.mem_edit_buf)
                                                    .desired_width(24.0)
                                                    .id_source(cell_id)
                                                    .cursor_at_end(true);
                                                let resp = ui.add(te);
                                                if self.mem_focus_next { resp.request_focus(); self.mem_focus_next = false; }
                                                // On focus gain (e.g., via Tab/Enter), select all text so即時上書き可能
                                                if resp.gained_focus() {
                                                    if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), resp.id) {
                                                        let len = self.mem_edit_buf.len();
                                                        let range = egui::text::CCursorRange::two(
                                                            egui::text::CCursor::new(0),
                                                            egui::text::CCursor::new(len),
                                                        );
                                                        state.cursor.set_char_range(Some(range));
                                                        state.store(ui.ctx(), resp.id);
                                                    }
                                                }
                                                // Key handling: consume Tab/Enter to avoid default focus changes; arrows remain non-consuming
                                                let (enter, tab_any, shift_held, arrow_up, arrow_down, left_pressed, right_pressed, home_key, end_key) = ui.input_mut(|i| {
                                                    let shift_now = i.modifiers.shift;
                                                    let enter = i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                                                    // Consume both plain Tab and Shift+Tab
                                                    let tab_plain_c = i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                                                    let tab_shift_c = i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab);
                                                    let tab_any = tab_plain_c || tab_shift_c;
                                                    let arrow_up = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                                                    let arrow_down = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                                                    // Consume left/right to detect a single navigation intent this frame
                                                    let left_pressed = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft);
                                                    let right_pressed = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight);
                                                    let home_key = i.key_pressed(egui::Key::Home);
                                                    let end_key = i.key_pressed(egui::Key::End);
                                                    (enter, tab_any, shift_now, arrow_up, arrow_down, left_pressed, right_pressed, home_key, end_key)
                                                });
                                                let shift_tab = tab_any && shift_held;
                                                let tab_plain = tab_any && !shift_held;
                                                // After TextEdit processed keys, re-check caret; only jump if caret stayed at boundary
                                                let (at_start_after, at_end_after) = {
                                                    if let Some(state) = egui::text_edit::TextEditState::load(ui.ctx(), resp.id) {
                                                        if let Some(r) = state.cursor.char_range() {
                                                            let len = self.mem_edit_buf.len();
                                                            let collapsed = r.primary == r.secondary;
                                                            let idx = r.primary.index;
                                                            (collapsed && idx == 0, collapsed && idx == len)
                                                        } else { (false, false) }
                                                    } else { (false, false) }
                                                };
                                                // Reverted overwrite mode: rely on TextEdit's default insertion behavior
                                                // Cross-frame boundary tracking: require端滞在の明確な2度押し
                                                let now_at_start = at_start_after;
                                                let now_at_end = at_end_after;
                                                let same_cell = self.mem_last_sel_addr == Some(a);
                                                let move_prev = same_cell && left_pressed && self.mem_last_at_start && now_at_start;
                                                let move_next = same_cell && right_pressed && self.mem_last_at_end && now_at_end;
                                                let commit = resp.lost_focus() || enter || tab_any || arrow_up || arrow_down || move_prev || move_next || home_key || end_key;
                                                if commit {
                                                    if let Ok(v) = u8::from_str_radix(self.mem_edit_buf.trim(), 16) {
                                                        // Only keep pending entry if it differs from the original byte
                                                        if v != *b { self.mem_pending.insert(a, v); } else { self.mem_pending.remove(&a); }
                                                        // Decide next selection
                                                        let line_base = a & 0xFFF0;
                                                        let next_sel = if home_key { line_base }
                                                            else if end_key { line_base.wrapping_add(15) }
                                                            else if shift_tab || move_prev { a.wrapping_sub(1) }
                                                            else if arrow_up { a.wrapping_sub(16) }
                                                            else if arrow_down { a.wrapping_add(16) }
                                                            else if enter || tab_plain || move_next { a.wrapping_add(1) }
                                                            else { a };
                                                        if next_sel != a {
                                                            self.mem_selected = Some(next_sel);
                                                            self.mem_focus_next = true;
                                                            // Reset boundary tracking so新セルでも2度押しが必要
                                                            self.mem_last_sel_addr = None;
                                                            self.mem_last_at_start = false;
                                                            self.mem_last_at_end = false;
                                                        }
                                                        self.mem_edit_buf.clear();
                                                    } else {
                                                        // keep selection, but reset buffer to current value
                                                        self.mem_edit_buf = format!("{:02X}", shown);
                                                    }
                                                }
                                                // Update boundary tracking for this cell each frame
                                                self.mem_last_sel_addr = Some(a);
                                                self.mem_last_at_start = now_at_start;
                                                self.mem_last_at_end = now_at_end;
                                            } else {
                                                let mut rt = egui::RichText::new(format!("{:02X}", shown)).monospace();
                                                if in_hl { rt = rt.background_color(hl_color); }
                                                if let Some(&nv) = self.mem_pending.get(&a) { if nv != *b { rt = rt.background_color(egui::Color32::from_rgb(255, 255, 128)); } }
                                                let label = egui::Label::new(rt).sense(if self.mem_edit_mode { egui::Sense::click() } else { egui::Sense::hover() });
                                                let resp = ui.add(label);
                                                if self.mem_edit_mode && resp.clicked() {
                                                    self.mem_selected = Some(a);
                                                    self.mem_edit_buf = String::new();
                                                    self.mem_focus_next = true;
                                                    // Reset boundary tracking on new selection
                                                    self.mem_last_sel_addr = None;
                                                    self.mem_last_at_start = false;
                                                    self.mem_last_at_end = false;
                                                }
                                            }
                                            if i != chunk.len() - 1 { ui.monospace(" "); }
                                        }
                                        // pad if fewer than 16 bytes
                                        for _ in chunk.len()..16 { ui.monospace("   "); }
                                        ui.monospace("| ");
                                        // ASCII / UTF-8 per-cell render so only changed bytes get highlight
                                        if utf8_mode {
                                            for (i, ob) in chunk.iter().enumerate() {
                                                let a = addr.wrapping_add(i as u16);
                                                let shown_b = *self.mem_pending.get(&a).unwrap_or(ob);
                                                let ch = if (0x20..=0x7E).contains(&shown_b) { shown_b as char } else { '.' };
                                                let mut rt = egui::RichText::new(format!("{}", ch)).monospace();
                                                if let Some(&nv) = self.mem_pending.get(&a) { if nv != *ob { rt = rt.background_color(egui::Color32::from_rgb(255,255,128)); } }
                                                ui.add(egui::Label::new(rt));
                                            }
                                        } else {
                                            for (i, ob) in chunk.iter().enumerate() {
                                                let a = addr.wrapping_add(i as u16);
                                                let shown_b = *self.mem_pending.get(&a).unwrap_or(ob);
                                                let ch = if (0x20..=0x7E).contains(&shown_b) { shown_b as char } else { '.' };
                                                let mut rt = egui::RichText::new(format!("{}", ch)).monospace();
                                                if let Some(&nv) = self.mem_pending.get(&a) { if nv != *ob { rt = rt.background_color(egui::Color32::from_rgb(255,255,128)); } }
                                                ui.add(egui::Label::new(rt));
                                            }
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
                        let mut auto_hex = self.auto_hex_on_edit.load(Ordering::SeqCst);
                        let mut follow_pc = self.follow_pc_flag.load(Ordering::SeqCst);
                        let mut policy = self.span_policy.load(Ordering::SeqCst);
                        let mut win_len = self.window_len.load(Ordering::SeqCst);
                        ui.horizontal(|ui| {
                            ui.heading(if use_hex_view { fl!(LOADER, "listing-hex") } else { fl!(LOADER, "listing-lst") });
                            ui.separator();
                            if have_lst && have_hex {
                                if ui.checkbox(&mut use_hex_view, fl!(LOADER, "hex-view")).clicked() {
                                    self.use_hex.store(use_hex_view, Ordering::SeqCst);
                                    if follow_pc { self.scroll_to_pc_req.store(true, Ordering::SeqCst); }
                                    // When switching to Hex, request the backend to (re)build the hex listing immediately
                                    if use_hex_view {
                                        if let Ok(mut q) = self.cmds.lock() { q.push(Command::RefreshHexListing); }
                                    }
                                }
                            }
                            if ui.checkbox(&mut auto_hex, fl!(LOADER, "auto-hex-on-edit")).clicked() {
                                self.auto_hex_on_edit.store(auto_hex, Ordering::SeqCst);
                            }
                            if ui.checkbox(&mut follow_pc, fl!(LOADER, "follow-pc")).clicked() {
                                self.follow_pc_flag.store(follow_pc, Ordering::SeqCst);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(fl!(LOADER, "span-mode"));
                            let full = policy == 0;
                            let select_full = full;
                            if ui.radio(select_full, fl!(LOADER, "full-span")).clicked() {
                                policy = 0;
                                self.span_policy.store(0, Ordering::SeqCst);
                            }
                            let select_win = !full;
                            if ui.radio(select_win, fl!(LOADER, "pc-window")).clicked() {
                                policy = 1;
                                self.span_policy.store(1, Ordering::SeqCst);
                            }
                            ui.add(egui::DragValue::new(&mut win_len).clamp_range(16..=4096).speed(1));
                            ui.label(fl!(LOADER, "bytes"));
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
                                        // Avoid highlighting macro definition lines even if they show a PC-like address
                                        let upper = ltrim.to_ascii_uppercase();
                                        let is_macro_line = upper.contains(" MACRO") || upper.contains(" ENDM") || upper.starts_with("MACRO ");
                                        let is_pc_addr_token = addr_for_line.is_some() && token.eq_ignore_ascii_case(&pc_hex) && !is_macro_line;
                            let (label_run_until, label_set_pc, label_cancel) = (
                                fl!(LOADER, "run-to-here"),
                                fl!(LOADER, "set-pc-to-here"),
                                fl!(LOADER, "cancel"),
                            );
                                        if is_pc_addr_token {
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
                                // Auto-scroll when PC changes or a view switch requested a scroll.
                                // Make the scroll request sticky until we successfully scroll to a highlighted row.
                                let need_scroll = self
                                    .scroll_to_pc_req
                                    .load(Ordering::SeqCst)
                                    || (cur_pc != self.last_pc_for_scroll);
                                if follow_pc && need_scroll {
                                    if let Some(rect) = highlight_rect {
                                        ui.scroll_to_rect(rect, Some(egui::Align::Center));
                                        // Clear the request only after performing the scroll, and update last_pc marker.
                                        self.scroll_to_pc_req.store(false, Ordering::SeqCst);
                                        self.last_pc_for_scroll = cur_pc;
                                    }
                                }
                            });
                    });
            }

            // Left/center: registers and controls
            egui::CentralPanel::default().show(ctx, |ui| {
                let s = *self.snap.lock().unwrap();
                ui.heading(fl!(LOADER, "registers-heading"));
                ui.horizontal(|ui| {
                    if self.paused.load(Ordering::SeqCst) {
                        if ui.button(fl!(LOADER, "resume")).clicked() {
                            self.paused.store(false, Ordering::SeqCst);
                        }
                        if ui.button(fl!(LOADER, "step")).clicked() {
                            self.paused.store(true, Ordering::SeqCst);
                            self.step_requests.fetch_add(1, Ordering::SeqCst);
                        }
                    } else {
                        if ui.button(fl!(LOADER, "pause")).clicked() {
                            self.paused.store(true, Ordering::SeqCst);
                        }
                        if ui.button(fl!(LOADER, "step")).clicked() {
                            self.paused.store(true, Ordering::SeqCst);
                            self.step_requests.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                    ui.separator();
                    if self.edit_mode {
                        if ui.button(fl!(LOADER, "apply")).clicked() {
                            // Parse inputs; numbers accept 0x/0o/0b or decimal
                            let parse8 = |s: &str| -> Option<u8> { parse_num_u16_any_gui(s).and_then(|v| if v <= 0xFF { Some(v as u8) } else { None }) };
                            let parse16 = |s: &str| -> Option<u16> { parse_num_u16_any_gui(s) };
                            let a_opt = parse8(&self.edit_a);
                            let b_opt = parse8(&self.edit_b);
                            let d_opt = parse16(&self.edit_d);
                            let x_opt = parse16(&self.edit_x);
                            let y_opt = parse16(&self.edit_y);
                            let u_opt = parse16(&self.edit_u);
                            let s_opt = parse16(&self.edit_s);
                            let dp_opt = parse8(&self.edit_dp);
                            let pc_opt = parse16(&self.edit_pc);
                            let cc_val: u8 = (if self.cc_e { 0x80 } else { 0 })
                                | (if self.cc_f { 0x40 } else { 0 })
                                | (if self.cc_h { 0x20 } else { 0 })
                                | (if self.cc_i { 0x10 } else { 0 })
                                | (if self.cc_n { 0x08 } else { 0 })
                                | (if self.cc_z { 0x04 } else { 0 })
                                | (if self.cc_v { 0x02 } else { 0 })
                                | (if self.cc_c { 0x01 } else { 0 });
                            if a_opt.is_none() || b_opt.is_none() || x_opt.is_none() || y_opt.is_none() || u_opt.is_none() || s_opt.is_none() || dp_opt.is_none() || pc_opt.is_none() {
                                self.input_error = Some(fl!(LOADER, "invalid-reg-values"));
                            } else {
                                // D takes precedence if provided
                                let mut a = a_opt.unwrap();
                                let mut b = b_opt.unwrap();
                                if let Some(dv) = d_opt { a = (dv >> 8) as u8; b = dv as u8; }
                                let snap = RegSnapshot {
                                    a,
                                    b,
                                    d: (((a as u16) << 8) | (b as u16)),
                                    x: x_opt.unwrap(),
                                    y: y_opt.unwrap(),
                                    u: u_opt.unwrap(),
                                    s: s_opt.unwrap(),
                                    dp: dp_opt.unwrap(),
                                    pc: pc_opt.unwrap(),
                                    cc: cc_val,
                                };
                                if let Ok(mut q) = self.cmds.lock() { q.push(Command::ApplyRegs(snap)); }
                                self.input_error = None;
                                // Leave editing mode after successful apply
                                self.edit_mode = false;
                                self.edit_initialized = false;
                            }
                        }
                        if ui.button(fl!(LOADER, "cancel")).clicked() {
                            if let Ok(s2) = self.snap.lock() {
                                self.edit_a = format!("0x{:02X}", s2.a);
                                self.edit_b = format!("0x{:02X}", s2.b);
                                self.edit_d = format!("0x{:04X}", s2.d);
                                self.edit_x = format!("0x{:04X}", s2.x);
                                self.edit_y = format!("0x{:04X}", s2.y);
                                self.edit_u = format!("0x{:04X}", s2.u);
                                self.edit_s = format!("0x{:04X}", s2.s);
                                self.edit_dp = format!("0x{:02X}", s2.dp);
                                self.edit_pc = format!("0x{:04X}", s2.pc);
                                self.cc_e = (s2.cc & 0x80) != 0;
                                self.cc_f = (s2.cc & 0x40) != 0;
                                self.cc_h = (s2.cc & 0x20) != 0;
                                self.cc_i = (s2.cc & 0x10) != 0;
                                self.cc_n = (s2.cc & 0x08) != 0;
                                self.cc_z = (s2.cc & 0x04) != 0;
                                self.cc_v = (s2.cc & 0x02) != 0;
                                self.cc_c = (s2.cc & 0x01) != 0;
                            }
                            self.input_error = None;
                            self.edit_mode = false;
                            self.edit_initialized = false;
                        }
                    } else {
                        if ui.button(fl!(LOADER, "edit")).clicked() {
                            self.edit_mode = true;
                            self.edit_initialized = false;
                        }
                    }
                });
                if let Some(err) = &self.input_error {
                    ui.colored_label(egui::Color32::RED, err);
                }
                ui.separator();
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::auto().at_least(40.0))
                    .column(Column::remainder())
                    .header(22.0, |mut header| {
                        header.col(|ui| {
                            ui.strong(fl!(LOADER, "reg"));
                            let rect = ui.max_rect();
                            let stroke = egui::Stroke::new(
                                1.0,
                                ui.visuals().widgets.noninteractive.bg_stroke.color,
                            );
                            ui.painter()
                                .rect_stroke(rect, egui::Rounding::same(0.0), stroke);
                        });
                        header.col(|ui| {
                            ui.strong(fl!(LOADER, "value"));
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
                        // Editable rows while paused; otherwise show live snapshot values
                        let mut add_row = |name: &str, edit: &mut String, hint: &str, show_val: String| {
                            body.row(22.0, |mut row| {
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
                                    if editing {
                                        let te = egui::TextEdit::singleline(edit)
                                            .desired_width(100.0)
                                            .hint_text(hint);
                                        let _resp = ui.add(te);
                                    } else {
                                        ui.monospace(show_val);
                                    }
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
                        };
                        add_row("A", &mut self.edit_a, "0x00", format!("${:02X}", s.a));
                        add_row("B", &mut self.edit_b, "0x00", format!("${:02X}", s.b));
                        add_row("D", &mut self.edit_d, "0x0000", format!("${:04X}", s.d));
                        add_row("X", &mut self.edit_x, "0x0000", format!("${:04X}", s.x));
                        add_row("Y", &mut self.edit_y, "0x0000", format!("${:04X}", s.y));
                        add_row("U", &mut self.edit_u, "0x0000", format!("${:04X}", s.u));
                        add_row("S", &mut self.edit_s, "0x0000", format!("${:04X}", s.s));
                        add_row("DP", &mut self.edit_dp, "0x00", format!("${:02X}", s.dp));
                        add_row("PC", &mut self.edit_pc, "0x0000", format!("${:04X}", s.pc));

                        // CC flags as toggles
                        body.row(24.0, |mut row| {
                            row.col(|ui| {
                                ui.monospace("CC");
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
                                if editing {
                                    ui.horizontal(|ui| {
                                        ui.toggle_value(&mut self.cc_e, "E");
                                        ui.toggle_value(&mut self.cc_f, "F");
                                        ui.toggle_value(&mut self.cc_h, "H");
                                        ui.toggle_value(&mut self.cc_i, "I");
                                        ui.toggle_value(&mut self.cc_n, "N");
                                        ui.toggle_value(&mut self.cc_z, "Z");
                                        ui.toggle_value(&mut self.cc_v, "V");
                                        ui.toggle_value(&mut self.cc_c, "C");
                                    });
                                } else {
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
                                    ui.monospace(cc_text);
                                }
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
                    });

                // GPIO LEDs (simple quick panel)
                if self.gpio_present {
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        ui.label("GPIO LEDs:");
                        let bits = self.gpio_bits as usize;
                        let mut new_out = self.gpio_out;
                        for i in 0..bits {
                            let is_out = ((self.gpio_dir >> i) & 1) != 0;
                            let on = ((new_out >> i) & 1) != 0;
                            let color = if is_out { if on { self.gpio_on_color } else { egui::Color32::DARK_GRAY } } else { egui::Color32::GRAY };
                            let label = format!("{:02}", i);
                            let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                            ui.painter().rect_filled(rect.shrink(2.0), 4.0, color);
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::TextStyle::Small.resolve(ui.style()), egui::Color32::BLACK);
                            if is_out && resp.clicked() { new_out ^= 1u32 << i; }
                        }
                        if new_out != self.gpio_out { self.gpio_out = new_out; if let Ok(mut q) = self.cmds.lock() { q.push(Command::GpioSetOut(new_out)); } }
                    });
                }

                if self.mmu_present {
                    ui.separator();
                    let title = if self.mmu_sys_mode { format!("MMU map (SYS) task{}", self.mmu_task) } else { format!("MMU map (USER) task{}", self.mmu_task) };
                    ui.monospace(title);
                    ui.horizontal_wrapped(|ui| {
                        for p in 0..16 {
                            let f = self.mmu_map_cur[p];
                            let prev = self.mmu_map_prev[p];
                            let changed = f != prev;
                            let text = format!("P{:X}:F{:02X}", p, f & 0xFF);
                            let (rect, _resp) = ui.allocate_exact_size(egui::vec2(70.0, 20.0), egui::Sense::hover());
                            let bg = if changed { egui::Color32::DARK_GREEN } else { egui::Color32::DARK_GRAY };
                            ui.painter().rect_filled(rect.shrink(2.0), 3.0, bg);
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::TextStyle::Monospace.resolve(ui.style()), egui::Color32::WHITE);
                        }
                    });
                    if self.timer_present {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.monospace("Timer:");
                            let color = if self.timer_pending { egui::Color32::LIGHT_RED } else { egui::Color32::DARK_GREEN };
                            let (rect, _resp) = ui.allocate_exact_size(egui::vec2(60.0, 18.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect.shrink(3.0), 6.0, color);
                            let label = if self.timer_pending { "PENDING" } else { "idle" };
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::TextStyle::Monospace.resolve(ui.style()), egui::Color32::BLACK);
                        });
                    }
                }

                // If MMU is not shown, still show standalone Timer indicator
                if !self.mmu_present && self.timer_present {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.monospace("Timer:");
                        let color = if self.timer_pending { egui::Color32::LIGHT_RED } else { egui::Color32::DARK_GREEN };
                        let (rect, _resp) = ui.allocate_exact_size(egui::vec2(60.0, 18.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect.shrink(3.0), 6.0, color);
                        let label = if self.timer_pending { "PENDING" } else { "idle" };
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::TextStyle::Monospace.resolve(ui.style()), egui::Color32::BLACK);
                    });
                }

                // Error display moved next to the Edit/Apply controls at the top

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
        auto_hex_on_edit: Arc<AtomicBool>,
        scroll_to_pc_req: Arc<AtomicBool>,
        follow_pc_flag: Arc<AtomicBool>,
        span_policy: Arc<AtomicUsize>,
        window_len: Arc<AtomicUsize>,
        cmds: Arc<Mutex<Vec<Command>>>,
        ja_font: Option<Arc<Vec<u8>>>,
        _handle: Option<std::thread::JoinHandle<()>>,
        // Memory view shared state
        mem_follow_pc: Arc<AtomicBool>,
        mem_window_len: Arc<AtomicUsize>,
        mem_window_start: Arc<AtomicUsize>,
        mem_buf: Arc<Mutex<(u16, Vec<u8>)>>,
        mem_hl: Arc<Mutex<Option<(u16, usize)>>>,
        mem_utf8: Arc<AtomicBool>,
        quit_flag: Arc<AtomicBool>,
        alive: Arc<AtomicBool>,
        started: Arc<AtomicBool>,
        // Emulator->GUI sync
        gpio_sync: Arc<Mutex<Option<(bool, u8, u32, u32)>>>,
        mmu_sync: Arc<Mutex<Option<(bool, u8, [u16; 16])>>>,
    }

    impl Clone for RegView {
        fn clone(&self) -> Self {
            Self {
                snapshot: Arc::clone(&self.snapshot),
                paused: Arc::clone(&self.paused),
                step_requests: Arc::clone(&self.step_requests),
                lst_mmap: self.lst_mmap.as_ref().map(Arc::clone),
                lst_text: self.lst_text.as_ref().map(Arc::clone),
                use_hex: Arc::clone(&self.use_hex),
                auto_hex_on_edit: Arc::clone(&self.auto_hex_on_edit),
                scroll_to_pc_req: Arc::clone(&self.scroll_to_pc_req),
                follow_pc_flag: Arc::clone(&self.follow_pc_flag),
                span_policy: Arc::clone(&self.span_policy),
                window_len: Arc::clone(&self.window_len),
                cmds: Arc::clone(&self.cmds),
                ja_font: self.ja_font.as_ref().map(Arc::clone),
                _handle: None,
                mem_follow_pc: Arc::clone(&self.mem_follow_pc),
                mem_window_len: Arc::clone(&self.mem_window_len),
                mem_window_start: Arc::clone(&self.mem_window_start),
                mem_buf: Arc::clone(&self.mem_buf),
                mem_hl: Arc::clone(&self.mem_hl),
                mem_utf8: Arc::clone(&self.mem_utf8),
                quit_flag: Arc::clone(&self.quit_flag),
                alive: Arc::clone(&self.alive),
                started: Arc::clone(&self.started),
                gpio_sync: Arc::new(Mutex::new(None)),
                mmu_sync: Arc::new(Mutex::new(None)),
            }
        }
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
            let auto_hex_on_edit = Arc::new(AtomicBool::new(true));
            let scroll_to_pc_req = Arc::new(AtomicBool::new(false));
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
                            // English-name Hiragino (newer macOS)
                            "/System/Library/Fonts/Hiragino Sans W6.ttc",
                            "/System/Library/Fonts/Hiragino Sans W3.ttc",
                            "/Library/Fonts/Hiragino Sans W6.ttc",
                            "/Library/Fonts/Hiragino Sans W3.ttc",
                            // Older ProN names (may be installed in /Library/Fonts)
                            "/Library/Fonts/Hiragino Kaku Gothic ProN W6.otf",
                            "/Library/Fonts/Hiragino Kaku Gothic ProN W3.otf",
                            // Japanese-name variants (some macOS locales)
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
            let quit_flag_thread = Arc::new(AtomicBool::new(false));
            let quit_flag_thread_clone = quit_flag_thread.clone();
            let alive_flag = Arc::new(AtomicBool::new(false));
            let alive_flag_thread = alive_flag.clone();
            let started_flag = Arc::new(AtomicBool::new(false));
            let started_flag_thread = started_flag.clone();
            let auto_hex_on_edit_thread = Arc::clone(&auto_hex_on_edit);
            let scroll_to_pc_req_thread = Arc::clone(&scroll_to_pc_req);
            let handle = std::thread::spawn(move || {
                let mut native_options = eframe::NativeOptions::default();
                // Make the window compact and appropriate for the register grid
                let base = eframe::egui::ViewportBuilder::default()
                    .with_inner_size(eframe::egui::vec2(1800.0, 700.0))
                    .with_min_inner_size(eframe::egui::vec2(900.0, 400.0))
                    .with_resizable(true);
                #[cfg(all(unix, not(target_os = "macos")))]
                let vp = base.with_app_id("em6809");
                #[cfg(not(all(unix, not(target_os = "macos"))))]
                let vp = base;
                native_options.viewport = vp;
                // On Windows, winit requires the event loop to be created on the main thread by default.
                // Allow creating it on this background thread to avoid a panic.
                #[cfg(windows)]
                {
                    native_options.event_loop_builder = Some(Box::new(|builder| {
                        use winit::platform::windows::EventLoopBuilderExtWindows;
                        builder.with_any_thread(true);
                    }));
                }
                // Note: On some Unix configurations (e.g., WSL2/Wayland), winit requires
                // the event loop to live on the main thread. If creating on a background
                // thread panics, we catch it below and continue without the GUI.
            let app = RegApp {
                snap: snap_clone,
                paused: paused_clone,
                step_requests: step_requests_clone,
                lst_mmap: lst_clone,
                lst_text: lst_text_clone,
                use_hex: use_hex_clone,
                auto_hex_on_edit: Arc::clone(&auto_hex_on_edit_thread),
                scroll_to_pc_req: Arc::clone(&scroll_to_pc_req_thread),
                follow_pc_flag: follow_clone,
                span_policy: span_policy_clone,
                window_len: window_len_clone,
                cmds: cmds_clone,
                ja_font: ja_font_clone,
                    fonts_applied: false,
                    menu_fixed: false,
                    use_japanese,
                    last_pc_for_scroll: 0,
                    #[cfg(windows)]
                    about_open: false,
                    #[cfg(windows)]
                    about_icon_tex: None,
                    #[cfg(target_os = "linux")]
                    about_open: false,
                    #[cfg(target_os = "linux")]
                    about_icon_tex: None,
                    mem_follow_pc: mem_follow_pc_thread.clone(),
                    mem_window_len: mem_window_len_thread.clone(),
                    mem_window_start: mem_window_start_thread.clone(),
                    mem_buf: mem_buf_thread.clone(),
                    mem_hl: mem_hl_thread.clone(),
                    mem_utf8: mem_utf8_thread.clone(),
                    mem_jump_text: format!("0x{:04X}", initial_start),
                    mem_len_text: format!("{}", mem_window_len_thread.load(Ordering::SeqCst)),
                    quit_flag: quit_flag_thread_clone,
                    mem_use_drag: false,
                    mem_edit_mode: false,
                    mem_pending: HashMap::new(),
                    mem_selected: None,
                    mem_edit_buf: String::new(),
                    mem_focus_next: false,
                    mem_last_sel_addr: None,
                    mem_last_at_start: false,
                    mem_last_at_end: false,
                    edit_mode: false,
                    edit_initialized: false,
                    edit_a: String::new(),
                    edit_b: String::new(),
                    edit_d: String::new(),
                    edit_x: String::new(),
                    edit_y: String::new(),
                    edit_u: String::new(),
                    edit_s: String::new(),
                    edit_dp: String::new(),
                    edit_pc: String::new(),
                    cc_e: false,
                    cc_f: false,
                    cc_h: false,
                    cc_i: false,
                    cc_n: false,
                    cc_z: false,
                    cc_v: false,
                    cc_c: false,
                    input_error: None,
                    gpio_present: true,
                    gpio_bits: 8,
                    gpio_out: 0,
                    gpio_dir: 0xFFFF_FFFF,
                    gpio_on_color: egui::Color32::LIGHT_YELLOW,
                    gpio_sync: Arc::new(Mutex::new(None)),
                    mmu_sync: Arc::new(Mutex::new(None)),
                    mmu_present: false,
                    mmu_sys_mode: false,
                    mmu_task: 0,
                    mmu_map_prev: [0u16; 16],
                    mmu_map_cur: [0u16; 16],
                    timer_present: false,
                    timer_pending: false,
                };
                // Windows supports any-thread event loop via ExtWindows; run normally.
                // On Unix (incl. WSL2), winit may require main-thread; catch and fall back headless.
                #[cfg(windows)]
                {
                    started_flag_thread.store(true, Ordering::SeqCst);
                    alive_flag_thread.store(true, Ordering::SeqCst);
                    let _ = eframe::run_native(&title, native_options, Box::new(|_| Box::new(app)));
                    // Window closed: mark not alive. Keep `started` true so is_closed() becomes true.
                    alive_flag_thread.store(false, Ordering::SeqCst);
                }
                #[cfg(not(windows))]
                {
                    use std::panic::{catch_unwind, AssertUnwindSafe};
                    // Suppress panic noise only during GUI bootstrap on Unix
                    let prev_hook = std::panic::take_hook();
                    std::panic::set_hook(Box::new(|_| {}));
                    started_flag_thread.store(true, Ordering::SeqCst);
                    alive_flag_thread.store(true, Ordering::SeqCst);
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        let _ = eframe::run_native(&title, native_options, Box::new(|_| Box::new(app)));
                    })).map_err(|_| {
                        eprintln!(
                            "[regview] GUI failed to start on this platform/thread. Running headless."
                        );
                    });
                    let _ = std::panic::set_hook(prev_hook);
                    alive_flag_thread.store(false, Ordering::SeqCst);
                    started_flag_thread.store(false, Ordering::SeqCst);
                }
            });

            Self {
                snapshot,
                paused,
                step_requests,
                lst_mmap,
                lst_text,
                use_hex,
                auto_hex_on_edit,
                scroll_to_pc_req,
                follow_pc_flag,
                span_policy,
                window_len,
                cmds,
                ja_font,
                _handle: Some(handle),
                mem_follow_pc,
                mem_window_len,
                mem_window_start,
                mem_buf,
                mem_hl,
                mem_utf8,
                quit_flag: quit_flag_thread,
                alive: alive_flag,
                started: started_flag,
                gpio_sync: Arc::new(Mutex::new(None)),
                mmu_sync: Arc::new(Mutex::new(None)),
            }
        }

        pub fn create(
            _title: &str,
            lst_path: Option<PathBuf>,
            lst_fallback_text: Option<String>,
            initial_start: u16,
        ) -> Self {
            // Build the same shared state as spawn(), but do not start a GUI thread.
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
            let lst_text: Option<Arc<Mutex<String>>> = lst_fallback_text.map(|s| Arc::new(Mutex::new(s)));
            let default_use_hex_view: bool = lst_text.is_some() && lst_mmap.is_none();
            let use_hex = Arc::new(AtomicBool::new(default_use_hex_view));
            let auto_hex_on_edit = Arc::new(AtomicBool::new(true));
            let scroll_to_pc_req = Arc::new(AtomicBool::new(false));
            let follow_pc_flag = Arc::new(AtomicBool::new(true));
            let span_policy = Arc::new(AtomicUsize::new(0));
            let window_len = Arc::new(AtomicUsize::new(256));
            let cmds: Arc<Mutex<Vec<Command>>> = Arc::new(Mutex::new(Vec::new()));
            let mem_follow_pc = Arc::new(AtomicBool::new(true));
            let mem_window_len = Arc::new(AtomicUsize::new(256));
            let mem_window_start = Arc::new(AtomicUsize::new(initial_start as usize));
            let mem_buf: Arc<Mutex<(u16, Vec<u8>)>> = Arc::new(Mutex::new((0, Vec::new())));
            let mem_hl: Arc<Mutex<Option<(u16, usize)>>> = Arc::new(Mutex::new(None));
            let mem_utf8 = Arc::new(AtomicBool::new(false));
            let quit_flag = Arc::new(AtomicBool::new(false));
            let alive = Arc::new(AtomicBool::new(false));
            let started = Arc::new(AtomicBool::new(false));

            // Font discovery (same as spawn)
            let ja_font: Option<Arc<Vec<u8>>> = {
                if let Ok(p) = std::env::var("EM6809_GUI_FONT") {
                    if !p.is_empty() {
                        if let Ok(b) = std::fs::read(&p) { Some(Arc::new(b)) } else { None }
                    } else { None }
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
                        for p in candidates.iter() { if let Ok(b) = std::fs::read(p) { found = Some(Arc::new(b)); break; } }
                        found
                    }
                    #[cfg(target_os = "macos")]
                    {
                        let candidates = [
                            // English-name Hiragino (newer macOS)
                            "/System/Library/Fonts/Hiragino Sans W6.ttc",
                            "/System/Library/Fonts/Hiragino Sans W3.ttc",
                            "/Library/Fonts/Hiragino Sans W6.ttc",
                            "/Library/Fonts/Hiragino Sans W3.ttc",
                            // Older ProN names (may be installed in /Library/Fonts)
                            "/Library/Fonts/Hiragino Kaku Gothic ProN W6.otf",
                            "/Library/Fonts/Hiragino Kaku Gothic ProN W3.otf",
                            // Japanese-name variants (some macOS locales)
                            "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc",
                            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
                            "/System/Library/Fonts/AppleGothic.ttf",
                        ];
                        let mut found: Option<Arc<Vec<u8>>> = None;
                        for p in candidates.iter() { if let Ok(b) = std::fs::read(p) { found = Some(Arc::new(b)); break; } }
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
                        for p in candidates.iter() { if let Ok(b) = std::fs::read(p) { found = Some(Arc::new(b)); break; } }
                        found
                    }
                }
            };

            Self {
                snapshot,
                paused,
                step_requests,
                lst_mmap,
                lst_text,
                use_hex,
                auto_hex_on_edit,
                scroll_to_pc_req,
                follow_pc_flag,
                span_policy,
                window_len,
                cmds,
                ja_font,
                _handle: None,
                mem_follow_pc,
                mem_window_len,
                mem_window_start,
                mem_buf,
                mem_hl,
                mem_utf8,
                quit_flag,
                alive,
                started,
                gpio_sync: Arc::new(Mutex::new(None)),
                mmu_sync: Arc::new(Mutex::new(None)),
            }
        }

        pub fn run_blocking(&self, title: &str) {
            let snap_clone = Arc::clone(&self.snapshot);
            let paused_clone = Arc::clone(&self.paused);
            let step_requests_clone = Arc::clone(&self.step_requests);
            let lst_clone = self.lst_mmap.as_ref().map(Arc::clone);
            let lst_text_clone = self.lst_text.as_ref().map(Arc::clone);
            let use_hex_clone = Arc::clone(&self.use_hex);
            let follow_clone = Arc::clone(&self.follow_pc_flag);
            let span_policy_clone = Arc::clone(&self.span_policy);
            let window_len_clone = Arc::clone(&self.window_len);
            let cmds_clone = Arc::clone(&self.cmds);
            let ja_font_clone = self.ja_font.as_ref().map(Arc::clone);
            let auto_hex_on_edit = Arc::clone(&self.auto_hex_on_edit);
            let mem_follow_pc_thread = self.mem_follow_pc.clone();
            let mem_window_len_thread = self.mem_window_len.clone();
            let mem_window_start_thread = self.mem_window_start.clone();
            let mem_buf_thread = self.mem_buf.clone();
            let mem_hl_thread = self.mem_hl.clone();
            let mem_utf8_thread = self.mem_utf8.clone();
            let initial_start = self.mem_window_start.load(Ordering::SeqCst) as u16;
            let use_japanese = Self::is_japanese_locale();

            let mut native_options = eframe::NativeOptions::default();
            let base = eframe::egui::ViewportBuilder::default()
                .with_inner_size(eframe::egui::vec2(1800.0, 700.0))
                .with_min_inner_size(eframe::egui::vec2(900.0, 400.0))
                .with_resizable(true);
            #[cfg(all(unix, not(target_os = "macos")))]
            let vp = base.with_app_id("em6809");
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            let vp = base;
            native_options.viewport = vp;
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
                auto_hex_on_edit: Arc::clone(&auto_hex_on_edit),
                scroll_to_pc_req: Arc::clone(&self.scroll_to_pc_req),
                follow_pc_flag: follow_clone,
                span_policy: span_policy_clone,
                window_len: window_len_clone,
                cmds: cmds_clone,
                ja_font: ja_font_clone,
                fonts_applied: false,
                menu_fixed: false,
                use_japanese,
                last_pc_for_scroll: 0,
                #[cfg(windows)]
                about_open: false,
                #[cfg(windows)]
                about_icon_tex: None,
                #[cfg(target_os = "linux")]
                about_open: false,
                #[cfg(target_os = "linux")]
                about_icon_tex: None,
                mem_follow_pc: mem_follow_pc_thread,
                mem_window_len: mem_window_len_thread,
                mem_window_start: mem_window_start_thread,
                mem_buf: mem_buf_thread,
                mem_hl: mem_hl_thread,
                mem_utf8: mem_utf8_thread,
                mem_jump_text: format!("0x{:04X}", initial_start),
                mem_len_text: format!("{}", self.mem_window_len.load(Ordering::SeqCst)),
                quit_flag: self.quit_flag.clone(),
                mem_use_drag: false,
                mem_edit_mode: false,
                mem_pending: HashMap::new(),
                mem_selected: None,
                mem_edit_buf: String::new(),
                    mem_focus_next: false,
                    mem_last_sel_addr: None,
                    mem_last_at_start: false,
                    mem_last_at_end: false,
                    edit_mode: false,
                edit_initialized: false,
                edit_a: String::new(),
                edit_b: String::new(),
                edit_d: String::new(),
                edit_x: String::new(),
                edit_y: String::new(),
                edit_u: String::new(),
                edit_s: String::new(),
                edit_dp: String::new(),
                edit_pc: String::new(),
                cc_e: false,
                cc_f: false,
                cc_h: false,
                cc_i: false,
                cc_n: false,
                cc_z: false,
                cc_v: false,
                cc_c: false,
                input_error: None,
                gpio_present: true,
                gpio_bits: 8,
                gpio_out: 0,
                gpio_dir: 0xFFFF_FFFF,
                    gpio_on_color: egui::Color32::LIGHT_YELLOW,
                gpio_sync: Arc::new(Mutex::new(None)),
                mmu_sync: Arc::new(Mutex::new(None)),
                mmu_present: false,
                mmu_sys_mode: false,
                mmu_task: 0,
                mmu_map_prev: [0u16; 16],
                mmu_map_cur: [0u16; 16],
                timer_present: false,
                timer_pending: false,
            };
            #[cfg(windows)]
            {
                self.started.store(true, Ordering::SeqCst);
                self.alive.store(true, Ordering::SeqCst);
                let _ = eframe::run_native(title, native_options, Box::new(|_| Box::new(app)));
                self.alive.store(false, Ordering::SeqCst);
                self.started.store(false, Ordering::SeqCst);
            }
            #[cfg(not(windows))]
            {
                use std::panic::{catch_unwind, AssertUnwindSafe};
                let prev = std::panic::take_hook();
                std::panic::set_hook(Box::new(|_| {}));
                self.started.store(true, Ordering::SeqCst);
                self.alive.store(true, Ordering::SeqCst);
                let res: Result<eframe::Result<(), eframe::Error>, _> =
                    catch_unwind(AssertUnwindSafe(|| {
                        eframe::run_native(title, native_options, Box::new(|_| Box::new(app)))
                    }));
                let _ = std::panic::set_hook(prev);
                self.alive.store(false, Ordering::SeqCst);
                self.started.store(false, Ordering::SeqCst);
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        eprintln!("[regview] GUI error: {} — running headless", e);
                    }
                    Err(_) => {
                        eprintln!(
                            "[regview] GUI failed to start on this platform/thread. Running headless."
                        );
                    }
                }
            }
        }

        pub fn push(&self, s: RegSnapshot) {
            if let Ok(mut guard) = self.snapshot.lock() {
                *guard = s;
            }
        }

        pub fn request_close(&self) {
            self.quit_flag.store(true, Ordering::SeqCst);
        }

        pub fn is_closed(&self) -> bool {
            self.started.load(Ordering::SeqCst) && !self.alive.load(Ordering::SeqCst)
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
            // When switching to Hex programmatically (e.g., after edits), also request centering on PC if following.
            if self.follow_pc_flag.load(Ordering::SeqCst) {
                self.scroll_to_pc_req.store(true, Ordering::SeqCst);
            }
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
    
    impl RegView {
        pub fn auto_hex_on_edit(&self) -> bool {
            self.auto_hex_on_edit.load(Ordering::SeqCst)
        }
        /// Worker-side: pop only non-UiSet commands, leaving UiSet* for GUI thread.
        pub fn take_command_worker(&self) -> Option<Command> {
            if let Ok(mut q) = self.cmds.lock() {
                if let Some(idx) = q
                    .iter()
                    .position(|c| !matches!(c, Command::UiSetGpio(..) | Command::UiSetMmu(..) | Command::UiSetTimer(..) | Command::UiSetGpioOnColor(..)))
                {
                    return Some(q.remove(idx));
                }
            }
            None
        }
        pub fn sync_gpio(&self, present: bool, bits: u8, out: u32, dir: u32) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetGpio(present, bits, out, dir));
            }
        }
        pub fn set_gpio_on_color(&self, rgb: u32) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetGpioOnColor(rgb));
            }
        }
        pub fn sync_mmu(&self, sys_mode: bool, task: u8, map: [u16; 16]) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetMmu(sys_mode, task, map));
            }
        }
        pub fn sync_timer(&self, present: bool, pending: bool) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetTimer(present, pending));
            }
        }
    }

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

pub use gui::Command;
pub use gui::Viewer;
