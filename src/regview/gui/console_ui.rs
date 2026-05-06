use once_cell::sync::OnceCell;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;

use crate::regview::gui::{ConsoleState, TermColor};

#[derive(Clone)]
pub(super) struct ConsoleConfig {
    pub enabled: bool,
    pub title: String,
    pub font_size: f32,
    pub cols: u16,
    pub rows: u16,
    pub scrollback: usize,
    pub ctrl_keys: bool,
}

static CONSOLE_CLOSED: OnceCell<AtomicBool> = OnceCell::new();
static CONSOLE_VISIBLE: OnceCell<AtomicBool> = OnceCell::new();
static CONSOLE_CTX: OnceCell<egui::Context> = OnceCell::new();
static CONSOLE_VIEWPORT_SIZE: OnceCell<Mutex<egui::Vec2>> = OnceCell::new();
static CONSOLE_REPAINT_TICKER: OnceCell<AtomicBool> = OnceCell::new();

pub(crate) fn console_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("console_vt100")
}

fn update_console_visible(desired: bool) -> bool {
    let flag = CONSOLE_VISIBLE.get_or_init(|| AtomicBool::new(desired));
    let prev = flag.swap(desired, std::sync::atomic::Ordering::SeqCst);
    prev != desired
}

pub(crate) fn set_console_visible(ctx: &egui::Context, visible: bool) {
    let id = console_viewport_id();
    update_console_visible(visible);
    ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Visible(visible));
}

pub(crate) fn with_console_hidden<R>(ctx: &egui::Context, f: impl FnOnce() -> R) -> R {
    let was_visible = CONSOLE_VISIBLE
        .get()
        .map(|flag| flag.load(std::sync::atomic::Ordering::SeqCst))
        .unwrap_or(false);
    if was_visible {
        set_console_visible(ctx, false);
    }
    let out = f();
    if was_visible {
        set_console_visible(ctx, true);
    }
    out
}

pub(crate) fn request_console_repaint(ctx_fallback: &egui::Context) {
    if let Some(ctx) = CONSOLE_CTX.get() {
        ctx.request_repaint();
        ctx.request_repaint_after(Duration::from_millis(16));
    } else {
        ctx_fallback.request_repaint();
        ctx_fallback.request_repaint_after(Duration::from_millis(16));
    }
}

/// Background thread that requests a periodic Console repaint so the
/// eframe / winit event loop never goes fully idle on a stuck child
/// viewport.  Originally Windows-only (a separate occlusion-related
/// quirk there); now also enabled on Linux/macOS because Mutter on
/// Ubuntu under Parallels was observed to deadlock at startup when
/// the Console viewport opens fully behind the main window — Wayland
/// withholds the first frame callback and the run loop blocks.  A
/// 33 ms heartbeat from a separate thread keeps the loop progressing
/// regardless of compositor state.
fn ensure_console_repaint_ticker() {
    let started = CONSOLE_REPAINT_TICKER.get_or_init(|| AtomicBool::new(false));
    if started.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        // [DIAG] Linux freeze investigation: confirm the ticker
        // thread actually keeps running during a freeze.
        #[cfg(target_os = "linux")]
        let mut tick_n: u64 = 0;
        loop {
            let visible = CONSOLE_VISIBLE
                .get()
                .map(|flag| flag.load(std::sync::atomic::Ordering::SeqCst))
                .unwrap_or(false);
            #[cfg(target_os = "linux")]
            {
                if tick_n % 30 == 0 {
                    eprintln!(
                        "[diag console_ticker] tick={} visible={} ctx_set={}",
                        tick_n,
                        visible,
                        CONSOLE_CTX.get().is_some()
                    );
                }
                tick_n = tick_n.wrapping_add(1);
            }
            if visible {
                if let Some(ctx) = CONSOLE_CTX.get() {
                    let id = console_viewport_id();
                    ctx.request_repaint();
                    ctx.request_repaint_of(id);
                    ctx.request_repaint_after_for(Duration::from_millis(33), id);
                }
            }
            std::thread::sleep(Duration::from_millis(33));
        }
    });
}

