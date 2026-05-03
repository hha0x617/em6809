#![allow(dead_code)]

mod helpers;
mod settings;
pub use settings::SettingsState;
// Expose GPIO snapshot publisher to outer modules
#[allow(unused_imports)]
pub use em6809::io::publish_gpio_broadcast as publish_gpio_snapshot;
pub use helpers::{
    draw_origin_badge, format_color_hex, format_memory_dump, parse_color_hex,
    parse_num_u16_any_gui, parse_num_usize_any_gui, parse_u16_any,
};

use std::time::Duration;

#[cfg(target_os = "macos")]
use eframe::egui;
use once_cell::sync::OnceCell;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Arc, Mutex,
};

// macOS Preferences… (Cmd+,) fallback flag and context to wake UI
static PREFS_REQUEST: OnceCell<AtomicBool> = OnceCell::new();
#[cfg(target_os = "macos")]
static PREFS_WAKE_TICKS: OnceCell<AtomicUsize> = OnceCell::new();
#[cfg(target_os = "macos")]
static GUI_CTX: OnceCell<egui::Context> = OnceCell::new();

#[cfg(target_os = "macos")]
pub fn request_prefs_open() {
    PREFS_REQUEST
        .get_or_init(|| AtomicBool::new(false))
        .store(true, Ordering::SeqCst);
    PREFS_WAKE_TICKS
        .get_or_init(|| AtomicUsize::new(0))
        .store(10, Ordering::SeqCst); // wake for ~10 frames
    if let Some(ctx) = GUI_CTX.get() {
        ctx.request_repaint();
        ctx.request_repaint_after(Duration::from_millis(10));
    }
}

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

pub static SHOW_ORIGINS_OVERRIDE: OnceCell<AtomicBool> = OnceCell::new();

pub mod gui {
    pub(crate) mod breakpoints_ui;
    pub(crate) mod call_stack_ui;
    pub(crate) mod console_ui;
    mod list_ui;
    mod mem_ui;
    mod regs_ui;
    mod settings_ui;
    use super::*;
    use super::{
        draw_origin_badge, format_color_hex, format_memory_dump, parse_color_hex,
        parse_num_u16_any_gui, parse_num_usize_any_gui, parse_u16_any, SHOW_ORIGINS_OVERRIDE,
    };
    use crate::i18n::LOADER;
    use crate::settings_worker;
    use eframe::{egui, App};
    use em6809::debug::InstructionBoundaries;
    use memmap2::Mmap;
    use std::collections::HashMap;
    use std::fs::File;
    use std::path::PathBuf;
    use vte;

    // Type aliases to simplify complex sync types
    type GpioSyncState = Option<(bool, u8, u32, u32)>;
    type MmuSyncState = Option<(bool, u8, [u16; 16])>;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum TermColor {
        Default,
        Idx(u8),
        Rgb(u8, u8, u8),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct TermCell {
        pub(crate) ch: char,
        pub(crate) fg: TermColor,
        pub(crate) bg: TermColor,
        pub(crate) bold: bool,
        pub(crate) underline: bool,
        pub(crate) inverse: bool,
    }

    impl Default for TermCell {
        fn default() -> Self {
            Self {
                ch: ' ',
                fg: TermColor::Default,
                bg: TermColor::Default,
                bold: false,
                underline: false,
                inverse: false,
            }
        }
    }

    impl TermCell {
        fn has_contents(&self) -> bool {
            self.ch != ' '
                || self.fg != TermColor::Default
                || self.bg != TermColor::Default
                || self.bold
                || self.underline
                || self.inverse
        }
    }

    struct TermState {
        pub(crate) rows: usize,
        pub(crate) cols: usize,
        pub(crate) cursor_row: usize,
        pub(crate) cursor_col: usize,
        saved_row: usize,
        saved_col: usize,
        fg: TermColor,
        bg: TermColor,
        bold: bool,
        underline: bool,
        inverse: bool,
        wrap_pending: bool,
        pub(crate) screen: Vec<Vec<TermCell>>,
        pub(crate) history: std::collections::VecDeque<Vec<TermCell>>,
        scrollback: usize,
        used_rows: usize,
    }

    impl TermState {
        fn new(rows: u16, cols: u16, scrollback: usize) -> Self {
            let rows = rows.max(1) as usize;
            let cols = cols.max(1) as usize;
            let mut state = Self {
                rows,
                cols,
                cursor_row: 0,
                cursor_col: 0,
                saved_row: 0,
                saved_col: 0,
                fg: TermColor::Default,
                bg: TermColor::Default,
                bold: false,
                underline: false,
                inverse: false,
                wrap_pending: false,
                screen: Vec::new(),
                history: std::collections::VecDeque::new(),
                scrollback,
                used_rows: 0,
            };
            state.reset_screen();
            state
        }

        fn reset_screen(&mut self) {
            self.screen = vec![self.blank_line_default(); self.rows];
            self.cursor_row = 0;
            self.cursor_col = 0;
            self.wrap_pending = false;
            self.used_rows = 0;
        }

        fn blank_line_default(&self) -> Vec<TermCell> {
            vec![TermCell::default(); self.cols]
        }

        fn blank_cell_current(&self) -> TermCell {
            TermCell {
                ch: ' ',
                fg: self.fg,
                bg: self.bg,
                bold: self.bold,
                underline: self.underline,
                inverse: self.inverse,
            }
        }

        fn set_size(&mut self, rows: u16, cols: u16) {
            let rows = rows.max(1) as usize;
            let cols = cols.max(1) as usize;
            if self.rows == rows && self.cols == cols {
                return;
            }
            let old_rows = self.rows;
            let old_cols = self.cols;
            self.rows = rows;
            self.cols = cols;
            self.wrap_pending = false;
            if self.used_rows > rows {
                self.used_rows = rows;
            }

            if cols != old_cols {
                for line in &mut self.screen {
                    if line.len() > cols {
                        line.truncate(cols);
                    } else if line.len() < cols {
                        line.extend(
                            std::iter::repeat_with(TermCell::default).take(cols - line.len()),
                        );
                    }
                }
                for line in &mut self.history {
                    if line.len() > cols {
                        line.truncate(cols);
                    } else if line.len() < cols {
                        line.extend(
                            std::iter::repeat_with(TermCell::default).take(cols - line.len()),
                        );
                    }
                }
            }

            if rows > old_rows {
                let extra = rows - old_rows;
                for _ in 0..extra {
                    self.screen.push(self.blank_line_default());
                }
            } else if rows < old_rows {
                let remove = old_rows - rows;
                let drained: Vec<Vec<TermCell>> = self.screen.drain(0..remove).collect();
                for line in drained {
                    self.push_history_line(line);
                }
                self.cursor_row = self.cursor_row.saturating_sub(remove);
                if self.cursor_row >= rows {
                    self.cursor_row = rows.saturating_sub(1);
                }
            }
        }

        fn set_scrollback_len(&mut self, rows: usize) {
            if self.scrollback == rows {
                return;
            }
            self.scrollback = rows;
            while self.history.len() > self.scrollback {
                self.history.pop_front();
            }
        }

        fn screen_content_rows(&self) -> usize {
            let mut last = 0usize;
            for (idx, row) in self.screen.iter().enumerate() {
                if row.iter().any(|cell| cell.has_contents()) {
                    last = idx + 1;
                }
            }
            last = last.max(self.used_rows);
            if self.wrap_pending {
                last = last.max(self.cursor_row.saturating_add(1));
            }
            last
        }

        fn clear_screen(&mut self, mode: u16) {
            match mode {
                1 => {
                    for r in 0..=self.cursor_row {
                        let end = if r == self.cursor_row {
                            self.cursor_col
                        } else {
                            self.cols.saturating_sub(1)
                        };
                        self.clear_line_range(r, 0, end);
                    }
                }
                2 => {
                    self.commit_screen_to_history();
                    for r in 0..self.rows {
                        self.clear_line_range(r, 0, self.cols.saturating_sub(1));
                    }
                    self.used_rows = 0;
                }
                _ => {
                    for r in self.cursor_row..self.rows {
                        let start = if r == self.cursor_row {
                            self.cursor_col
                        } else {
                            0
                        };
                        self.clear_line_range(r, start, self.cols.saturating_sub(1));
                    }
                }
            }
        }

        fn clear_line(&mut self, mode: u16) {
            match mode {
                1 => self.clear_line_range(self.cursor_row, 0, self.cursor_col),
                2 => self.clear_line_range(self.cursor_row, 0, self.cols.saturating_sub(1)),
                _ => self.clear_line_range(
                    self.cursor_row,
                    self.cursor_col,
                    self.cols.saturating_sub(1),
                ),
            }
        }

        fn clear_line_range(&mut self, row: usize, start: usize, end: usize) {
            if row >= self.rows || self.cols == 0 {
                return;
            }
            let end = end.min(self.cols.saturating_sub(1));
            let start = start.min(end);
            let blank = self.blank_cell_current();
            let line = &mut self.screen[row];
            for cell in line.iter_mut().take(end + 1).skip(start) {
                *cell = blank.clone();
            }
        }

        fn scroll_up(&mut self, mut count: usize) {
            count = count.min(self.rows);
            for _ in 0..count {
                if let Some(line) = self.screen.first().cloned() {
                    self.push_history_line(line);
                }
                if !self.screen.is_empty() {
                    self.screen.remove(0);
                }
                self.screen.push(self.blank_line_default());
            }
            self.used_rows = self.rows;
        }

        fn push_history_line(&mut self, line: Vec<TermCell>) {
            if self.scrollback == 0 {
                return;
            }
            self.history.push_back(line);
            while self.history.len() > self.scrollback {
                self.history.pop_front();
            }
        }

        fn newline(&mut self) {
            self.wrap_pending = false;
            if self.cursor_row + 1 >= self.rows {
                self.scroll_up(1);
            } else {
                self.cursor_row += 1;
            }
            self.used_rows = self.used_rows.max(self.cursor_row.saturating_add(1));
        }

        fn carriage_return(&mut self) {
            self.cursor_col = 0;
            self.wrap_pending = false;
        }

        fn backspace(&mut self) {
            self.cursor_col = self.cursor_col.saturating_sub(1);
            self.wrap_pending = false;
        }

        fn tab(&mut self) {
            let next = ((self.cursor_col / 8) + 1) * 8;
            self.cursor_col = next.min(self.cols.saturating_sub(1));
            self.wrap_pending = false;
        }

        fn put_char(&mut self, c: char) {
            if self.rows == 0 || self.cols == 0 {
                return;
            }
            if self.wrap_pending {
                self.cursor_col = 0;
                self.newline();
                self.wrap_pending = false;
            }
            if self.cursor_row >= self.rows {
                self.cursor_row = self.rows.saturating_sub(1);
            }
            let cell = TermCell {
                ch: c,
                fg: self.fg,
                bg: self.bg,
                bold: self.bold,
                underline: self.underline,
                inverse: self.inverse,
            };
            self.screen[self.cursor_row][self.cursor_col] = cell;
            self.used_rows = self.used_rows.max(self.cursor_row.saturating_add(1));
            if self.cursor_col + 1 >= self.cols {
                self.wrap_pending = true;
            } else {
                self.cursor_col += 1;
            }
        }

        fn reset_attrs(&mut self) {
            self.fg = TermColor::Default;
            self.bg = TermColor::Default;
            self.bold = false;
            self.underline = false;
            self.inverse = false;
        }

        fn cursor_pos(&mut self, row: usize, col: usize) {
            self.cursor_row = row.min(self.rows.saturating_sub(1));
            self.cursor_col = col.min(self.cols.saturating_sub(1));
            self.wrap_pending = false;
        }

        fn cursor_up(&mut self, n: usize) {
            self.cursor_row = self.cursor_row.saturating_sub(n);
            self.wrap_pending = false;
        }

        fn cursor_down(&mut self, n: usize) {
            self.cursor_row = (self.cursor_row + n).min(self.rows.saturating_sub(1));
            self.wrap_pending = false;
        }

        fn cursor_forward(&mut self, n: usize) {
            self.cursor_col = (self.cursor_col + n).min(self.cols.saturating_sub(1));
            self.wrap_pending = false;
        }

        fn cursor_back(&mut self, n: usize) {
            self.cursor_col = self.cursor_col.saturating_sub(n);
            self.wrap_pending = false;
        }

        fn save_cursor(&mut self) {
            self.saved_row = self.cursor_row;
            self.saved_col = self.cursor_col;
        }

        fn restore_cursor(&mut self) {
            self.cursor_row = self.saved_row.min(self.rows.saturating_sub(1));
            self.cursor_col = self.saved_col.min(self.cols.saturating_sub(1));
        }

        fn reset(&mut self) {
            self.commit_screen_to_history();
            self.reset_attrs();
            self.reset_screen();
        }

        fn commit_screen_to_history(&mut self) {
            let last = self.screen_content_rows();
            if last == 0 {
                return;
            }
            let lines: Vec<Vec<TermCell>> = self.screen.iter().take(last).cloned().collect();
            for line in lines {
                self.push_history_line(line);
            }
        }
    }

    impl vte::Perform for TermState {
        fn print(&mut self, c: char) {
            self.put_char(c);
        }

        fn execute(&mut self, byte: u8) {
            match byte {
                b'\n' => self.newline(),
                b'\r' => self.carriage_return(),
                b'\t' => self.tab(),
                0x08 => self.backspace(),
                _ => {}
            }
        }

        fn csi_dispatch(
            &mut self,
            params: &vte::Params,
            _intermediates: &[u8],
            _ignore: bool,
            action: char,
        ) {
            let mut flat: Vec<u16> = Vec::new();
            for p in params.iter() {
                flat.extend(p.iter().copied());
            }
            match action {
                'm' => {
                    if flat.is_empty() {
                        self.reset_attrs();
                    } else {
                        let mut i = 0;
                        while i < flat.len() {
                            let code = flat[i];
                            match code {
                                0 => self.reset_attrs(),
                                1 => self.bold = true,
                                4 => self.underline = true,
                                7 => self.inverse = true,
                                22 => self.bold = false,
                                24 => self.underline = false,
                                27 => self.inverse = false,
                                30..=37 => self.fg = TermColor::Idx((code - 30) as u8),
                                39 => self.fg = TermColor::Default,
                                40..=47 => self.bg = TermColor::Idx((code - 40) as u8),
                                49 => self.bg = TermColor::Default,
                                90..=97 => self.fg = TermColor::Idx((code - 90 + 8) as u8),
                                100..=107 => self.bg = TermColor::Idx((code - 100 + 8) as u8),
                                38 | 48 => {
                                    let is_fg = code == 38;
                                    if i + 1 < flat.len() {
                                        let mode = flat[i + 1];
                                        if mode == 5 && i + 2 < flat.len() {
                                            let idx = flat[i + 2] as u8;
                                            if is_fg {
                                                self.fg = TermColor::Idx(idx);
                                            } else {
                                                self.bg = TermColor::Idx(idx);
                                            }
                                            i += 2;
                                        } else if mode == 2 && i + 4 < flat.len() {
                                            let r = flat[i + 2] as u8;
                                            let g = flat[i + 3] as u8;
                                            let b = flat[i + 4] as u8;
                                            if is_fg {
                                                self.fg = TermColor::Rgb(r, g, b);
                                            } else {
                                                self.bg = TermColor::Rgb(r, g, b);
                                            }
                                            i += 4;
                                        }
                                    }
                                }
                                _ => {}
                            }
                            i += 1;
                        }
                    }
                }
                'H' | 'f' => {
                    let row = flat.get(0).copied().unwrap_or(1).saturating_sub(1) as usize;
                    let col = flat.get(1).copied().unwrap_or(1).saturating_sub(1) as usize;
                    self.cursor_pos(row, col);
                }
                'A' => {
                    let n = flat.get(0).copied().unwrap_or(1) as usize;
                    self.cursor_up(n);
                }
                'B' => {
                    let n = flat.get(0).copied().unwrap_or(1) as usize;
                    self.cursor_down(n);
                }
                'C' => {
                    let n = flat.get(0).copied().unwrap_or(1) as usize;
                    self.cursor_forward(n);
                }
                'D' => {
                    let n = flat.get(0).copied().unwrap_or(1) as usize;
                    self.cursor_back(n);
                }
                'J' => {
                    let mode = flat.get(0).copied().unwrap_or(0);
                    self.clear_screen(mode);
                }
                'K' => {
                    let mode = flat.get(0).copied().unwrap_or(0);
                    self.clear_line(mode);
                }
                _ => {}
            }
        }

        fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
            match byte as char {
                'c' => self.reset(),
                '7' => self.save_cursor(),
                '8' => self.restore_cursor(),
                _ => {}
            }
        }

        fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
        fn hook(
            &mut self,
            _params: &vte::Params,
            _intermediates: &[u8],
            _ignore: bool,
            _action: char,
        ) {
        }
        fn put(&mut self, _byte: u8) {}
        fn unhook(&mut self) {}
    }