pub(crate) fn console_viewport_size() -> Option<egui::Vec2> {
    CONSOLE_VIEWPORT_SIZE
        .get()
        .and_then(|m| m.lock().ok().map(|g| *g))
}

pub(crate) fn poke_console_viewport(ctx: &egui::Context) {
    if let Some(size) = console_viewport_size() {
        ctx.send_viewport_cmd_to(
            console_viewport_id(),
            egui::ViewportCommand::InnerSize(size),
        );
    }
}

fn measure_char_size(ctx: &egui::Context, font_size: f32) -> egui::Vec2 {
    let font_id = egui::FontId::monospace(font_size);
    let size = ctx.fonts(|f| {
        f.layout_no_wrap("M".to_string(), font_id.clone(), egui::Color32::WHITE)
            .size()
    });
    let row_height = ctx.fonts(|f| f.row_height(&font_id));
    egui::vec2(size.x, row_height.max(size.y))
}

fn color_from_idx(idx: u8) -> egui::Color32 {
    match idx {
        0 => egui::Color32::from_rgb(0x00, 0x00, 0x00),
        1 => egui::Color32::from_rgb(0x80, 0x00, 0x00),
        2 => egui::Color32::from_rgb(0x00, 0x80, 0x00),
        3 => egui::Color32::from_rgb(0x80, 0x80, 0x00),
        4 => egui::Color32::from_rgb(0x00, 0x00, 0x80),
        5 => egui::Color32::from_rgb(0x80, 0x00, 0x80),
        6 => egui::Color32::from_rgb(0x00, 0x80, 0x80),
        7 => egui::Color32::from_rgb(0xC0, 0xC0, 0xC0),
        8 => egui::Color32::from_rgb(0x80, 0x80, 0x80),
        9 => egui::Color32::from_rgb(0xFF, 0x00, 0x00),
        10 => egui::Color32::from_rgb(0x00, 0xFF, 0x00),
        11 => egui::Color32::from_rgb(0xFF, 0xFF, 0x00),
        12 => egui::Color32::from_rgb(0x00, 0x00, 0xFF),
        13 => egui::Color32::from_rgb(0xFF, 0x00, 0xFF),
        14 => egui::Color32::from_rgb(0x00, 0xFF, 0xFF),
        15 => egui::Color32::from_rgb(0xFF, 0xFF, 0xFF),
        16..=231 => {
            let idx = idx - 16;
            let r = idx / 36;
            let g = (idx / 6) % 6;
            let b = idx % 6;
            let conv = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            egui::Color32::from_rgb(conv(r), conv(g), conv(b))
        }
        232..=255 => {
            let gray = 8 + (idx - 232) * 10;
            egui::Color32::from_rgb(gray, gray, gray)
        }
    }
}

fn map_color(color: TermColor, fallback: egui::Color32) -> egui::Color32 {
    match color {
        TermColor::Default => fallback,
        TermColor::Idx(idx) => color_from_idx(idx),
        TermColor::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
    }
}

fn ctrl_key_to_byte(key: egui::Key) -> Option<u8> {
    match key {
        egui::Key::A => Some(0x01),
        egui::Key::B => Some(0x02),
        egui::Key::C => Some(0x03),
        egui::Key::D => Some(0x04),
        egui::Key::E => Some(0x05),
        egui::Key::F => Some(0x06),
        egui::Key::G => Some(0x07),
        egui::Key::H => Some(0x08),
        egui::Key::I => Some(0x09),
        egui::Key::J => Some(0x0A),
        egui::Key::K => Some(0x0B),
        egui::Key::L => Some(0x0C),
        egui::Key::M => Some(0x0D),
        egui::Key::N => Some(0x0E),
        egui::Key::O => Some(0x0F),
        egui::Key::P => Some(0x10),
        egui::Key::Q => Some(0x11),
        egui::Key::R => Some(0x12),
        egui::Key::S => Some(0x13),
        egui::Key::T => Some(0x14),
        egui::Key::U => Some(0x15),
        egui::Key::V => Some(0x16),
        egui::Key::W => Some(0x17),
        egui::Key::X => Some(0x18),
        egui::Key::Y => Some(0x19),
        egui::Key::Z => Some(0x1A),
        _ => None,
    }
}

fn collect_input(ctx: &egui::Context, ctrl_keys: bool) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut enter_handled = false;
    let events = ctx.input(|i| i.events.clone());
    for ev in &events {
        if let egui::Event::Key {
            key,
            pressed,
            modifiers,
            ..
        } = ev
        {
            if !*pressed {
                continue;
            }
            if modifiers.ctrl && ctrl_keys {
                if let Some(b) = ctrl_key_to_byte(*key) {
                    out.push(b);
                    continue;
                }
            }
            match key {
                egui::Key::Enter => {
                    out.push(b'\r');
                    enter_handled = true;
                }
                egui::Key::Backspace => out.push(0x08),
                egui::Key::Tab => out.push(0x09),
                egui::Key::Escape => out.push(0x1B),
                egui::Key::ArrowUp => out.extend_from_slice(b"\x1B[A"),
                egui::Key::ArrowDown => out.extend_from_slice(b"\x1B[B"),
                egui::Key::ArrowRight => out.extend_from_slice(b"\x1B[C"),
                egui::Key::ArrowLeft => out.extend_from_slice(b"\x1B[D"),
                egui::Key::Home => out.extend_from_slice(b"\x1B[H"),
                egui::Key::End => out.extend_from_slice(b"\x1B[F"),
                egui::Key::PageUp => out.extend_from_slice(b"\x1B[5~"),
                egui::Key::PageDown => out.extend_from_slice(b"\x1B[6~"),
                egui::Key::Insert => out.extend_from_slice(b"\x1B[2~"),
                egui::Key::Delete => out.extend_from_slice(b"\x1B[3~"),
                _ => {}
            }
        }
    }
    for ev in events {
        if let egui::Event::Text(text) = ev {
            if enter_handled && (text == "\n" || text == "\r") {
                continue;
            }
            out.extend_from_slice(text.as_bytes());
        }
    }
    out
}