    pub(crate) struct ConsoleState {
        parser: vte::Parser,
        term: TermState,
        scroll_offset: usize,
        follow: bool,
        last_scroll_y: f32,
    }

    impl ConsoleState {
        fn new(rows: u16, cols: u16, scrollback: usize) -> Self {
            Self {
                parser: vte::Parser::new(),
                term: TermState::new(rows, cols, scrollback),
                scroll_offset: 0,
                follow: true,
                last_scroll_y: 0.0,
            }
        }

        fn set_size(&mut self, rows: u16, cols: u16) {
            self.term.set_size(rows, cols);
        }

        fn set_scrollback_len(&mut self, rows: usize) {
            if self.term.scrollback == rows {
                return;
            }
            self.term.set_scrollback_len(rows);
            self.scroll_offset = 0;
            self.follow = true;
            self.last_scroll_y = 0.0;
        }

        fn process_bytes(&mut self, bytes: &[u8]) {
            for b in bytes {
                self.parser.advance(&mut self.term, *b);
            }
        }

        fn screen_content_rows(&self) -> usize {
            self.term.screen_content_rows()
        }

        fn commit_screen_to_history(&mut self) {
            self.term.commit_screen_to_history();
        }
    }

    // macOS-specific helpers moved to crate::platform::macos

    use once_cell::sync::OnceCell;

    pub fn parse_u8_hex(s: &str) -> Option<u8> {
        let sl = s.trim();
        if sl.is_empty() {
            return None;
        }
        let t = sl.trim_start_matches("0x").trim_start_matches("0X");
        u16::from_str_radix(t, 16).ok().map(|v| (v & 0xFF) as u8)
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum DeviceTab {
        Console,
        Block,
        Gpio,
        Timer,
        Mmu,
    }

    /// UI-side mirror of `em6809::debug::CallKind`. Kept as its own
    /// type so the GUI layer doesn't pull `em6809::debug` into every
    /// module — the worker thread converts when pushing snapshots.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum CallKindUi {
        Bsr,
        Jsr,
        Swi(u8),
        Irq,
        Firq,
        Nmi,
    }