fn render_console(
    ctx: &egui::Context,
    state: &Arc<Mutex<ConsoleState>>,
    input: &Arc<Mutex<Vec<u8>>>,
    cfg: &ConsoleConfig,
) {
    let font_id = egui::FontId::monospace(cfg.font_size);
    let default_fg = egui::Color32::from_rgb(0xE0, 0xE0, 0xE0);
    let default_bg = egui::Color32::from_rgb(0x10, 0x10, 0x10);
    let char_size = measure_char_size(ctx, cfg.font_size);
    let bytes = em6809::io::take_console_gui_bytes();
    egui::CentralPanel::default()
        .frame(egui::Frame::none().inner_margin(egui::Margin::same(0.0)))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            ui.painter().rect_filled(ui.max_rect(), 0.0, default_bg);
            let cols = cfg.cols.max(1);
            let rows = cfg.rows.max(1);
            if let Ok(mut st) = state.lock() {
                st.set_scrollback_len(cfg.scrollback);
                st.set_size(rows, cols);
                if !bytes.is_empty() {
                    st.process_bytes(&bytes);
                }
                let rows = st.term.rows;
                let cols = st.term.cols;
                let history_len = st.term.history.len();
                let content_rows = history_len.saturating_add(st.screen_content_rows());
                let total_rows = content_rows.max(rows);
                let scroll_id = egui::Id::new("console_scroll");
                let row_height = char_size.y.max(1.0);
                let view_height = (rows as f32 * row_height).max(1.0);
                let hover_pos = ctx.input(|i| i.pointer.hover_pos());
                let hover_in_console = hover_pos
                    .map(|p| ui.max_rect().contains(p))
                    .unwrap_or(false);
                let user_scrolling_now = ctx.input(|i| {
                    i.raw_scroll_delta.y != 0.0 || (hover_in_console && i.pointer.primary_down())
                });
                let output = egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt(scroll_id)
                    .max_height(view_height)
                    .min_scrolled_height(view_height)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .stick_to_bottom(false)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                        let content_size =
                            egui::vec2(ui.available_width(), total_rows as f32 * row_height);
                        let content_rect = ui.allocate_space(content_size).1;
                        let origin = content_rect.min;
                        let painter = ui.painter().clone();
                        let clip_rect = ui.clip_rect();
                        let start_row =
                            ((clip_rect.min.y - origin.y) / row_height).floor().max(0.0) as usize;
                        let end_row =
                            ((clip_rect.max.y - origin.y) / row_height).ceil().max(0.0) as usize;
                        let start_row = start_row.min(total_rows);
                        let end_row = end_row.min(total_rows);
                        let last_row_rect = if total_rows == 0 {
                            None
                        } else {
                            Some(egui::Rect::from_min_size(
                                egui::pos2(
                                    origin.x,
                                    origin.y + (total_rows.saturating_sub(1)) as f32 * row_height,
                                ),
                                egui::vec2(content_size.x, row_height),
                            ))
                        };
                        for line_index in start_row..end_row {
                            let line = if line_index < history_len {
                                st.term.history.get(line_index)
                            } else {
                                st.term.screen.get(line_index.saturating_sub(history_len))
                            };
                            let mut job = egui::text::LayoutJob::default();
                            job.halign = egui::Align::LEFT;
                            job.wrap.max_width = f32::INFINITY;
                            let mut c = 0;
                            while c < cols {
                                let cell = line.and_then(|l| l.get(c));
                                if let Some(cell) = cell {
                                    let text = cell.ch;
                                    let mut fg = map_color(cell.fg, default_fg);
                                    let mut bg = map_color(cell.bg, default_bg);
                                    if cell.inverse {
                                        std::mem::swap(&mut fg, &mut bg);
                                    }
                                    let mut format = egui::TextFormat {
                                        font_id: font_id.clone(),
                                        color: fg,
                                        background: bg,
                                        ..Default::default()
                                    };
                                    if cell.bold {
                                        format.font_id.size = cfg.font_size + 1.0;
                                    }
                                    if cell.underline {
                                        format.underline = egui::Stroke::new(1.0, format.color);
                                    }
                                    let mut buf = [0u8; 4];
                                    job.append(text.encode_utf8(&mut buf), 0.0, format);
                                } else {
                                    let format = egui::TextFormat {
                                        font_id: font_id.clone(),
                                        color: default_fg,
                                        background: default_bg,
                                        ..Default::default()
                                    };
                                    job.append(" ", 0.0, format);
                                }
                                c = c.saturating_add(1);
                            }
                            let row_rect = egui::Rect::from_min_size(
                                egui::pos2(origin.x, origin.y + line_index as f32 * row_height),
                                egui::vec2(content_size.x, row_height),
                            );
                            let galley = painter.layout_job(job);
                            painter.galley(row_rect.min, galley, egui::Color32::WHITE);
                            ui.allocate_rect(row_rect, egui::Sense::hover());
                        }
                        if st.follow && !user_scrolling_now {
                            if let Some(rect) = last_row_rect {
                                ui.scroll_to_rect(rect, Some(egui::Align::BOTTOM));
                            }
                        }
                        if st.follow {
                            let row = st.term.cursor_row.min(rows.saturating_sub(1));
                            let col = st.term.cursor_col.min(cols.saturating_sub(1));
                            let cursor_line = history_len.saturating_add(row);
                            let pos = origin
                                + egui::vec2(
                                    col as f32 * char_size.x,
                                    cursor_line as f32 * row_height,
                                );
                            let rect = egui::Rect::from_min_size(pos, char_size);
                            let color = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 80);
                            painter.rect_filled(rect, 0.0, color);
                        }
                    });
                let max_offset_y =
                    (total_rows as f32 * row_height - output.inner_rect.height()).max(0.0);
                let at_bottom = output.state.offset.y >= (max_offset_y - row_height * 0.25);
                let scroll_y = output.state.offset.y;
                let user_scrolled = user_scrolling_now && scroll_y != st.last_scroll_y;
                if user_scrolled {
                    // Only user intent toggles follow. Re-enable only if user scrolls to bottom.
                    st.follow = at_bottom;
                    if st.follow {
                        st.scroll_offset = 0;
                    }
                }
                st.last_scroll_y = scroll_y;
            }
            if ctx.input(|i| i.viewport().focused).unwrap_or(true) {
                let input_bytes = collect_input(ctx, cfg.ctrl_keys);
                if !input_bytes.is_empty() {
                    if let Ok(mut g) = input.lock() {
                        g.extend_from_slice(&input_bytes);
                    }
                }
            }
        });
    // Keep repainting even when the console viewport is unfocused (Windows).
    ctx.request_repaint_after(Duration::from_millis(33));
}

pub(super) fn show_console_viewport(
    ctx: &egui::Context,
    state: Arc<Mutex<ConsoleState>>,
    input: Arc<Mutex<Vec<u8>>>,
    cfg: ConsoleConfig,
) {
    let id = console_viewport_id();
    if update_console_visible(cfg.enabled) {
        set_console_visible(ctx, cfg.enabled);
    }
    let char_size = measure_char_size(ctx, cfg.font_size);
    let width = (char_size.x * cfg.cols as f32 + 16.0).max(320.0);
    let height = (char_size.y * cfg.rows as f32 + 16.0).max(240.0);
    if let Some(slot) = CONSOLE_VIEWPORT_SIZE.get() {
        if let Ok(mut g) = slot.lock() {
            *g = egui::vec2(width, height);
        }
    } else {
        let _ = CONSOLE_VIEWPORT_SIZE.set(Mutex::new(egui::vec2(width, height)));
    }
    let builder = egui::ViewportBuilder::default()
        .with_title(cfg.title.clone())
        .with_inner_size(egui::vec2(width, height))
        .with_visible(cfg.enabled)
        .with_window_level(egui::WindowLevel::Normal)
        .with_resizable(true);
    // Use `show_viewport_immediate` on Windows + Linux: the closure
    // runs synchronously inside the parent's update, so there is no
    // independent surface-ack cycle that can wedge when the Console
    // window is occluded.  macOS keeps `show_viewport_deferred`
    // because the focus / canBecomeKeyWindow swizzle (see
    // `platform/macos.rs`) is engineered around the deferred model
    // and tested across BP / CS / Console.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        ctx.show_viewport_immediate(id, builder, move |ctx, _class| {
            let _ = CONSOLE_CTX.get_or_init(|| ctx.clone());
            ensure_console_repaint_ticker();
            // [DIAG] Linux freeze investigation: count Console
            // viewport closure invocations.
            #[cfg(target_os = "linux")]
            {
                use std::sync::atomic::{AtomicU64, Ordering as AtOrd};
                static CLOSURE_TICK: AtomicU64 = AtomicU64::new(0);
                let n = CLOSURE_TICK.fetch_add(1, AtOrd::Relaxed);
                if n % 30 == 0 {
                    let t = ctx.input(|i| i.time);
                    let focused = ctx.input(|i| i.viewport().focused).unwrap_or(false);
                    eprintln!(
                        "[diag console_closure] tick={} t={:.3} focused={}",
                        n, t, focused
                    );
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(cfg.title.clone()));
            if cfg.enabled {
                render_console(ctx, &state, &input, &cfg);
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                update_console_visible(false);
                note_console_closed();
            }
        });
    }
    #[cfg(target_os = "macos")]
    {
        ensure_console_repaint_ticker();
        ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
            let _ = CONSOLE_CTX.get_or_init(|| ctx.clone());
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(cfg.title.clone()));
            if cfg.enabled {
                render_console(ctx, &state, &input, &cfg);
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                update_console_visible(false);
                note_console_closed();
            }
        });
    }
}

pub(super) fn close_console_viewport(ctx: &egui::Context) {
    let id = egui::ViewportId::from_hash_of("console_vt100");
    ctx.send_viewport_cmd_to(id, egui::ViewportCommand::CancelClose);
    set_console_visible(ctx, false);
}

pub(super) fn note_console_closed() {
    if let Some(flag) = CONSOLE_CLOSED.get() {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    } else {
        let _ = CONSOLE_CLOSED.set(std::sync::atomic::AtomicBool::new(true));
    }
}

pub(super) fn take_console_closed() -> bool {
    if let Some(flag) = CONSOLE_CLOSED.get() {
        flag.swap(false, std::sync::atomic::Ordering::SeqCst)
    } else {
        false
    }
}