    impl CallKindUi {
        pub fn label(&self) -> &'static str {
            match self {
                CallKindUi::Bsr => "BSR",
                CallKindUi::Jsr => "JSR",
                CallKindUi::Swi(1) => "SWI",
                CallKindUi::Swi(2) => "SWI2",
                CallKindUi::Swi(3) => "SWI3",
                CallKindUi::Swi(_) => "SWI?",
                CallKindUi::Irq => "IRQ",
                CallKindUi::Firq => "FIRQ",
                CallKindUi::Nmi => "NMI",
            }
        }
    }

    /// One row in the breakpoints panel — full mirror of
    /// `em6809::debug::Breakpoint` for the GUI to read each frame
    /// without holding the worker thread's lock.
    #[derive(Clone, Debug)]
    pub struct BpInfo {
        pub id: u32,
        pub addr: u16,
        pub enabled: bool,
        pub condition: Option<String>,
        pub hit_count: u64,
        pub ignore_count: u64,
    }

    /// One row in the call-stack panel — UI mirror of
    /// `em6809::debug::CallFrame`.
    #[derive(Clone, Copy, Debug)]
    pub struct CallFrameInfo {
        pub return_addr: u16,
        pub call_site: u16,
        pub target: u16,
        pub sp_at_call: u16,
        pub kind: CallKindUi,
    }

    /// UI-side breakpoint state for a given address. The list pane
    /// renders a `●` for `Enabled`, a `○` for `Disabled`, and nothing
    /// for an address with no breakpoint at all.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum BpUiState {
        Enabled,
        Disabled,
    }

    #[derive(Clone, Debug)]
    pub enum Command {
        SetPc(u16),
        RunUntil(u16),
        /// Cycle the breakpoint state at `addr` through none → enabled
        /// → disabled → none. Triggered by double-clicking a line in
        /// the listing pane (or via the right-click context menu).
        /// Worker thread mutates `cpu.breakpoints` and pushes a fresh
        /// snapshot back via `Viewer::update_breakpoint_states`.
        CycleBreakpointAtAddr(u16),
        /// Set whether the breakpoint with this id is enabled. Used
        /// by the breakpoints panel's per-row checkbox. Unknown ids
        /// (deleted between paint and dispatch) are silently ignored.
        SetBreakpointEnabled(u32, bool),
        /// Remove the breakpoint with this id. Used by the
        /// breakpoints panel's per-row delete button.
        RemoveBreakpoint(u32),
        /// Set (or clear, with an empty string) the condition
        /// expression on the breakpoint with this id. Empty / all-
        /// whitespace strings are stored as `None`. The expression
        /// is **not** validated here; if it fails to parse, the
        /// fail-safe path in `BreakpointSet::check` triggers the BP
        /// anyway so the user notices.
        SetBreakpointCondition(u32, String),
        ApplyRegs(RegSnapshot),
        MemWrite(Vec<(u16, u8)>),
        OpenImage {
            path: std::path::PathBuf,
            base: Option<u16>,
        },
        UiPromptBinBase(std::path::PathBuf),
        UiLoadLst(std::path::PathBuf),
        RefreshHexListing,
        GpioSetOut(u32),
        UiSetGpio(bool, u8, u32, u32),
        UiSetMmu(bool, u8, [u16; 16], [u16; 16]),
        UiSetTimer(bool, bool, bool),
        UiOpenImageDialog,
        OpenImageParsed {
            path: std::path::PathBuf,
            parsed: crate::loader::ParsedImage,
        },
        UiSetBlockStatus {
            present: bool,
            base: Option<u16>,
            backing_path: Option<String>,
            backing_online: bool,
            last_cmd: u8,
            last_data: u8,
            status: u8,
        },
        UiSetGpioOnColor(u32),
        UiOpenSettings,
        /// Request opening Settings in an external top-level OS window
        UiOpenSettingsExternal,
        /// macOS: View → Breakpoints menu item clicked. Flips
        /// `bp_window_open` in the GUI thread. Carried as a command
        /// instead of mutating the flag directly because the NSMenu
        /// callback fires on the AppKit main thread.
        UiToggleBreakpointsWindow,
        /// macOS: View → Call Stack menu item clicked. See
        /// [`UiToggleBreakpointsWindow`].
        UiToggleCallStackWindow,
        /// Recentre the listing pane on `addr` without changing PC.
        /// Pushed by double-click handlers in the Breakpoints / Call
        /// Stack panels — letting the user jump the disassembly view
        /// to the address shown in the panel row.  Handled entirely
        /// on the UI thread (no worker action); the worker no-ops it.
        UiCenterListingAt(u16),
        /// Set baseline settings + origins (0=Default,1=Saved,2=CLI) and optional GPIO color
        UiSetSettingsBaseline(SettingsState, Vec<(String, u8)>, Option<u32>),
        /// Worker acknowledgement that Apply/Save completed for a dialog instance
        UiAckSettings {
            instance_id: u64,
        },
        ApplyConsole {
            enable: bool,
            base: Option<u16>,
            out_path: Option<String>,
            stdin: bool,
            echo: bool,
            flush_every: Option<usize>,
            flush_newline: bool,
            rx_watermark: Option<usize>,
            irq_hold: Option<u32>,
            tee_stderr: bool,
        },
        ApplyBlock {
            enable: bool,
            base: Option<u16>,
            file_path: Option<String>,
            irq_enable: bool,
            firq: bool,
            irq_hold: Option<u32>,
        },
        ApplyGpio {
            enable: bool,
            base: Option<u16>,
            bits: Option<u8>,
        },
        ApplyTimer {
            enable: bool,
            base: Option<u16>,
            reload: Option<u16>,
            irq_enable: bool,
            firq: bool,
            start: bool,
        },
        // MMU controls (Phase 1)
        ApplyMmu {
            sys_mode: Option<bool>,
            task: Option<u8>,
            prot: Option<u8>,
            log_maps: Option<bool>,
            maps_sys: Vec<(usize, u16)>,  // (page, frame)
            maps_user: Vec<(usize, u16)>, // (page, frame)
            attrs_sys: Vec<(usize, u8)>,  // (page, attr)
            attrs_user: Vec<(usize, u8)>, // (page, attr)
        },
        MmuIdentityMap,
        // MMU controls (Phase 2)
        ApplyMmuPreset {
            name: String,
        },
        ApplyMmuMapFileText {
            text: String,
        },
        /// Persist settings snapshot to disk
        SavePrefs {
            settings: SettingsState,
            gpio_on_color: u32,
            instance_id: u64,
        },
    }

    pub(super) static GLOBAL_CMDS: OnceCell<Arc<Mutex<Vec<Command>>>> = OnceCell::new();
    pub(super) static GLOBAL_SETTINGS: OnceCell<
        Arc<Mutex<(SettingsState, HashMap<String, u8>, Option<u32>)>>,
    > = OnceCell::new();
    pub(super) static FORCE_IN_APP_SETTINGS: OnceCell<bool> = OnceCell::new();

    pub fn set_force_in_app_settings(v: bool) {
        let _ = FORCE_IN_APP_SETTINGS.set(v);
    }

    pub fn force_in_app_settings() -> bool {
        *FORCE_IN_APP_SETTINGS.get().unwrap_or(&false)
    }

    pub fn set_show_settings_origins(on: bool) {
        if let Some(flag) = SHOW_ORIGINS_OVERRIDE.get() {
            flag.store(on, Ordering::SeqCst);
        } else {
            let _ = SHOW_ORIGINS_OVERRIDE.set(AtomicBool::new(on));
        }
    }

    #[derive(Default)]
    pub(crate) struct ListingCache {
        pub rev: usize,
        pub line_offsets: Vec<usize>,
        pub line_count: usize,
        pub lossy_text: Option<String>,
    }

    impl ListingCache {
        pub fn force_refresh() -> Self {
            Self {
                rev: usize::MAX,
                line_offsets: Vec::new(),
                line_count: 0,
                lossy_text: None,
            }
        }
    }

    pub struct RegApp {
        pub snap: Arc<Mutex<RegSnapshot>>,
        pub paused: Arc<AtomicBool>,
        pub step_requests: Arc<AtomicUsize>,
        /// Pending Step Over requests (counter, multi-click queues up).
        /// Drained by the run loop in priority `Out > Over > Step` so a
        /// queued plain Step doesn't sneak ahead of an Over the user
        /// pressed afterwards.
        pub step_over_requests: Arc<AtomicUsize>,
        /// Pending Step Out requests. Same draining rules as
        /// `step_over_requests`.
        pub step_out_requests: Arc<AtomicUsize>,
        pub lst_mmap: Option<Arc<Mmap>>,
        pub lst_text: Option<Arc<Mutex<String>>>,
        pub lst_rev: Arc<AtomicUsize>,
        pub hex_rev: Arc<AtomicUsize>,
        pub lst_cache: ListingCache,
        pub hex_cache: ListingCache,
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
        // File open dialog state
        pub open_dialog_active: bool,
        pub open_pending_path: Option<std::path::PathBuf>,
        pub open_bin_base_text: String,
        #[cfg(target_os = "linux")]
        pub csd: bool,
        #[cfg(target_os = "linux")]
        pub csd_maximized: bool,
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
        pub console_input: Arc<Mutex<Vec<u8>>>,
        pub console_state: Arc<Mutex<ConsoleState>>,
        pub mem_jump_text: String,
        // GPIO LED color when ON
        pub gpio_on_color: egui::Color32,
        // Baseline color to support Revert in Settings
        pub gpio_on_color_baseline: egui::Color32,
        // Text field for direct hex input (e.g., 0xFF0000)
        pub gpio_on_color_text: String,
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
        pub block_present: bool,
        pub block_base: Option<u16>,
        pub block_backing_path: Option<String>,
        pub block_backing_online: bool,
        pub block_last_cmd: u8,
        pub block_last_data: u8,
        pub block_status: u8,
        // Emulator -> GUI sync (shared across threads)
        pub gpio_sync: Arc<Mutex<GpioSyncState>>,
        pub mmu_sync: Arc<Mutex<MmuSyncState>>,
        // MMU grid
        pub mmu_present: bool,
        pub mmu_sys_mode: bool,
        pub mmu_task: u8,
        pub mmu_map_prev_sys: [u16; 16],
        pub mmu_map_cur_sys: [u16; 16],
        pub mmu_map_prev_user: [u16; 16],
        pub mmu_map_cur_user: [u16; 16],
        pub mmu_view_sys: bool,
        pub timer_present: bool,
        pub timer_pending: bool,
        pub timer_running: bool,
        // Settings dialog
        pub settings_open: bool,
        pub settings_tab: usize,
        pub settings: SettingsState,
        // Last applied/saved settings snapshot (baseline for Revert)
        pub settings_baseline: SettingsState,
        // Bump this each time Settings opens to reset egui's remembered window size
        pub settings_instance: u64,
        pub settings_origins: HashMap<String, u8>,
        pub settings_busy: bool,
        pub settings_busy_since: Option<std::time::Instant>,
        // Console status (for quick panel order)
        pub console_enabled: bool,
        pub console_base: Option<u16>,
        pub console_gui_open: bool,
        pub dev_tab: DeviceTab,
        // Shared with the spawning RegView; the run loop writes here
        // after every step, the load handlers seed it via linear sweep,
        // and the live-disasm pane reads it to anchor on a confirmed
        // instruction boundary near the current PC.
        pub boundaries: Arc<Mutex<InstructionBoundaries>>,
        /// Snapshot of breakpoint UI state. Worker thread writes,
        /// list pane reads on every paint to render markers.
        pub bp_states: Arc<Mutex<Vec<(u16, BpUiState)>>>,
        /// Full breakpoint list snapshot for the breakpoints window.
        pub bp_list: Arc<Mutex<Vec<BpInfo>>>,
        /// Bumped by `Viewer::update_bp_list` whenever the worker
        /// rewrites the BP-panel snapshot.  Compared against
        /// `last_bp_list_rev` each frame; we only nudge the deferred
        /// Breakpoints viewport with `request_repaint_of` when this
        /// counter advanced.  Replaces the previous unconditional
        /// per-frame repaint pump, which on macOS caused the BP / Call
        /// Stack windows to monopolise key input.
        pub bp_list_rev: Arc<AtomicU64>,
        /// Last `bp_list_rev` value the GUI thread acted on.  Local
        /// to `RegApp`; not shared with the worker.
        pub last_bp_list_rev: u64,
        /// Shadow call stack snapshot for the call-stack window.
        pub call_stack: Arc<Mutex<Vec<CallFrameInfo>>>,
        /// See `bp_list_rev`.  Bumped by `Viewer::update_call_stack`.
        pub call_stack_rev: Arc<AtomicU64>,
        /// Last `call_stack_rev` value the GUI thread acted on.
        pub last_call_stack_rev: u64,
        /// Open/closed state of the standalone Breakpoints window.
        /// Toggled from the View menu; the X close button on the
        /// window itself flips it back via egui's `.open(&mut bool)`.
        pub bp_window_open: bool,
        /// Open/closed state of the standalone Call Stack window.
        pub call_stack_window_open: bool,
        /// One-shot listing-pane recentre target, set by the BP / CS
        /// double-click handlers via [`Command::UiCenterListingAt`].
        /// `list_ui` consumes it on the next paint: finds the line
        /// matching the address, scrolls it into view (centered), and
        /// clears the field back to `None`.  Independent of the
        /// follow-PC scroll path so jumping from a panel does not
        /// implicitly toggle Follow PC.
        pub pending_listing_center: Option<u16>,
    }

    impl RegApp {
        fn spawn_image_loader(&self, path: std::path::PathBuf, base: Option<u16>) {
            let cmds = Arc::clone(&self.cmds);
            #[cfg(windows)]
            {
                if let Ok(mut q) = cmds.lock() {
                    q.push(Command::OpenImage { path, base });
                }
            };
            #[cfg(not(windows))]
            std::thread::spawn(move || {
                let lower = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let parsed = if lower == "s19" || lower == "srec" {
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            match crate::loader::parse_srec(&String::from_utf8_lossy(&bytes)) {
                                Ok(img) => Ok(img),
                                Err(e) => Err(format!("Open failed (SREC): {e}")),
                            }
                        }
                        Err(e) => Err(format!("Open failed (read): {e}")),
                    }
                } else if lower == "bin" {
                    match std::fs::read(&path) {
                        Ok(bytes) => Ok(crate::loader::parse_binary(base.unwrap_or(0), &bytes)),
                        Err(e) => Err(format!("Open failed (read): {e}")),
                    }
                } else {
                    Err(format!("Unsupported extension for Open: {}", lower))
                };
                match parsed {
                    Ok(parsed) => {
                        if let Ok(mut q) = cmds.lock() {
                            q.push(Command::OpenImageParsed { path, parsed });
                        }
                    }
                    Err(e) => {
                        eprintln!("{e}");
                    }
                }
            });
        }

        fn open_in_app_settings(&mut self) {
            // Open and reset fields to baseline on each open
            self.settings = self.settings_baseline.clone();
            // Also reset color to its baseline for the GPIO tab
            self.gpio_on_color = self.gpio_on_color_baseline;
            self.gpio_on_color_text = format_color_hex(self.gpio_on_color);
            // Sync console quick-panel state from baseline
            self.console_enabled = self.settings_baseline.use_console;
            self.console_base = parse_u16_any(&self.settings_baseline.console_base_hex);
            let instance = settings_worker::next_dialog_instance();
            self.settings_instance = instance;
            settings_worker::set_active_instance(instance);
            self.settings_open = true;
            self.settings_busy = false;
            self.settings_busy_since = None;
        }
    }

    impl App for RegApp {
        fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
            // Initialize color text once if empty (for first render before opening settings)
            if self.gpio_on_color_text.is_empty() {
                self.gpio_on_color_text = format_color_hex(self.gpio_on_color);
            }
            let frame_ref = &*frame;
            if console_ui::take_console_closed() {
                self.console_gui_open = false;
            }
            // macOS: ensure GUI_CTX is set so external triggers can repaint
            #[cfg(target_os = "macos")]
            {
                let _ = GUI_CTX.get_or_init(|| ctx.clone());
            }
            #[cfg(target_os = "macos")]
            {
                if let Some(w) = PREFS_WAKE_TICKS.get() {
                    let ticks = w.load(Ordering::SeqCst);
                    if ticks > 0 {
                        ctx.request_repaint();
                        ctx.request_repaint_after(Duration::from_millis(10));
                        w.fetch_sub(1, Ordering::SeqCst);
                    }
                }
            }
            // Failsafe: if busy state lingers too long, clear it
            if self.settings_busy {
                if let Some(since) = self.settings_busy_since {
                    if since.elapsed().as_secs_f32() > 1.0 {
                        self.settings_busy = false;
                        self.settings_busy_since = None;
                    }
                }
            }
            // Apply any latest GPIO state snapshot (for cases where UiSetGpio is missed)
            if let Ok(mut g) = self.gpio_sync.lock() {
                if let Some((present, bits, out, dir)) = g.take() {
                    self.gpio_present = present;
                    self.gpio_bits = bits;
                    self.gpio_out = out;
                    self.gpio_dir = dir;
                }
            }
            // Apply latest GPIO snapshot broadcast from worker (fallback when commands drop)
            if let Some((present, bits, out, dir)) = em6809::io::peek_gpio_broadcast() {
                self.gpio_present = present;
                self.gpio_bits = bits;
                self.gpio_out = out;
                self.gpio_dir = dir;
            }
            // macOS: check pending Preferences request flag (App menu) and open in-app settings
            #[cfg(target_os = "macos")]
            if let Some(flag) = PREFS_REQUEST.get() {
                if flag.swap(false, Ordering::SeqCst) {
                    if crate::settings_native::settings_debug_enabled() {
                        eprintln!("[macos-debug] prefs flag set -> open_in_app_settings()");
                    }
                    self.open_in_app_settings();
                    ctx.request_repaint();
                    ctx.request_repaint_after(Duration::from_millis(16));
                }
            }
            // Drain UI sync commands injected by emulator thread
            let drained_cmds = if let Ok(mut q) = self.cmds.lock() {
                q.drain(..).collect()
            } else {
                Vec::new()
            };
            let mut rest: Vec<Command> = Vec::new();
            for cmd in drained_cmds {
                match cmd {
                    Command::UiOpenSettings => {
                        self.open_in_app_settings();
                    }
                    Command::UiOpenSettingsExternal => {
                        self.open_in_app_settings();
                    }
                    Command::UiToggleBreakpointsWindow => {
                        self.bp_window_open = !self.bp_window_open;
                    }
                    Command::UiToggleCallStackWindow => {
                        self.call_stack_window_open = !self.call_stack_window_open;
                    }
                    Command::UiCenterListingAt(addr) => {
                        // Recentre listing on `addr`. We park it as
                        // `pending_listing_center`; list_ui.rs reads
                        // it on the next paint, scrolls there, and
                        // clears the field.  Re-enabling Follow PC
                        // would fight this jump on the next pause
                        // edge, so leave it untouched — the user
                        // can re-arm it from the toolbar.
                        self.pending_listing_center = Some(addr);
                    }
                    Command::UiSetSettingsBaseline(s, origins, color_opt) => {
                        let prev_console_gui = self.settings_baseline.console_gui;
                        self.settings_baseline = s.clone();
                        // Update origins map
                        self.settings_origins.clear();
                        for (k, v) in origins.into_iter() {
                            self.settings_origins.insert(k, v);
                        }
                        // Settings were just applied; clear busy state
                        self.settings_busy = false;
                        self.settings_busy_since = None;
                        // Update quick-panel console state from new baseline
                        self.console_enabled = self.settings_baseline.use_console;
                        self.console_base = parse_u16_any(&self.settings_baseline.console_base_hex);
                        if self.settings_baseline.console_gui {
                            if !prev_console_gui {
                                self.console_gui_open = true;
                            }
                        } else {
                            self.console_gui_open = false;
                        }
                        // publish to globals
                        let glob = GLOBAL_SETTINGS.get_or_init(|| {
                            Arc::new(Mutex::new((SettingsState::default(), HashMap::new(), None)))
                        });
                        if let Ok(mut g) = glob.lock() {
                            *g = (
                                self.settings_baseline.clone(),
                                self.settings_origins.clone(),
                                color_opt,
                            );
                        }
                        // Also refresh current fields even if dialog is open so UI matches applied preset
                        // Keep current preset selection if it was set locally and baseline is empty.
                        if !self.settings.mmu_preset_name.is_empty() && s.mmu_preset_name.is_empty()
                        {
                            self.settings = self.settings_baseline.clone();
                            self.settings.mmu_preset_name =
                                self.settings_baseline.mmu_preset_name.clone();
                        } else {
                            self.settings = self.settings_baseline.clone();
                        }
                        if let Some(rgb) = color_opt {
                            let col = egui::Color32::from_rgb(
                                ((rgb >> 16) & 0xFF) as u8,
                                ((rgb >> 8) & 0xFF) as u8,
                                (rgb & 0xFF) as u8,
                            );
                            self.gpio_on_color_baseline = col;
                            self.gpio_on_color = col;
                            self.gpio_on_color_text = format_color_hex(col);
                        }
                    }
                    Command::UiAckSettings { instance_id } => {
                        if instance_id == self.settings_instance {
                            self.settings_busy = false;
                            self.settings_busy_since = None;
                            // In case UiSetSettingsBaseline was dropped, refresh from latest baseline.
                            if let Some((s, origins, color_opt)) = get_latest_settings_baseline() {
                                let prev_console_gui = self.settings_baseline.console_gui;
                                self.settings_baseline = s.clone();
                                self.settings_origins = origins;
                                self.console_enabled = self.settings_baseline.use_console;
                                self.console_base =
                                    parse_u16_any(&self.settings_baseline.console_base_hex);
                                if self.settings_baseline.console_gui {
                                    if !prev_console_gui {
                                        self.console_gui_open = true;
                                    }
                                } else {
                                    self.console_gui_open = false;
                                }
                                if !self.settings_open {
                                    self.settings = s;
                                }
                                if let Some(rgb) = color_opt {
                                    let col = egui::Color32::from_rgb(
                                        ((rgb >> 16) & 0xFF) as u8,
                                        ((rgb >> 8) & 0xFF) as u8,
                                        (rgb & 0xFF) as u8,
                                    );
                                    self.gpio_on_color_baseline = col;
                                    if !self.settings_open {
                                        self.gpio_on_color = col;
                                        self.gpio_on_color_text = format_color_hex(col);
                                    }
                                }
                            }
                        }
                    }
                    Command::UiSetGpio(present, bits, out, dir) => {
                        self.gpio_present = present;
                        self.gpio_bits = bits;
                        self.gpio_out = out;
                        self.gpio_dir = dir;
                    }
                    Command::UiSetGpioOnColor(rgb) => {
                        self.gpio_on_color = egui::Color32::from_rgb(
                            ((rgb >> 16) & 0xFF) as u8,
                            ((rgb >> 8) & 0xFF) as u8,
                            (rgb & 0xFF) as u8,
                        );
                    }
                    Command::UiSetMmu(sys, task, map_sys, map_user) => {
                        let first = !self.mmu_present;
                        self.mmu_present = true;
                        self.mmu_sys_mode = sys;
                        self.mmu_task = task;
                        self.mmu_map_prev_sys = self.mmu_map_cur_sys;
                        self.mmu_map_cur_sys = map_sys;
                        self.mmu_map_prev_user = self.mmu_map_cur_user;
                        self.mmu_map_cur_user = map_user;
                        if first {
                            self.mmu_view_sys = sys;
                        }
                    }
                    Command::UiSetTimer(present, pending, running) => {
                        self.timer_running = running;

                        self.timer_present = present;
                        self.timer_pending = pending;
                    }
                    Command::UiSetBlockStatus {
                        present,
                        base,
                        backing_path,
                        backing_online,
                        last_cmd,
                        last_data,
                        status,
                    } => {
                        self.block_present = present;
                        self.block_base = base;
                        self.block_backing_path = backing_path;
                        self.block_backing_online = backing_online;
                        self.block_last_cmd = last_cmd;
                        self.block_last_data = last_data;
                        self.block_status = status;
                        // Keep Settings dialog state aligned with actual device presence
                        self.settings_baseline.use_block = present;
                        if !self.settings_open {
                            self.settings.use_block = present;
                        }
                        if let Some(glob) = GLOBAL_SETTINGS.get() {
                            if let Ok(mut g) = glob.lock() {
                                g.0.use_block = present;
                            }
                        }
                    }
                    Command::ApplyMmuPreset { .. } => { /* worker-handled */ }
                    Command::ApplyMmuMapFileText { .. } => { /* worker-handled */ }
                    Command::UiPromptBinBase(path) => {
                        self.open_pending_path = Some(path);
                        self.open_bin_base_text = "0x0000".to_string();
                        self.open_dialog_active = true;
                    }
                    Command::UiOpenImageDialog => {
                        // Pause before opening modal dialog to prevent console buffer growth.
                        self.paused.store(true, Ordering::SeqCst);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        let file = rfd::FileDialog::new()
                            .set_parent(frame_ref)
                            .add_filter("Images", &["s19", "srec", "bin"])
                            .pick_file();
                        if let Some(path) = file {
                            self.paused.store(true, Ordering::SeqCst);
                            let ext = path
                                .extension()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            if ext == "bin" {
                                self.open_pending_path = Some(path);
                                self.open_bin_base_text = "0x0000".to_string();
                                self.open_dialog_active = true;
                            } else {
                                self.spawn_image_loader(path, None);
                            }
                        }
                    }
                    Command::OpenImageParsed { .. } => {
                        // Worker-only; leave for CPU thread.
                        rest.push(cmd);
                    }
                    Command::OpenImage { .. } => {
                        if let Ok(mut st) = self.console_state.lock() {
                            st.commit_screen_to_history();
                        }
                        rest.push(cmd);
                    }
                    Command::UiLoadLst(path) => {
                        // Map .lst and switch to LST view immediately
                        if let Ok(f) = std::fs::File::open(&path) {
                            if let Ok(m) = unsafe { Mmap::map(&f) } {
                                self.lst_mmap = Some(Arc::new(m));
                                self.lst_rev.fetch_add(1, Ordering::SeqCst);
                                self.lst_cache = ListingCache::force_refresh();
                                self.use_hex.store(false, Ordering::SeqCst);
                                // Center listing around current PC after switching views
                                self.scroll_to_pc_req.store(true, Ordering::SeqCst);
                            }
                        }
                    }
                    other => rest.push(other),
                }
            }
            if !rest.is_empty() {
                if let Ok(mut q) = self.cmds.lock() {
                    q.extend(rest);
                }
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

            // Keep scheduling native Settings viewport for a few frames after request (macOS multi-viewport create)
            // If we are forcing the in-app dialog (Windows fallback), skip pumping native viewports.
            if !force_in_app_settings() {
                settings::pump_native_settings(ctx);
            }

            // macOS: set app menu title, install About override, and install native system menubar once.
            #[cfg(target_os = "macos")]
            if !self.menu_fixed {
                em6809::platform::macos::set_app_menu_title("em6809");
                em6809::platform::macos::install_about_handler_once();
                {
                    em6809::platform::macos::install_preferences_handler_once(Box::new(
                        move || {
                            #[cfg(target_os = "macos")]
                            {
                                if crate::settings_native::settings_debug_enabled() {
                                    eprintln!(
                                        "[macos-debug] Preferences handler fired -> flag + cmd"
                                    );
                                }
                                crate::regview::request_prefs_open();
                                let _ =
                                    crate::regview::push_global_command(Command::UiOpenSettings);
                            }
                        },
                    ));
                }
                let cmds = self.cmds.clone();
                em6809::platform::macos::install_global_menubar_once(Box::new(move || {
                    // Runs on main thread via ObjC target/selector
                    let mut pushed = false;
                    if let Ok(mut q) = cmds.lock() {
                        q.push(Command::UiOpenImageDialog);
                        pushed = true;
                    }
                    if !pushed {
                        let _ = crate::regview::push_global_command(Command::UiOpenImageDialog);
                    }
                    if let Some(ctx) = super::GUI_CTX.get() {
                        ctx.request_repaint();
                        ctx.request_repaint_after(Duration::from_millis(10));
                    }
                }));
                let cmds_bp = self.cmds.clone();
                let cmds_cs = self.cmds.clone();
                em6809::platform::macos::install_view_menu_once(
                    Box::new(move || {
                        if let Ok(mut q) = cmds_bp.lock() {
                            q.push(Command::UiToggleBreakpointsWindow);
                        } else {
                            let _ = crate::regview::push_global_command(
                                Command::UiToggleBreakpointsWindow,
                            );
                        }
                        if let Some(ctx) = super::GUI_CTX.get() {
                            ctx.request_repaint();
                        }
                    }),
                    Box::new(move || {
                        if let Ok(mut q) = cmds_cs.lock() {
                            q.push(Command::UiToggleCallStackWindow);
                        } else {
                            let _ = crate::regview::push_global_command(
                                Command::UiToggleCallStackWindow,
                            );
                        }
                        if let Some(ctx) = super::GUI_CTX.get() {
                            ctx.request_repaint();
                        }
                    }),
                );
                self.menu_fixed = true;
            }
            // macOS: keep View menu checkmarks in sync with the actual
            // window-open flags. Cheap and idempotent.
            #[cfg(target_os = "macos")]
            em6809::platform::macos::set_view_menu_check_states(
                self.bp_window_open,
                self.call_stack_window_open,
            );

            // (timer indicator top strip removed; shown below MMU panel)

            // Windows: simple menu bar with About
            #[cfg(windows)]
            {
                eframe::egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
                    eframe::egui::menu::bar(ui, |ui| {
                        ui.menu_button("File", |ui| {
                            if ui.button("Open…").clicked() {
                                self.paused.store(true, Ordering::SeqCst);
                                let file = rfd::FileDialog::new()
                                    .set_parent(frame_ref)
                                    .add_filter("Images", &["s19", "srec", "bin"])
                                    .pick_file();
                                if let Some(path) = file {
                                    let ext = path
                                        .extension()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    if ext == "bin" {
                                        self.open_pending_path = Some(path);
                                        self.open_bin_base_text = "0x0000".to_string();
                                        self.open_dialog_active = true;
                                    } else {
                                        self.spawn_image_loader(path, None);
                                    }
                                }
                                ui.close_menu();
                            }
                            if ui.button("Settings…").clicked() {
                                self.open_in_app_settings();
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Quit").clicked() {
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                ui.close_menu();
                            }
                        });
                        ui.menu_button("View", |ui| {
                            // Checkbox-style toggle so the menu shows
                            // current visibility at a glance.
                            ui.checkbox(&mut self.bp_window_open, "Breakpoints");
                            ui.checkbox(&mut self.call_stack_window_open, "Call Stack");
                        });
                        ui.menu_button("Help", |ui| {
                            if ui.button("About em6809…").clicked() {
                                self.about_open = true;
                                ui.close_menu();
                            }
                        });
                    });
                });
                if self.about_open {
                    em6809::platform::windows::ensure_about_icon(ctx, &mut self.about_icon_tex);
                    em6809::platform::windows::show_about_dialog(
                        ctx,
                        &mut self.about_open,
                        &self.about_icon_tex,
                    );
                }
            }

            // Linux: experimental CSD header or legacy simple menu bar
            #[cfg(target_os = "linux")]
            {
                if self.csd {
                    let mut csd_max = self.csd_maximized;
                    // Defer opening settings until after header rendering to avoid borrow issues
                    let mut open_settings_req = false;
                    em6809::platform::linux::csd::show_header(ctx, &mut csd_max, |ui| {
                        ui.menu_button("File", |ui| {
                            if ui.button("Open…").clicked() {
                                self.paused.store(true, Ordering::SeqCst);
                                let file = rfd::FileDialog::new()
                                    .set_parent(frame_ref)
                                    .add_filter("Images", &["s19", "srec", "bin"])
                                    .pick_file();
                                if let Some(path) = file {
                                    let ext = path
                                        .extension()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    if ext == "bin" {
                                        self.open_pending_path = Some(path);
                                        self.open_bin_base_text = "0x0000".to_string();
                                        self.open_dialog_active = true;
                                    } else {
                                        self.spawn_image_loader(path, None);
                                    }
                                }
                                ui.close_menu();
                            }
                            if ui.button("Settings…").clicked() {
                                open_settings_req = true;
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Quit").clicked() {
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                ui.close_menu();
                            }
                        });
                        ui.menu_button("View", |ui| {
                            ui.checkbox(&mut self.bp_window_open, "Breakpoints");
                            ui.checkbox(&mut self.call_stack_window_open, "Call Stack");
                        });
                        ui.menu_button("Help", |ui| {
                            if ui.button("About em6809…").clicked() {
                                self.about_open = true;
                                ui.close_menu();
                            }
                        });
                    });
                    self.csd_maximized = csd_max;
                    if open_settings_req {
                        self.open_in_app_settings();
                    }
                } else {
                    eframe::egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
                        eframe::egui::menu::bar(ui, |ui| {
                            ui.menu_button("File", |ui| {
                                if ui.button("Open…").clicked() {
                                    self.paused.store(true, Ordering::SeqCst);
                                    let file = rfd::FileDialog::new()
                                        .set_parent(frame_ref)
                                        .add_filter("Images", &["s19", "srec", "bin"])
                                        .pick_file();
                                    if let Some(path) = file {
                                        let ext = path
                                            .extension()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("")
                                            .to_lowercase();
                                        if ext == "bin" {
                                            self.open_pending_path = Some(path);
                                            self.open_bin_base_text = "0x0000".to_string();
                                            self.open_dialog_active = true;
                                        } else {
                                            self.spawn_image_loader(path, None);
                                        }
                                    }
                                    ui.close_menu();
                                }
                                if ui.button("Settings…").clicked() {
                                    self.open_in_app_settings();
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button("Quit").clicked() {
                                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                    ui.close_menu();
                                }
                            });
                            ui.menu_button("View", |ui| {
                                ui.checkbox(&mut self.bp_window_open, "Breakpoints");
                                ui.checkbox(&mut self.call_stack_window_open, "Call Stack");
                            });
                            ui.menu_button("Help", |ui| {
                                if ui.button("About em6809…").clicked() {
                                    self.about_open = true;
                                    ui.close_menu();
                                }
                            });
                        });
                    });
                }
                if self.about_open {
                    em6809::platform::linux::about::ensure_about_icon(
                        ctx,
                        &mut self.about_icon_tex,
                    );
                    em6809::platform::linux::about::show_about_dialog(
                        ctx,
                        &mut self.about_open,
                        &self.about_icon_tex,
                    );
                }
            }

            // Windows: no title override to keep window caption unchanged

            // If external quit is requested (e.g., Ctrl+C), close the window
            if self.quit_flag.load(Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            // Windows: ensure title-bar close triggers a full shutdown
            #[cfg(windows)]
            {
                let input = ctx.input(|i| i.viewport().clone());
                if input.close_requested() {
                    // Signal backend loop to stop and close the viewport
                    self.quit_flag.store(true, Ordering::SeqCst);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }

            // Modal to request BIN base address after selecting a file
            if self.open_dialog_active {
                egui::Window::new("Open Binary: Base Address")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label("Enter base address for .bin (e.g., 0xE000)");
                        ui.text_edit_singleline(&mut self.open_bin_base_text);
                        ui.horizontal(|ui| {
                            if ui.button("OK").clicked() {
                                if let Some(path) = self.open_pending_path.take() {
                                    let txt = self.open_bin_base_text.trim();
                                    let base = if let Some(h) =
                                        txt.strip_prefix("0x").or(txt.strip_prefix("0X"))
                                    {
                                        u16::from_str_radix(h, 16).ok()
                                    } else {
                                        txt.parse::<u16>().ok()
                                    };
                                    if let Some(b) = base {
                                        self.spawn_image_loader(path, Some(b));
                                        self.open_dialog_active = false;
                                    }
                                }
                            }
                            if ui.button("Cancel").clicked() {
                                self.open_dialog_active = false;
                                self.open_pending_path = None;
                            }
                        });
                    });
            }

            settings_ui::render_settings_dialog(self, ctx, frame);

            let prev_edit_mode = self.edit_mode;
            mem_ui::render_memory_panel(self, ctx);
            list_ui::render_listing_panel(self, ctx);

            regs_ui::render_regs_panel(self, ctx);

            // Real OS-level child windows for breakpoints / call stack
            // (eframe viewport API — same pattern as the VT100 console).
            // The View-menu checkbox writes `self.bp_window_open`;
            // the OS window's X button is reconciled here by
            // consuming the module's "closed" flag.
            if breakpoints_ui::take_closed() {
                self.bp_window_open = false;
            }
            if call_stack_ui::take_closed() {
                self.call_stack_window_open = false;
            }
            // Nudge the deferred BP / Call Stack viewports to repaint
            // only when the worker rewrote their snapshots.  The
            // previous "pump every frame" approach burned cycles on
            // Linux and confused AppKit on macOS.
            if self.bp_window_open {
                let cur = self.bp_list_rev.load(Ordering::SeqCst);
                if cur != self.last_bp_list_rev {
                    ctx.request_repaint_of(breakpoints_ui::breakpoints_viewport_id());
                    self.last_bp_list_rev = cur;
                }
            }
            if self.call_stack_window_open {
                let cur = self.call_stack_rev.load(Ordering::SeqCst);
                if cur != self.last_call_stack_rev {
                    ctx.request_repaint_of(call_stack_ui::call_stack_viewport_id());
                    self.last_call_stack_rev = cur;
                }
            }
            breakpoints_ui::show_breakpoints_viewport(
                ctx,
                Arc::clone(&self.bp_list),
                Arc::clone(&self.cmds),
                self.bp_window_open,
            );
            call_stack_ui::show_call_stack_viewport(
                ctx,
                Arc::clone(&self.call_stack),
                Arc::clone(&self.cmds),
                self.call_stack_window_open,
            );

            // macOS: stop AppKit from promoting auxiliary NSWindows
            // to key window during eframe's paint cycle, which would
            // otherwise monopolise keyboard input from the main
            // window.  Two flavours:
            //
            //   * Call Stack — read-only labels, never needs
            //     keystrokes.  `mark_window_non_activating` makes it
            //     unconditionally non-key.
            //   * Breakpoints + Console — both have `TextEdit`s
            //     (BP condition / Console terminal input) that need
            //     keystrokes when the user is editing them.
            //     `mark_window_click_activated` drops them to non-key
            //     on resign and bumps them back to key on a user
            //     mouse-down inside the window.  Each gets its own
            //     internal flag so they don't influence each other.
            //
            // Idempotent — safe to call every frame, only marks each
            // NSWindow once.
            #[cfg(target_os = "macos")]
            {
                em6809::platform::macos::mark_window_click_activated("em6809 Breakpoints");
                em6809::platform::macos::mark_window_non_activating("em6809 Call Stack");
                let console_title_for_swizzle =
                    if self.settings_baseline.console_gui_title.trim().is_empty() {
                        "em6809 Console".to_string()
                    } else {
                        self.settings_baseline.console_gui_title.clone()
                    };
                em6809::platform::macos::mark_window_click_activated(&console_title_for_swizzle);
            }

            // Editing is user-controlled via Edit/Apply toggle (not tied to pause)
            let editing = self.edit_mode;
            // Initialize edit buffers from current snapshot the first time we enter editing mode
            if editing && (!self.edit_initialized || !prev_edit_mode) {
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
            } else if !editing && self.edit_initialized {
                // When leaving editing mode, reset so we sync from latest snapshot next pause
                self.edit_initialized = false;
            }

            let console_title = if self.settings_baseline.console_gui_title.trim().is_empty() {
                "em6809 Console".to_string()
            } else {
                self.settings_baseline.console_gui_title.clone()
            };
            let console_font_size = self
                .settings_baseline
                .console_gui_font_size
                .trim()
                .parse::<f32>()
                .unwrap_or(14.0)
                .clamp(8.0, 32.0);
            let console_cols = parse_num_usize_any_gui(&self.settings_baseline.console_gui_cols)
                .unwrap_or(80)
                .clamp(20, 300) as u16;
            let console_rows = parse_num_usize_any_gui(&self.settings_baseline.console_gui_rows)
                .unwrap_or(24)
                .clamp(10, 120) as u16;
            let console_scrollback =
                parse_num_usize_any_gui(&self.settings_baseline.console_gui_scrollback)
                    .unwrap_or(1000)
                    .clamp(0, 20000);
            let console_cfg = console_ui::ConsoleConfig {
                enabled: self.settings_baseline.console_gui && self.console_gui_open,
                title: console_title,
                font_size: console_font_size,
                cols: console_cols,
                rows: console_rows,
                scrollback: console_scrollback,
                ctrl_keys: self.settings_baseline.console_gui_ctrl_keys,
            };
            console_ui::show_console_viewport(
                ctx,
                self.console_state.clone(),
                self.console_input.clone(),
                console_cfg,
            );

            // Keep the UI ticking while emulation is running so highlights and panels update.
            if !self.paused.load(Ordering::SeqCst) {
                ctx.request_repaint();
            }
            // Ensure the UI keeps repainting while the console window is visible.
            if self.settings_baseline.console_gui && self.console_gui_open {
                ctx.request_repaint_after(Duration::from_millis(33));
                let console_id = console_ui::console_viewport_id();
                ctx.request_repaint_of(console_id);
                ctx.request_repaint_after_for(Duration::from_millis(33), console_id);
                console_ui::request_console_repaint(ctx);
                console_ui::poke_console_viewport(ctx);
                let ctx_clone = ctx.clone();
                let cb = std::sync::Arc::new(move || {
                    ctx_clone.request_repaint();
                    ctx_clone.request_repaint_of(console_id);
                    ctx_clone.request_repaint_after_for(Duration::from_millis(16), console_id);
                    console_ui::poke_console_viewport(&ctx_clone);
                });
                em6809::io::set_console_repaint_callback(Some(cb));
            } else {
                em6809::io::set_console_repaint_callback(None);
            }
        }
    }

    pub struct RegView {
        pub snapshot: Arc<Mutex<RegSnapshot>>,
        pub paused: Arc<AtomicBool>,
        pub step_requests: Arc<AtomicUsize>,
        /// Pending Step Over requests. Mirrored into `RegApp` for the
        /// UI thread; the worker drains via `take_step_action`.
        pub step_over_requests: Arc<AtomicUsize>,
        /// Pending Step Out requests. Same lifetime as
        /// `step_over_requests`.
        pub step_out_requests: Arc<AtomicUsize>,
        pub lst_mmap: Option<Arc<Mmap>>,
        pub lst_text: Option<Arc<Mutex<String>>>,
        pub lst_rev: Arc<AtomicUsize>,
        pub hex_rev: Arc<AtomicUsize>,
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
        console_input: Arc<Mutex<Vec<u8>>>,
        console_state: Arc<Mutex<ConsoleState>>,
        quit_flag: Arc<AtomicBool>,
        alive: Arc<AtomicBool>,
        started: Arc<AtomicBool>,
        // Emulator->GUI sync
        gpio_sync: Arc<Mutex<GpioSyncState>>,
        mmu_sync: Arc<Mutex<MmuSyncState>>,
        dev_tab: DeviceTab,
        // Confirmed instruction-start addresses, populated by the run
        // loop (per-step) and by the load handlers (linear sweep). Used
        // by the live-disasm pane to anchor on a real boundary instead
        // of guessing with `pc - constant`. See em6809::debug.
        boundaries: Arc<Mutex<InstructionBoundaries>>,
        /// UI-visible snapshot of the current breakpoint set. Keyed by
        /// address so a per-row lookup in the list pane is cheap.
        /// The worker thread is the writer; the GUI thread reads.
        bp_states: Arc<Mutex<Vec<(u16, BpUiState)>>>,
        /// Full breakpoint list for the breakpoints panel. Contains
        /// id / addr / enabled / hit_count / ignore_count / condition
        /// for each registered BP. Pushed by the worker on the same
        /// pause-edge events as bp_states.
        bp_list: Arc<Mutex<Vec<BpInfo>>>,
        /// Bumped by `update_bp_list` so the GUI thread can detect
        /// when the deferred Breakpoints viewport needs a repaint
        /// without polling the list contents on every frame.
        bp_list_rev: Arc<AtomicU64>,
        /// Snapshot of the shadow call stack for the call-stack panel.
        /// Frames are in push order (oldest → newest). Pushed on
        /// pause-edge events.
        call_stack: Arc<Mutex<Vec<CallFrameInfo>>>,
        /// Bumped by `update_call_stack`. See `bp_list_rev`.
        call_stack_rev: Arc<AtomicU64>,
    }

    impl Clone for RegView {
        fn clone(&self) -> Self {
            Self {
                snapshot: Arc::clone(&self.snapshot),
                paused: Arc::clone(&self.paused),
                step_requests: Arc::clone(&self.step_requests),
                step_over_requests: Arc::clone(&self.step_over_requests),
                step_out_requests: Arc::clone(&self.step_out_requests),
                lst_mmap: self.lst_mmap.as_ref().map(Arc::clone),
                lst_text: self.lst_text.as_ref().map(Arc::clone),
                lst_rev: Arc::clone(&self.lst_rev),
                hex_rev: Arc::clone(&self.hex_rev),
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
                console_input: Arc::clone(&self.console_input),
                console_state: Arc::clone(&self.console_state),
                quit_flag: Arc::clone(&self.quit_flag),
                alive: Arc::clone(&self.alive),
                started: Arc::clone(&self.started),
                gpio_sync: Arc::new(Mutex::new(None)),
                mmu_sync: Arc::new(Mutex::new(None)),
                dev_tab: self.dev_tab,
                boundaries: Arc::clone(&self.boundaries),
                bp_states: Arc::clone(&self.bp_states),
                bp_list: Arc::clone(&self.bp_list),
                bp_list_rev: Arc::clone(&self.bp_list_rev),
                call_stack: Arc::clone(&self.call_stack),
                call_stack_rev: Arc::clone(&self.call_stack_rev),
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
            let step_over_requests = Arc::new(AtomicUsize::new(0));
            let step_out_requests = Arc::new(AtomicUsize::new(0));
            let lst_mmap: Option<Arc<Mmap>> = lst_path.and_then(|p| {
                if let Ok(f) = File::open(p) {
                    unsafe { Mmap::map(&f).ok() }.map(Arc::new)
                } else {
                    None
                }
            });
            let lst_text: Option<Arc<Mutex<String>>> = lst_fallback_text
                .map(|s| Arc::new(Mutex::new(s)))
                .or_else(|| Some(Arc::new(Mutex::new(String::new()))));
            let lst_rev = Arc::new(AtomicUsize::new(0));
            let hex_rev = Arc::new(AtomicUsize::new(0));
            let default_use_hex_view: bool = lst_mmap.is_none();
            let use_hex = Arc::new(AtomicBool::new(default_use_hex_view));
            let auto_hex_on_edit = Arc::new(AtomicBool::new(true));
            let scroll_to_pc_req = Arc::new(AtomicBool::new(false));
            let follow_pc_flag = Arc::new(AtomicBool::new(false));
            let span_policy = Arc::new(AtomicUsize::new(0)); // FullSpan
            let window_len = Arc::new(AtomicUsize::new(256));
            let cmds: Arc<Mutex<Vec<Command>>> = Arc::new(Mutex::new(Vec::new()));

            // Memory view defaults
            let mem_follow_pc = Arc::new(AtomicBool::new(false));
            let mem_window_len = Arc::new(AtomicUsize::new(256));
            let mem_window_start = Arc::new(AtomicUsize::new(initial_start as usize));
            let mem_buf: Arc<Mutex<(u16, Vec<u8>)>> = Arc::new(Mutex::new((0, Vec::new())));
            let mem_hl: Arc<Mutex<Option<(u16, usize)>>> = Arc::new(Mutex::new(None));
            let mem_utf8 = Arc::new(AtomicBool::new(false));
            let console_input = Arc::new(Mutex::new(Vec::new()));
            let console_state = Arc::new(Mutex::new(ConsoleState::new(24, 80, 1000)));
            let boundaries = Arc::new(Mutex::new(InstructionBoundaries::new()));
            let bp_states: Arc<Mutex<Vec<(u16, BpUiState)>>> = Arc::new(Mutex::new(Vec::new()));
            let bp_list: Arc<Mutex<Vec<BpInfo>>> = Arc::new(Mutex::new(Vec::new()));
            let bp_list_rev: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
            let call_stack: Arc<Mutex<Vec<CallFrameInfo>>> = Arc::new(Mutex::new(Vec::new()));
            let call_stack_rev: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));

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
            let step_over_requests_clone = Arc::clone(&step_over_requests);
            let step_out_requests_clone = Arc::clone(&step_out_requests);
            let lst_clone = lst_mmap.as_ref().map(Arc::clone);
            let lst_text_clone = lst_text.as_ref().map(Arc::clone);
            let lst_rev_clone = Arc::clone(&lst_rev);
            let hex_rev_clone = Arc::clone(&hex_rev);
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
            let console_input_thread = console_input.clone();
            let console_state_thread = console_state.clone();
            let quit_flag_thread = Arc::new(AtomicBool::new(false));
            let quit_flag_thread_clone = quit_flag_thread.clone();
            let alive_flag = Arc::new(AtomicBool::new(false));
            let alive_flag_thread = alive_flag.clone();
            let started_flag = Arc::new(AtomicBool::new(false));
            let started_flag_thread = started_flag.clone();
            let auto_hex_on_edit_thread = Arc::clone(&auto_hex_on_edit);
            let scroll_to_pc_req_thread = Arc::clone(&scroll_to_pc_req);
            let boundaries_thread = Arc::clone(&boundaries);
            let bp_states_thread = Arc::clone(&bp_states);
            let bp_list_thread = Arc::clone(&bp_list);
            let bp_list_rev_thread = Arc::clone(&bp_list_rev);
            let call_stack_thread = Arc::clone(&call_stack);
            let call_stack_rev_thread = Arc::clone(&call_stack_rev);
            let handle = std::thread::spawn(move || {
                let mut native_options = eframe::NativeOptions::default();
                // Locale-aware default width: ja-JP toolbar labels
                // (ステップオーバー / 適用 / …) need ~580 px on the
                // registers central panel; en-US fits in ~420. Memory
                // (440) + Listing (600) are constant, so the window
                // total swings by ~120 px.
                let init_w = match (use_japanese, cfg!(windows)) {
                    (true, true) => 1600.0,
                    (true, false) => 1540.0,
                    (false, true) => 1500.0,
                    (false, false) => 1440.0,
                };
                let base = eframe::egui::ViewportBuilder::default()
                    .with_inner_size(eframe::egui::vec2(init_w, 700.0))
                    .with_min_inner_size(eframe::egui::vec2(init_w, 700.0))
                    .with_resizable(true);
                #[cfg(all(unix, not(target_os = "macos")))]
                let vp = {
                    #[cfg(target_os = "linux")]
                    {
                        let mut b = base.clone().with_app_id("em6809");
                        if std::env::var("EM6809_CSD")
                            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                            .unwrap_or(false)
                        {
                            b = b.with_decorations(false);
                        }
                        b
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        base.with_app_id("em6809")
                    }
                };
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
                    step_over_requests: step_over_requests_clone,
                    step_out_requests: step_out_requests_clone,
                    lst_mmap: lst_clone,
                    lst_text: lst_text_clone,
                    lst_rev: lst_rev_clone,
                    hex_rev: hex_rev_clone,
                    lst_cache: ListingCache::force_refresh(),
                    hex_cache: ListingCache::force_refresh(),
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
                    open_dialog_active: false,
                    open_pending_path: None,
                    open_bin_base_text: String::new(),
                    #[cfg(target_os = "linux")]
                    csd: std::env::var("EM6809_CSD")
                        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                        .unwrap_or(false),
                    #[cfg(target_os = "linux")]
                    csd_maximized: false,
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
                    console_input: console_input_thread.clone(),
                    console_state: console_state_thread.clone(),
                    mem_jump_text: format!("0x{initial_start:04X}"),
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
                    block_present: false,
                    block_base: None,
                    block_backing_path: None,
                    block_backing_online: false,
                    block_last_cmd: 0,
                    block_last_data: 0,
                    block_status: 0,
                    gpio_on_color: egui::Color32::LIGHT_YELLOW,
                    gpio_on_color_baseline: egui::Color32::LIGHT_YELLOW,
                    gpio_on_color_text: String::new(),
                    gpio_sync: Arc::new(Mutex::new(None)),
                    mmu_sync: Arc::new(Mutex::new(None)),
                    mmu_present: false,
                    mmu_sys_mode: false,
                    mmu_task: 0,
                    mmu_map_prev_sys: [0u16; 16],
                    mmu_map_cur_sys: [0u16; 16],
                    mmu_map_prev_user: [0u16; 16],
                    mmu_map_cur_user: [0u16; 16],
                    mmu_view_sys: false,
                    timer_present: false,
                    timer_pending: false,
                    timer_running: false,
                    settings_open: false,
                    settings_tab: 0,
                    settings: SettingsState::default(),
                    settings_baseline: SettingsState::default(),
                    settings_instance: 0,
                    settings_busy: false,
                    settings_busy_since: None,
                    settings_origins: HashMap::new(),
                    console_enabled: false,
                    console_base: None,
                    console_gui_open: true,
                    dev_tab: DeviceTab::Console,
                    boundaries: boundaries_thread,
                    bp_states: bp_states_thread,
                    bp_list: bp_list_thread,
                    bp_list_rev: bp_list_rev_thread,
                    last_bp_list_rev: 0,
                    call_stack: call_stack_thread,
                    call_stack_rev: call_stack_rev_thread,
                    last_call_stack_rev: 0,
                    bp_window_open: false,
                    call_stack_window_open: false,
                    pending_listing_center: None,
                };
                // Windows supports any-thread event loop via ExtWindows; run normally.
                // On Unix (incl. WSL2), winit may require main-thread; catch and fall back headless.
                {
                    use std::panic::{catch_unwind, AssertUnwindSafe};
                    // Suppress panic noise only during GUI bootstrap on Unix
                    let prev_hook = std::panic::take_hook();
                    std::panic::set_hook(Box::new(|_| {}));
                    started_flag_thread.store(true, Ordering::SeqCst);
                    alive_flag_thread.store(true, Ordering::SeqCst);
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                            let _ = eframe::run_native(
                                &title,
                                native_options,
                                Box::new(|cc| {
                                    #[cfg(target_os = "macos")]
                                    {
                                        if GUI_CTX.set(cc.egui_ctx.clone()).is_ok() {
                                            eprintln!("[macos-debug] GUI_CTX set from CreationContext");
                                        }
                                    }
                                    #[cfg(not(target_os = "macos"))]
                                    {
                                        let _ = cc;
                                    }
                                    Ok(Box::new(app))
                                }),
                            );
                    }))
                    .map_err(|_| {
                        eprintln!(
                            "[regview] GUI failed to start on this platform/thread. Running headless."
                        );
                    });
                    std::panic::set_hook(prev_hook);
                    alive_flag_thread.store(false, Ordering::SeqCst);
                    started_flag_thread.store(false, Ordering::SeqCst);
                }
            });

            Self {
                snapshot,
                paused,
                step_requests,
                step_over_requests,
                step_out_requests,
                lst_mmap,
                lst_text,
                lst_rev,
                hex_rev,
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
                console_input,
                console_state,
                quit_flag: quit_flag_thread,
                alive: alive_flag,
                started: started_flag,
                gpio_sync: Arc::new(Mutex::new(None)),
                mmu_sync: Arc::new(Mutex::new(None)),
                dev_tab: DeviceTab::Console,
                boundaries,
                bp_states,
                bp_list,
                bp_list_rev,
                call_stack,
                call_stack_rev,
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
            let step_over_requests = Arc::new(AtomicUsize::new(0));
            let step_out_requests = Arc::new(AtomicUsize::new(0));
            let lst_mmap: Option<Arc<Mmap>> = lst_path.and_then(|p| {
                if let Ok(f) = File::open(p) {
                    unsafe { Mmap::map(&f).ok() }.map(Arc::new)
                } else {
                    None
                }
            });
            let lst_text: Option<Arc<Mutex<String>>> =
                lst_fallback_text.map(|s| Arc::new(Mutex::new(s)));
            let lst_rev = Arc::new(AtomicUsize::new(0));
            let hex_rev = Arc::new(AtomicUsize::new(0));
            let default_use_hex_view: bool = lst_mmap.is_none();
            let use_hex = Arc::new(AtomicBool::new(default_use_hex_view));
            let auto_hex_on_edit = Arc::new(AtomicBool::new(true));
            let scroll_to_pc_req = Arc::new(AtomicBool::new(false));
            let follow_pc_flag = Arc::new(AtomicBool::new(false));
            let span_policy = Arc::new(AtomicUsize::new(0));
            let window_len = Arc::new(AtomicUsize::new(256));
            let cmds: Arc<Mutex<Vec<Command>>> = Arc::new(Mutex::new(Vec::new()));
            let mem_follow_pc = Arc::new(AtomicBool::new(false));
            let mem_window_len = Arc::new(AtomicUsize::new(256));
            let mem_window_start = Arc::new(AtomicUsize::new(initial_start as usize));
            let mem_buf: Arc<Mutex<(u16, Vec<u8>)>> = Arc::new(Mutex::new((0, Vec::new())));
            let mem_hl: Arc<Mutex<Option<(u16, usize)>>> = Arc::new(Mutex::new(None));
            let mem_utf8 = Arc::new(AtomicBool::new(false));
            let console_input = Arc::new(Mutex::new(Vec::new()));
            let console_state = Arc::new(Mutex::new(ConsoleState::new(24, 80, 1000)));
            let quit_flag = Arc::new(AtomicBool::new(false));
            let alive = Arc::new(AtomicBool::new(false));
            let started = Arc::new(AtomicBool::new(false));
            let boundaries = Arc::new(Mutex::new(InstructionBoundaries::new()));
            let bp_states: Arc<Mutex<Vec<(u16, BpUiState)>>> = Arc::new(Mutex::new(Vec::new()));
            let bp_list: Arc<Mutex<Vec<BpInfo>>> = Arc::new(Mutex::new(Vec::new()));
            let bp_list_rev: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
            let call_stack: Arc<Mutex<Vec<CallFrameInfo>>> = Arc::new(Mutex::new(Vec::new()));
            let call_stack_rev: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));

            // Font discovery (same as spawn)
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

            Self {
                snapshot,
                paused,
                step_requests,
                step_over_requests,
                step_out_requests,
                lst_mmap,
                lst_text,
                lst_rev,
                hex_rev,
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
                console_input,
                console_state,
                quit_flag,
                alive,
                started,
                gpio_sync: Arc::new(Mutex::new(None)),
                mmu_sync: Arc::new(Mutex::new(None)),
                dev_tab: DeviceTab::Console,
                boundaries,
                bp_states,
                bp_list,
                bp_list_rev,
                call_stack,
                call_stack_rev,
            }
        }

        pub fn run_blocking(&self, title: &str) {
            self.run_blocking_with_init(title, None);
        }

        pub fn run_blocking_with_init(&self, title: &str, init: Option<Box<dyn FnOnce() + Send>>) {
            let snap_clone = Arc::clone(&self.snapshot);
            let paused_clone = Arc::clone(&self.paused);
            let step_requests_clone = Arc::clone(&self.step_requests);
            let step_over_requests_clone = Arc::clone(&self.step_over_requests);
            let step_out_requests_clone = Arc::clone(&self.step_out_requests);
            let lst_clone = self.lst_mmap.as_ref().map(Arc::clone);
            let lst_text_clone = self.lst_text.as_ref().map(Arc::clone);
            let lst_rev_clone = Arc::clone(&self.lst_rev);
            let hex_rev_clone = Arc::clone(&self.hex_rev);
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
            let console_input_thread = self.console_input.clone();
            let console_state_thread = self.console_state.clone();
            let initial_start = self.mem_window_start.load(Ordering::SeqCst) as u16;
            let use_japanese = Self::is_japanese_locale();

            let mut native_options = eframe::NativeOptions::default();
            // Width is locale-aware so the registers central panel
            // doesn't end up needlessly wide in en-US: ja-JP labels
            // (ステップオーバー / 適用 / …) need ~520 px of toolbar
            // headroom, en-US needs ~420. Memory (440) + Listing (600)
            // are constant, so the window total swings by ~100 px.
            let init_w = if use_japanese { 1540.0 } else { 1440.0 };
            let base = eframe::egui::ViewportBuilder::default()
                .with_inner_size(eframe::egui::vec2(init_w, 700.0))
                .with_min_inner_size(eframe::egui::vec2(1100.0, 400.0))
                .with_resizable(false);
            #[cfg(all(unix, not(target_os = "macos")))]
            let vp = {
                #[cfg(target_os = "linux")]
                {
                    let mut b = base.clone().with_app_id("em6809");
                    if std::env::var("EM6809_CSD")
                        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                        .unwrap_or(false)
                    {
                        b = b.with_decorations(false);
                    }
                    b
                }
                #[cfg(not(target_os = "linux"))]
                {
                    base.with_app_id("em6809")
                }
            };
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
                step_over_requests: step_over_requests_clone,
                step_out_requests: step_out_requests_clone,
                lst_mmap: lst_clone,
                lst_text: lst_text_clone,
                lst_rev: lst_rev_clone,
                hex_rev: hex_rev_clone,
                lst_cache: ListingCache::force_refresh(),
                hex_cache: ListingCache::force_refresh(),
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
                open_dialog_active: false,
                open_pending_path: None,
                open_bin_base_text: String::new(),
                #[cfg(target_os = "linux")]
                csd: std::env::var("EM6809_CSD")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false),
                #[cfg(target_os = "linux")]
                csd_maximized: false,
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
                console_input: console_input_thread,
                console_state: console_state_thread,
                mem_jump_text: format!("0x{initial_start:04X}"),
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
                block_present: false,
                block_base: None,
                block_backing_path: None,
                block_backing_online: false,
                block_last_cmd: 0,
                block_last_data: 0,
                block_status: 0,
                gpio_on_color: egui::Color32::LIGHT_YELLOW,
                gpio_on_color_baseline: egui::Color32::LIGHT_YELLOW,
                gpio_on_color_text: String::new(),
                gpio_sync: Arc::new(Mutex::new(None)),
                mmu_sync: Arc::new(Mutex::new(None)),
                mmu_present: false,
                mmu_sys_mode: false,
                mmu_task: 0,
                mmu_map_prev_sys: [0u16; 16],
                mmu_map_cur_sys: [0u16; 16],
                mmu_map_prev_user: [0u16; 16],
                mmu_map_cur_user: [0u16; 16],
                mmu_view_sys: false,
                timer_present: false,
                timer_pending: false,
                timer_running: false,
                settings_open: false,
                settings_tab: 0,
                settings: SettingsState::default(),
                settings_baseline: SettingsState::default(),
                settings_instance: 0,
                settings_busy: false,
                settings_busy_since: None,
                settings_origins: HashMap::new(),
                console_enabled: false,
                console_base: None,
                console_gui_open: true,
                dev_tab: DeviceTab::Console,
                boundaries: Arc::clone(&self.boundaries),
                bp_states: Arc::clone(&self.bp_states),
                bp_list: Arc::clone(&self.bp_list),
                bp_list_rev: Arc::clone(&self.bp_list_rev),
                last_bp_list_rev: 0,
                call_stack: Arc::clone(&self.call_stack),
                call_stack_rev: Arc::clone(&self.call_stack_rev),
                last_call_stack_rev: 0,
                bp_window_open: false,
                call_stack_window_open: false,
                pending_listing_center: None,
            };
            let mut init = init;
            #[cfg(windows)]
            {
                self.started.store(true, Ordering::SeqCst);
                self.alive.store(true, Ordering::SeqCst);
                let _ = eframe::run_native(
                    title,
                    native_options,
                    Box::new(move |_| {
                        if let Some(init) = init.take() {
                            init();
                        }
                        Ok(Box::new(app))
                    }),
                );
                self.alive.store(false, Ordering::SeqCst);
                self.started.store(false, Ordering::SeqCst);
            }
            #[cfg(not(windows))]
            {
                use std::panic::{catch_unwind, AssertUnwindSafe};
                #[cfg(not(target_os = "macos"))]
                let prev = std::panic::take_hook();
                #[cfg(not(target_os = "macos"))]
                std::panic::set_hook(Box::new(|_| {}));
                self.started.store(true, Ordering::SeqCst);
                self.alive.store(true, Ordering::SeqCst);
                let res: Result<eframe::Result<(), eframe::Error>, _> =
                    catch_unwind(AssertUnwindSafe(|| {
                        eframe::run_native(
                            title,
                            native_options,
                            Box::new(move |_| {
                                if let Some(init) = init.take() {
                                    init();
                                }
                                Ok(Box::new(app))
                            }),
                        )
                    }));
                #[cfg(not(target_os = "macos"))]
                std::panic::set_hook(prev);
                self.alive.store(false, Ordering::SeqCst);
                self.started.store(false, Ordering::SeqCst);
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        eprintln!("[regview] GUI error: {e} — running headless");
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
            !self.alive.load(Ordering::SeqCst)
        }

        /// Block the caller until the GUI thread has had a chance to flip
        /// `alive` to true, or until `timeout_ms` elapses.  Returns true if
        /// the viewer became alive in time.  Needed so the run loop on the
        /// main thread does not race the newly-spawned GUI thread and see
        /// `is_closed() == true` before the window has a chance to open.
        pub fn wait_until_ready(&self, timeout_ms: u64) -> bool {
            use std::time::{Duration, Instant};
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            while !self.alive.load(Ordering::SeqCst) {
                if Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            true
        }

        pub fn quit_requested(&self) -> bool {
            self.quit_flag.load(Ordering::SeqCst)
        }

        pub fn update_hex_listing(&self, s: String) {
            if let Some(ref t) = self.lst_text {
                if let Ok(mut g) = t.lock() {
                    *g = s;
                }
            }
            self.hex_rev.fetch_add(1, Ordering::SeqCst);
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
            self.hex_rev.fetch_add(1, Ordering::SeqCst);
        }

        pub fn get_span_policy(&self) -> (usize, usize) {
            (
                self.span_policy.load(Ordering::SeqCst),
                self.window_len.load(Ordering::SeqCst),
            )
        }
        pub fn set_span_policy_pc_window(&self) {
            self.span_policy.store(1, Ordering::SeqCst);
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
        pub fn set_memory_window(&self, start: u16, len: Option<usize>, follow: Option<bool>) {
            self.mem_window_start
                .store(start as usize, Ordering::SeqCst);
            if let Some(l) = len {
                self.mem_window_len.store(l, Ordering::SeqCst);
            }
            if let Some(f) = follow {
                self.mem_follow_pc.store(f, Ordering::SeqCst);
            }
        }
        pub fn set_memory_window_start(&self, start: usize) {
            self.mem_window_start.store(start, Ordering::SeqCst);
        }
        pub fn update_memory_window(&self, start: u16, bytes: Vec<u8>) {
            if let Ok(mut g) = self.mem_buf.lock() {
                *g = (start, bytes);
            }
        }
        pub fn take_console_input(&self) -> Vec<u8> {
            if let Ok(mut g) = self.console_input.lock() {
                let out = g.clone();
                g.clear();
                out
            } else {
                Vec::new()
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

        /// Returns true if the CPU loop should pause before executing
        /// the next instruction. The pause-busy-wait in main.rs's run
        /// loop spins on this. Returns false either because the user
        /// is no longer paused, OR because at least one step / step-
        /// over / step-out request is queued — in the latter case the
        /// caller drains the request via [`Self::next_step_action`].
        ///
        /// Unlike the older one-counter version this method does
        /// **not** consume any request as a side effect — that would
        /// drop the kind information and force the run loop to always
        /// run plain `cpu.step`.
        pub fn should_pause(&self) -> bool {
            self.is_paused() && !self.has_pending_step_request()
        }

        fn has_pending_step_request(&self) -> bool {
            self.step_requests.load(Ordering::SeqCst) > 0
                || self.step_over_requests.load(Ordering::SeqCst) > 0
                || self.step_out_requests.load(Ordering::SeqCst) > 0
        }

        /// Drain at most one queued step request and return which CPU
        /// step variant the run loop should invoke for the upcoming
        /// instruction. Priority is **Out > Over > Step** so a queued
        /// plain Step from earlier doesn't sneak ahead of an Over /
        /// Out the user pressed afterwards.
        ///
        /// Returns [`StepAction::Step`] when no request is queued —
        /// that's the free-running case, where the run loop just
        /// keeps calling `cpu.step` once per pass.
        pub fn next_step_action(&self) -> StepAction {
            if self.step_out_requests.load(Ordering::SeqCst) > 0 {
                self.step_out_requests.fetch_sub(1, Ordering::SeqCst);
                return StepAction::Out;
            }
            if self.step_over_requests.load(Ordering::SeqCst) > 0 {
                self.step_over_requests.fetch_sub(1, Ordering::SeqCst);
                return StepAction::Over;
            }
            if self.step_requests.load(Ordering::SeqCst) > 0 {
                self.step_requests.fetch_sub(1, Ordering::SeqCst);
            }
            StepAction::Step
        }
    }

    /// Which CPU step variant the run loop should invoke next pass.
    /// Returned by [`RegView::next_step_action`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum StepAction {
        /// Plain single-instruction step. Used both for free-running
        /// (no pending request) and when the user presses Step.
        Step,
        /// Step over a CALL — descends, runs until PC reaches the
        /// byte right after the call (or a BP hits / limit exhausted).
        Over,
        /// Step out — runs until the current callee's return target
        /// (or a BP hits / limit exhausted).
        Out,
    }

    pub use RegView as Viewer;

    impl RegView {
        /// Record `pc` as a confirmed instruction-start. Called by the
        /// run loop right before each `cpu.step` so the live-disasm
        /// pane has authoritative anchor points to align on.
        pub fn record_instruction_pc(&self, pc: u16) {
            if let Ok(mut b) = self.boundaries.lock() {
                b.insert(pc);
            }
        }

        /// Linearly sweep `[start, end)` and record every disassembled
        /// instruction start. Call after a successful program load to
        /// pre-seed boundaries for the just-loaded code region.
        pub fn seed_boundaries_sweep<B: em6809::bus::Bus + ?Sized>(
            &self,
            bus: &mut B,
            start: u16,
            end: u16,
        ) {
            if let Ok(mut b) = self.boundaries.lock() {
                em6809::debug::linear_sweep(bus, start, end, &mut b);
            }
        }

        /// Forget every recorded boundary. Call when the program is
        /// reloaded or memory is wholesale replaced.
        pub fn clear_boundaries(&self) {
            if let Ok(mut b) = self.boundaries.lock() {
                b.clear();
            }
        }

        /// Render a boundary-anchored disassembly window around `pc`
        /// and push it as the new hex listing for the GUI to display.
        ///
        /// Why this lives on the emulator side: the GUI thread does
        /// not own a `Bus`, but disassembly needs to read instruction
        /// bytes through it. Doing the work here keeps the bus access
        /// strictly on the worker thread, while `lst_text` carries
        /// the rendered result over to the GUI through the same
        /// channel that file-loaded `.lst` text uses.
        ///
        /// This is the live-disasm answer to the anchor problem: the
        /// produced text is built with `disasm_window`, so the line
        /// containing `pc` is guaranteed to start on a confirmed
        /// instruction boundary, which means the existing
        /// PC-highlight and breakpoint markers in `list_ui.rs` (which
        /// match by address token) line up correctly without any
        /// changes to the GUI side.
        pub fn render_pc_window<B: em6809::bus::Bus + ?Sized>(
            &self,
            bus: &mut B,
            pc: u16,
            n_before: usize,
            n_after: usize,
        ) {
            // Hold the boundaries lock only while collecting addresses;
            // the per-line bytes-column re-disassembly below issues its
            // own bus reads and must not be done under the lock.
            let lines = {
                let guard = match self.boundaries.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                em6809::disasm::disasm_window(bus, pc, &guard, n_before, n_after)
            };
            let mut out = String::new();
            use core::fmt::Write as _;
            for (addr, _) in lines {
                // Re-disassemble with the hex variant so the text
                // matches the byte-column format used by file-loaded
                // `.lst` listings — list_ui.rs already understands it.
                let (_, hex_text) = em6809::disasm::disasm_one_hex(bus, addr);
                let _ = writeln!(&mut out, "{addr:04X}: {hex_text}");
            }
            self.update_hex_listing(out);
        }

        pub fn request_load_lst(&self, path: std::path::PathBuf) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiLoadLst(path));
            }
        }

        /// Ask the worker to re-render the live disassembly listing.
        /// Used when execution pauses on a new PC so the boundary-anchored
        /// window picks up the new anchor before the next paint.
        pub fn request_refresh_hex_listing(&self) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::RefreshHexListing);
            }
        }

        /// Worker-side: replace the GUI's BP-state snapshot with the
        /// current view of `cpu.breakpoints`. Cheap (small Vec, no
        /// allocation when the state is identical and the caller
        /// reuses an existing Vec, but we just replace for simplicity).
        pub fn update_breakpoint_states(&self, states: Vec<(u16, BpUiState)>) {
            if let Ok(mut g) = self.bp_states.lock() {
                *g = states;
            }
        }

        /// Worker-side: replace the BP-panel snapshot. Caller passes
        /// in the full list each time; the panel keeps no state of
        /// its own, so we don't need diff machinery here.  The rev
        /// counter is bumped so the GUI thread can detect that the
        /// deferred Breakpoints viewport (Linux/macOS) needs a
        /// repaint — pumping `request_repaint_of` every frame
        /// regardless of changes was found to interfere with macOS
        /// AppKit focus dispatch (BP / Call Stack windows would
        /// monopolise key input).
        pub fn update_bp_list(&self, list: Vec<BpInfo>) {
            if let Ok(mut g) = self.bp_list.lock() {
                *g = list;
            }
            self.bp_list_rev.fetch_add(1, Ordering::SeqCst);
        }

        /// Worker-side: replace the call-stack-panel snapshot. Frames
        /// are passed in oldest-first order (push order); the panel
        /// renders them newest-first so the current frame is on top.
        /// See `update_bp_list` for the rev-counter rationale.
        pub fn update_call_stack(&self, frames: Vec<CallFrameInfo>) {
            if let Ok(mut g) = self.call_stack.lock() {
                *g = frames;
            }
            self.call_stack_rev.fetch_add(1, Ordering::SeqCst);
        }

        pub fn auto_hex_on_edit(&self) -> bool {
            self.auto_hex_on_edit.load(Ordering::SeqCst)
        }
        /// Worker-side: pop only breakpoint-class commands so the
        /// running CPU loop on Linux/macOS can apply BP set/toggle/
        /// remove/condition edits without first requiring the user to
        /// pause. Mirrors the Windows behavior (where the main CPU
        /// loop drains *all* commands every iteration via
        /// [`take_command_worker`]). Order-preserving: returns the
        /// oldest matching command and leaves all others queued.
        pub fn take_breakpoint_command_eager(&self) -> Option<Command> {
            if let Ok(mut q) = self.cmds.lock() {
                if let Some(idx) = q.iter().position(|c| {
                    matches!(
                        c,
                        Command::CycleBreakpointAtAddr(..)
                            | Command::SetBreakpointEnabled(..)
                            | Command::RemoveBreakpoint(..)
                            | Command::SetBreakpointCondition(..)
                    )
                }) {
                    return Some(q.remove(idx));
                }
            }
            None
        }
        /// Worker-side: pop only non-UI commands that must be handled by the worker.
        /// Leave UiSet* and UI-only commands (e.g., prompts, LST load) for the GUI thread.
        pub fn take_command_worker(&self) -> Option<Command> {
            if let Ok(mut q) = self.cmds.lock() {
                // Process commands in FIFO order, skipping UI-only variants
                if let Some(idx) = q.iter().position(|c| {
                    !matches!(
                        c,
                        Command::UiSetGpio(..)
                            | Command::UiSetMmu(..)
                            | Command::UiSetTimer(..)
                            | Command::UiSetBlockStatus { .. }
                            | Command::UiSetGpioOnColor(..)
                            | Command::UiOpenSettingsExternal
                            | Command::UiSetSettingsBaseline(..)
                            | Command::UiAckSettings { .. }
                            | Command::UiPromptBinBase(..)
                            | Command::UiLoadLst(..)
                            | Command::UiOpenImageDialog
                            | Command::UiToggleBreakpointsWindow
                            | Command::UiToggleCallStackWindow
                    )
                }) {
                    return Some(q.remove(idx));
                }
            }
            None
        }
        pub fn sync_gpio(&self, present: bool, bits: u8, out: u32, dir: u32) {
            if let Ok(mut g) = self.gpio_sync.lock() {
                *g = Some((present, bits, out, dir));
            }
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetGpio(present, bits, out, dir));
            }
        }
        pub fn set_gpio_on_color(&self, rgb: u32) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetGpioOnColor(rgb));
            }
        }
        pub fn sync_mmu(&self, sys_mode: bool, task: u8, map_sys: [u16; 16], map_user: [u16; 16]) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetMmu(sys_mode, task, map_sys, map_user));
            }
        }
        pub fn sync_timer(&self, present: bool, pending: bool, running: bool) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetTimer(present, pending, running));
            }
        }
        pub fn sync_block_status(
            &self,
            present: bool,
            base: Option<u16>,
            backing_path: Option<String>,
            backing_online: bool,
            last_cmd: u8,
            last_data: u8,
            status: u8,
        ) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetBlockStatus {
                    present,
                    base,
                    backing_path,
                    backing_online,
                    last_cmd,
                    last_data,
                    status,
                });
            }
        }
        pub fn set_settings_baseline(
            &self,
            s: SettingsState,
            origins: Vec<(String, u8)>,
            gpio_on_color: Option<u32>,
        ) {
            if let Ok(mut q) = self.cmds.lock() {
                q.push(Command::UiSetSettingsBaseline(s, origins, gpio_on_color));
            }
        }
        pub fn init_global_cmds(&self) {
            let _ = GLOBAL_CMDS.set(self.cmds.clone());
            let _ = GLOBAL_SETTINGS.set(Arc::new(Mutex::new((
                SettingsState::default(),
                HashMap::new(),
                None,
            ))));
            #[cfg(target_os = "macos")]
            if crate::settings_native::settings_debug_enabled() {
                eprintln!("[macos-debug] init_global_cmds set GLOBAL_CMDS");
            }
        }
    }
}

pub use gui::set_force_in_app_settings;
pub use gui::BpInfo;
pub use gui::BpUiState;
pub use gui::CallFrameInfo;
pub use gui::CallKindUi;
pub use gui::Command;
pub use gui::StepAction;
pub use gui::Viewer;
// Public helpers for external/native windows to interact with the same viewer command queue
pub fn push_global_command(cmd: Command) -> bool {
    if let Some(q) = gui::GLOBAL_CMDS.get() {
        if let Ok(mut v) = q.lock() {
            #[cfg(target_os = "macos")]
            if crate::settings_native::settings_debug_enabled() {
                eprintln!("[macos-debug] push_global_command {:?}", cmd);
            }
            v.push(cmd);
            return true;
        }
    }
    #[cfg(target_os = "macos")]
    if crate::settings_native::settings_debug_enabled() {
        eprintln!("[macos-debug] push_global_command failed (GLOBAL_CMDS not set)");
    }
    false
}

pub fn push_global_commands_front(cmds: Vec<Command>) -> bool {
    if let Some(q) = gui::GLOBAL_CMDS.get() {
        if let Ok(mut v) = q.lock() {
            for cmd in cmds.into_iter().rev() {
                v.insert(0, cmd);
            }
            return true;
        }
    }
    false
}

pub fn get_latest_settings_baseline() -> Option<(
    SettingsState,
    std::collections::HashMap<String, u8>,
    Option<u32>,
)> {
    if let Some(g) = gui::GLOBAL_SETTINGS.get() {
        if let Ok(lock) = g.lock() {
            return Some((lock.0.clone(), lock.1.clone(), lock.2));
        }
    }
    None
}

fn push_settings_command(cmds: &Arc<Mutex<Vec<Command>>>, native: bool) {
    let _ = native; // unified: always use in-app dialog
    if let Ok(mut q) = cmds.lock() {
        #[cfg(target_os = "macos")]
        eprintln!("[macos-debug] enqueue UiOpenSettings (native={native})");
        q.push(Command::UiOpenSettings);
    }
}
