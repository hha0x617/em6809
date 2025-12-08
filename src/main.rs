// Use library modules instead of duplicating

use std::env;
use std::fs;
use std::path::Path;

use em6809::bootscript::{emit_boot_template, parse_boot_script, BootSequencer};
use em6809::bus::Bus;
use em6809::bus::{self, Memory};
use em6809::config::{apply_mmu_config_from_str, apply_preset};
use em6809::cpu::Cpu;
use em6809::io::{BlockDev, ConsoleDev, GpioDev, IoBus};
use em6809::loader::{self, ImageFormat, LoadedImage};
use em6809::mmu::Mc6829;
use em6809::prefs;
use em6809::timer::TimerDev;
mod settings_native;
mod settings_worker;

// GUI register viewer (no-op unless built with `--features gui`)
mod i18n;
mod regview;

fn coalesce_ranges(mut ranges: Vec<(u16, u16)>) -> Vec<(u16, u16)> {
    ranges.sort_by_key(|(s, _)| *s);
    let mut out: Vec<(u16, u16)> = Vec::new();
    for (s, e) in ranges.into_iter() {
        if out.is_empty() {
            out.push((s, e));
        } else {
            let (_last_s, last_e) = out.last_mut().unwrap();
            if s <= *last_e {
                // overlap or touch
                if e > *last_e {
                    *last_e = e;
                }
            } else {
                out.push((s, e));
            }
        }
    }
    out
}

fn log_program_info(loaded: &LoadedImage, pc: u16) {
    if loaded.loaded_ranges.is_empty() {
        println!("Program: no image loaded (PC ${:04X})", pc);
        return;
    }
    let mut loaded_bytes_sum: usize = 0;
    for (s, e) in &loaded.loaded_ranges {
        loaded_bytes_sum = loaded_bytes_sum.saturating_add(e.wrapping_sub(*s) as usize);
    }
    let spans = coalesce_ranges(loaded.loaded_ranges.clone());
    let mut span = spans.iter().find(|(s, e)| *s <= pc && pc < *e).copied();
    if span.is_none() && !spans.is_empty() {
        let min_start = spans.iter().map(|(s, _)| *s).min().unwrap();
        let max_end = spans.iter().map(|(_, e)| *e).max().unwrap();
        span = Some((min_start, max_end));
    }
    if let Some((s, e)) = span {
        let span_bytes: usize = e.wrapping_sub(s) as usize;
        println!(
            "Program: start ${:04X}, loaded {} bytes, span ${:04X}..${:04X} ({} bytes)",
            pc,
            loaded_bytes_sum,
            s,
            e.wrapping_sub(1),
            span_bytes
        );
    }
}

fn print_usage() {
    println!("em6809 - MC6809 software emulator\n");
    println!("Usage:\n  em6809 [image] [options]\n");
    println!("Notes:");
    println!("  - When `--run` is specified, <image> must also be provided.");
    println!("  - Without `--run`, omitting <image> opens the GUI with no program loaded.");
    println!("Image Options:");
    println!("  --format bin|srec         Input format override");
    println!("  --base 0xADDR             Base address for raw binary");
    println!("  --pc 0xADDR               Override reset PC");
    println!("\nExecution / Debug:");
    println!("  --cycles                  Print cycle count at end");
    // Paused is default in GUI-only mode
    println!("  (GUI-only) Disasm/Memory/Trace are available in the window UI");
    // --run and --bp removed in GUI-only mode
    println!("\nConsole I/O:");
    println!("  --console 0xADDR          Attach console device at base (e.g., 0xFF00)");
    println!("  --console-in STRING       Prefeed input (supports \\r, \\n, \\t, \\\\)");
    println!("  --console-stdin           Read from stdin (Ctrl+C to stop)");
    println!("  --console-out PATH        Mirror output to file");
    println!("  --console-flush-every N   Flush every N chars (default 1, max 128)");
    println!("  --console-flush-newline   Also flush on newline characters");
    println!("  --console-echo            Local echo on RX read (default OFF)");
    println!("  --console-rx-wm N         RX IRQ watermark (if used)");
    println!("  --console-irq-hold N      IRQ hold cycles");
    println!("\nBlock I/O:");
    println!("  --block 0xADDR            Attach block device at base (e.g., 0xFF20)");
    println!("  --block-file PATH         Backing image file (read/write)");
    println!("  --block-irq               Enable block device IRQ on ready/complete/error");
    println!("  --block-firq              Use FIRQ for block device");
    println!("  --block-irq-hold N        IRQ hold cycles (default 0)");
    println!("\nGPIO:");
    println!("  --gpio 0xADDR             Attach virtual GPIO at base (e.g., 0xFF40)");
    println!("  --gpio-bits N             Number of GPIO bits/LEDs (default 8)");
    println!("  --gpio-on-color 0xRRGGBB  LED ON color (e.g., 0xFFD000)");
    println!("\nTimer:");
    println!("  --timer 0xADDR            Attach periodic timer at base (e.g., 0xFF10)");
    println!("  --timer-reload 0xNNNN     Timer period in instruction ticks");
    println!("  --timer-irq               Enable timer IRQ");
    println!("  --timer-firq              Use FIRQ for timer");
    println!("  --timer-start             Start timer immediately");
    println!("  --fallback-settings       Use in-app Settings dialog instead of native window");
    println!("\nMMU / Presets:");
    println!("  --mmu                     Use MC6829 MMU backend");
    println!("  --mmu-phys BYTES          Physical size (e.g., 1048576)");
    println!("  --mmu-reg-base 0xA        MMU regs base address");
    println!("  --mmu-task N              Task register");
    println!("  --mmu-idmap               Identity map current task");
    println!("  --mmu-map P=F             Map page P to frame F (hex or dec)");
    println!("  --mmu-map-file PATH       Apply map/config from file");
    println!("  --mmu-log-maps            Log MMU page->frame changes to stdout");
    println!("  --preset NAME             Apply preset (also enables --mmu)");
    println!("  --preset-list             List presets and exit");
    println!("  --mmu-mode sys|user       Set initial MMU mode");
    println!("  --protect-vectors         Write-protect vectors");
    println!("  --prot 0xNN               Protection control byte");
    println!("  --attr PG=0xNN            Attribute for page PG");
    println!("\nBoot Scripts:");
    println!("  --boot-script PATH        Apply boot script");
    println!("  --emit-boot-template N    Write N.boot template and exit");
    println!("\nGeneral:");
    println!("  -h, --help                Show this help and exit");
}

fn parse_hex(s: &str) -> Option<u16> {
    let t = s.trim_start_matches("0x");
    u16::from_str_radix(t, 16).ok()
}

// Removed unused numeric parsers (GUI-only mode)

fn parse_escapes(s: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    let mut it = s.bytes().peekable();
    while let Some(b) = it.next() {
        if b == b'\\' {
            match it.peek().copied() {
                Some(b'r') => {
                    out.push(0x0D);
                    it.next();
                }
                Some(b'n') => {
                    out.push(0x0A);
                    it.next();
                }
                Some(b't') => {
                    out.push(0x09);
                    it.next();
                }
                Some(b'\\') => {
                    out.push(b'\\');
                    it.next();
                }
                _ => {
                    out.push(b);
                }
            }
        } else {
            out.push(b);
        }
    }
    out
}

#[cfg(windows)]
mod console_mode {
    type DWORD = u32;
    type HANDLE = *mut core::ffi::c_void;
    const STD_INPUT_HANDLE: DWORD = -10i32 as u32;
    const ENABLE_ECHO_INPUT: DWORD = 0x0004;
    const ENABLE_LINE_INPUT: DWORD = 0x0002;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(nStdHandle: DWORD) -> HANDLE;
        fn GetConsoleMode(hConsoleHandle: HANDLE, lpMode: *mut DWORD) -> i32;
        fn SetConsoleMode(hConsoleHandle: HANDLE, dwMode: DWORD) -> i32;
    }

    pub fn enter_raw() -> Option<DWORD> {
        unsafe {
            let h = GetStdHandle(STD_INPUT_HANDLE);
            if h.is_null() {
                return None;
            }
            let mut mode: DWORD = 0;
            if GetConsoleMode(h, &mut mode as *mut DWORD) == 0 {
                return None;
            }
            // Disable echo and line-buffered input so reads return per key
            let new_mode = mode & !ENABLE_ECHO_INPUT & !ENABLE_LINE_INPUT;
            if SetConsoleMode(h, new_mode) == 0 {
                return None;
            }
            Some(mode)
        }
    }

    pub fn restore_stdin_mode(prev: DWORD) {
        unsafe {
            let h = GetStdHandle(STD_INPUT_HANDLE);
            if !h.is_null() {
                let _ = SetConsoleMode(h, prev);
            }
        }
    }
}

#[cfg(not(windows))]
mod console_mode {
    use libc::{
        c_int, tcgetattr, tcsetattr, termios, ECHO, ICANON, STDIN_FILENO, TCSANOW, VMIN, VTIME,
    };

    #[derive(Clone, Copy)]
    pub struct Prev(termios);

    pub fn enter_raw() -> Option<Prev> {
        unsafe {
            let fd: c_int = STDIN_FILENO;
            let mut tio: termios = core::mem::zeroed();
            if tcgetattr(fd, &mut tio as *mut termios) != 0 {
                return None;
            }
            let mut new_tio = tio;
            // Disable canonical mode and echo so reads return per byte without echoing
            new_tio.c_lflag &= !(ICANON | ECHO);
            // Return as soon as 1 byte is available, no timeout
            new_tio.c_cc[VMIN] = 1;
            new_tio.c_cc[VTIME] = 0;
            if tcsetattr(fd, TCSANOW, &new_tio as *const termios) != 0 {
                return None;
            }
            Some(Prev(tio))
        }
    }

    pub fn restore_stdin_mode(prev: Prev) {
        unsafe {
            let _ = tcsetattr(STDIN_FILENO, TCSANOW, &prev.0 as *const termios);
        }
    }
}

use std::sync::atomic::{AtomicBool, Ordering};
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
mod signals {
    use super::STOP_REQUESTED;
    type BOOL = i32;
    type DWORD = u32;
    #[allow(non_camel_case_types)]
    type PHANDLER_ROUTINE = Option<unsafe extern "system" fn(ctrl: DWORD) -> BOOL>;
    const CTRL_C_EVENT: DWORD = 0;
    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleCtrlHandler(h: PHANDLER_ROUTINE, add: i32) -> i32;
    }
    unsafe extern "system" fn handler(ctrl: DWORD) -> BOOL {
        if ctrl == CTRL_C_EVENT {
            STOP_REQUESTED.store(true, super::Ordering::SeqCst);
            return 1;
        }
        0
    }
    pub fn install() {
        unsafe {
            let _ = SetConsoleCtrlHandler(Some(handler), 1);
        }
    }
}

#[cfg(not(windows))]
mod signals {
    use super::STOP_REQUESTED;
    extern "C" fn h(_sig: i32) {
        STOP_REQUESTED.store(true, super::Ordering::SeqCst);
    }
    extern "C" {
        fn signal(sig: i32, handler: extern "C" fn(i32)) -> extern "C" fn(i32);
    }
    pub fn install() {
        unsafe {
            let _ = signal(2, h);
        }
    } // SIGINT=2
}

fn main() {
    // Initialize i18n (Fluent) from environment/OS locale
    i18n::init_from_env();
    let mut args = env::args().skip(1).peekable();
    let mut force_in_app_settings = match std::env::var("EM6809_FALLBACK_SETTINGS") {
        Ok(v) if v == "0" || v.eq_ignore_ascii_case("false") => false,
        Ok(_) => true,
        Err(_) => true, // 全プラットフォームでデフォルトはアプリ内設定
    };
    // Help early-exit
    if let Some(first) = args.peek() {
        if first == "--help" || first == "-h" {
            print_usage();
            return;
        }
    }
    let mut image_path: Option<String> = None;
    if let Some(first) = args.peek() {
        if !first.starts_with('-') {
            image_path = args.next();
        }
    }

    let mut format: Option<ImageFormat> = None;
    let mut base: Option<u16> = None;
    let mut force_pc: Option<u16> = None;
    // GUI-only: trace/step removed; pause is default
    let mut paused_start: bool = true;
    // GUI-only: legacy CUI dump flags removed
    let mut print_cycles = false;
    // GUI-only: legacy CUI disasm request removed
    // GUI-only: --run and --bp removed
    let mut use_mmu = false;
    // CLI presence trackers for origin badges
    let mut cli_protect_vectors = false;
    let mut mmu_phys: usize = 1 << 20; // 1MiB default
    let mut mmu_regs_base: u16 = 0xFFA0;
    let mut mmu_task: u8 = 0;
    let mut mmu_idmap = false;
    let mut mmu_log_maps = false;
    let mut console_base: Option<u16> = None;
    let mut cli_console_enable = false;
    let mut cli_console_base = false;
    let mut block_base: Option<u16> = None;
    let mut cli_block_enable = false;
    let mut cli_block_base = false;
    let mut console_in: Option<String> = None;
    let mut console_stdin = false;
    let mut cli_console_stdin = false;
    let mut block_file_path: Option<String> = None;
    let mut cli_block_file = false;
    let mut block_irq_enable: bool = false;
    let mut cli_block_irq = false;
    let mut block_firq: bool = false;
    let mut cli_block_firq = false;
    let mut block_irq_hold: Option<u32> = None;
    let mut cli_block_irq_hold = false;
    let mut gpio_base: Option<u16> = None;
    let mut cli_gpio_enable = false;
    let mut cli_gpio_base = false;
    let mut gpio_bits: u8 = 8;
    let mut cli_gpio_bits = false;
    let mut gpio_on_color: Option<u32> = None;
    let mut cli_gpio_on_color = false;
    let mut mmu_maps: Vec<(usize, u16)> = Vec::new();
    let mut mmu_map_file: Option<String> = None;
    let mut preset_name: Option<String> = None;
    let mut protect_vectors = false;
    // mmu origins
    let mut cli_mmu_enable = false;
    let cli_mmu_regs_base = false;
    let cli_mmu_phys = false;
    let cli_mmu_task = false;
    let mut cli_mmu_mode = false;
    let mut cli_mmu_prot = false;
    let mut cli_mmu_log_maps = false;
    let mut mmu_mode_sys: Option<bool> = None;
    let mut mmu_prot: Option<u8> = None;
    let mut mmu_attrs: Vec<(usize, u8)> = Vec::new();
    let mut console_rx_wm: Option<usize> = None;
    let mut cli_console_rx_wm = false;
    let mut console_irq_hold: Option<u32> = None;
    let mut cli_console_irq_hold = false;
    let mut boot_script_path: Option<String> = None;
    let mut console_out_path: Option<String> = None;
    let mut cli_console_out = false;
    let mut console_tee_stderr: bool = false;
    let mut cli_console_tee_stderr = false;
    let mut console_flush_every: Option<usize> = None;
    let mut cli_console_flush_every = false;
    let mut console_flush_newline: bool = false;
    let mut cli_console_flush_newline = false;
    let mut console_local_echo: bool = false; // default OFF
    let mut cli_console_echo = false;
    let mut emit_boot_template_name: Option<String> = None;
    let mut irq_log: bool = false;
    let mut gpio_log: bool = false;
    // Timer CLI
    let mut timer_base: Option<u16> = None;
    let mut cli_timer_enable = false;
    let mut cli_timer_base = false;
    let mut timer_reload: Option<u16> = None;
    let mut cli_timer_reload = false;
    let mut timer_rate_hz: Option<u32> = None;
    let mut timer_ips_est: u64 = 1_000_000; // default 1 MIPS equivalent (instructions/sec)
    let mut cli_timer_ips = false;
    let mut timer_irq: bool = false;
    let mut cli_timer_irq = false;
    let mut timer_firq: bool = false;
    let mut cli_timer_firq = false;
    let mut timer_start: bool = false;
    let mut cli_timer_start = false;
    let mut debug_console_log: bool = false;
    let mut debug_block_log: bool = false;
    let mut debug_gpio_log: bool = false;
    let mut debug_mmu_log: bool = false;
    let mut debug_settings_log: bool = false;
    let mut debug_show_settings_origins: bool = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => {
                if let Some(f) = args.next() {
                    format = match f.as_str() {
                        "bin" => Some(ImageFormat::Binary),
                        "srec" => Some(ImageFormat::Srec),
                        other => {
                            eprintln!("Unknown format: {other}");
                            return;
                        }
                    };
                }
            }
            "--base" => {
                if let Some(v) = args.next() {
                    base = parse_hex(&v);
                }
            }
            "--pc" => {
                if let Some(v) = args.next() {
                    force_pc = parse_hex(&v);
                }
            }
            "--cycles" => print_cycles = true,
            "--run" => {
                paused_start = false;
            }
            // --paused is default in GUI-only mode; ignore if present
            "--paused" => {}
            "--dump-regs" => { /* ignored in GUI-only mode */ }
            "--dump-mem" => {
                let _ = (args.next(), args.next()); /* ignored */
            }
            "--dump-mem-ascii" => {
                let _ = (args.next(), args.next()); /* ignored */
            }
            "--disasm" => { /* removed in GUI-only mode */ }
            // --bp removed in GUI-only mode
            "--mmu" => use_mmu = true,
            "--mmu-phys" => {
                if let Some(v) = args.next() {
                    if let Ok(n) = v.parse::<usize>() {
                        mmu_phys = n;
                    }
                }
            }
            "--mmu-reg-base" => {
                if let Some(v) = args.next() {
                    if let Some(a) = parse_hex(&v) {
                        mmu_regs_base = a;
                    }
                }
            }
            "--mmu-task" => {
                if let Some(v) = args.next() {
                    if let Ok(t) = v.parse::<u8>() {
                        mmu_task = t;
                    }
                }
            }
            "--mmu-idmap" => mmu_idmap = true,
            "--mmu-map" => {
                if let Some(v) = args.next() {
                    // format: PAGE=FRAME (hex like 0x3=0x42, or decimal)
                    if let Some(eq) = v.find('=') {
                        let (p, f) = v.split_at(eq);
                        let f = &f[1..];
                        let page = p.strip_prefix("0x").unwrap_or(p);
                        let frame = f.strip_prefix("0x").unwrap_or(f);
                        if let (Ok(pp), Ok(ff)) = (
                            usize::from_str_radix(page, if p.starts_with("0x") { 16 } else { 10 }),
                            u16::from_str_radix(frame, if f.starts_with("0x") { 16 } else { 10 }),
                        ) {
                            mmu_maps.push((pp, ff));
                        }
                    }
                }
            }
            "--console" => {
                if let Some(v) = args.next() {
                    console_base = parse_hex(&v);
                    cli_console_enable = console_base.is_some();
                    cli_console_base = cli_console_enable;
                }
            }
            "--block" => {
                if let Some(v) = args.next() {
                    block_base = parse_hex(&v);
                    cli_block_enable = block_base.is_some();
                    cli_block_base = cli_block_enable;
                }
            }
            "--console-in" => {
                if let Some(v) = args.next() {
                    console_in = Some(v);
                }
            }
            "--console-stdin" => {
                console_stdin = true;
                cli_console_stdin = true;
            }
            "--block-file" => {
                if let Some(v) = args.next() {
                    block_file_path = Some(v);
                    cli_block_file = true;
                }
            }
            "--block-irq" => {
                block_irq_enable = true;
                cli_block_irq = true;
            }
            "--block-firq" => {
                block_firq = true;
                cli_block_firq = true;
            }
            "--block-irq-hold" => {
                if let Some(v) = args.next() {
                    if let Ok(n) = v.parse::<u32>() {
                        block_irq_hold = Some(n);
                        cli_block_irq_hold = true;
                    }
                }
            }
            "--gpio" => {
                if let Some(v) = args.next() {
                    gpio_base = parse_hex(&v);
                    cli_gpio_enable = gpio_base.is_some();
                    cli_gpio_base = cli_gpio_enable;
                }
            }
            "--gpio-bits" => {
                if let Some(v) = args.next() {
                    if let Ok(n) = v.parse::<u8>() {
                        gpio_bits = n;
                        cli_gpio_bits = true;
                    }
                }
            }
            "--gpio-on-color" => {
                if let Some(v) = args.next() {
                    let s2 = v.trim();
                    let hex = s2
                        .strip_prefix("0x")
                        .or_else(|| s2.strip_prefix("#"))
                        .unwrap_or(s2);
                    if hex.len() == 6 {
                        if let Ok(n) = u32::from_str_radix(hex, 16) {
                            gpio_on_color = Some(n & 0xFFFFFF);
                            cli_gpio_on_color = true;
                        }
                    }
                }
            }
            "--gpio-log" => {
                gpio_log = true;
            }
            "--console-out" => {
                if let Some(v) = args.next() {
                    console_out_path = Some(v);
                    cli_console_out = true;
                }
            }
            "--console-tee-stderr" => {
                console_tee_stderr = true;
                cli_console_tee_stderr = true;
            }
            "--console-flush-every" => {
                if let Some(v) = args.next() {
                    if let Ok(n) = v.parse::<usize>() {
                        console_flush_every = Some(n.min(128));
                        cli_console_flush_every = true;
                    }
                }
            }
            "--console-flush-newline" => {
                console_flush_newline = true;
                cli_console_flush_newline = true;
            }
            "--console-echo" => {
                console_local_echo = true;
                cli_console_echo = true;
            }
            "--console-rx-wm" => {
                if let Some(v) = args.next() {
                    if let Ok(n) = v.parse::<usize>() {
                        console_rx_wm = Some(n);
                        cli_console_rx_wm = true;
                    }
                }
            }
            "--console-irq-hold" => {
                if let Some(v) = args.next() {
                    if let Ok(n) = v.parse::<u32>() {
                        console_irq_hold = Some(n);
                        cli_console_irq_hold = true;
                    }
                }
            }
            "--timer" => {
                if let Some(v) = args.next() {
                    timer_base = parse_hex(&v);
                    cli_timer_enable = timer_base.is_some();
                    cli_timer_base = cli_timer_enable;
                }
            }
            "--timer-reload" => {
                if let Some(v) = args.next() {
                    timer_reload = parse_hex(&v);
                    cli_timer_reload = timer_reload.is_some();
                }
            }
            "--timer-rate" => {
                if let Some(v) = args.next() {
                    if let Ok(hz) = v.parse::<u32>() {
                        timer_rate_hz = Some(hz);
                    }
                }
            }
            "--timer-ips" => {
                if let Some(v) = args.next() {
                    if let Ok(n) = v.parse::<u64>() {
                        timer_ips_est = n.max(1);
                        cli_timer_ips = true;
                    }
                }
            }
            "--timer-irq" => {
                timer_irq = true;
                cli_timer_irq = true;
            }
            "--timer-firq" => {
                timer_firq = true;
                cli_timer_firq = true;
            }
            "--timer-start" => {
                timer_start = true;
                cli_timer_start = true;
            }
            "--fallback-settings" => {
                force_in_app_settings = true;
            }
            "--irq-log" => {
                irq_log = true;
            }
            "--mmu-map-file" => {
                if let Some(v) = args.next() {
                    mmu_map_file = Some(v);
                }
            }
            "--mmu-log-maps" => {
                mmu_log_maps = true;
                cli_mmu_log_maps = true;
            }
            "--preset" | "--mmu-template" => {
                if let Some(v) = args.next() {
                    preset_name = Some(v);
                    use_mmu = true;
                    cli_mmu_enable = true;
                }
            }
            "--protect-vectors" => {
                protect_vectors = true;
                cli_protect_vectors = true;
            }
            "--mmu-mode" => {
                if let Some(v) = args.next() {
                    let s = v.to_lowercase();
                    if s == "sys" || s == "system" {
                        mmu_mode_sys = Some(true);
                        cli_mmu_mode = true;
                    } else if s == "user" {
                        mmu_mode_sys = Some(false);
                        cli_mmu_mode = true;
                    }
                }
            }
            "--prot" => {
                if let Some(v) = args.next() {
                    if let Some(p) = parse_hex(&v) {
                        mmu_prot = Some((p & 0xFF) as u8);
                        cli_mmu_prot = true;
                    }
                }
            }
            "--attr" => {
                if let Some(pv) = args.next() {
                    if let Some(eq) = pv.find('=') {
                        let (p, v) = pv.split_at(eq);
                        let v = &v[1..];
                        if let (Ok(pp), Ok(vv)) = (
                            usize::from_str_radix(
                                p.trim_start_matches("0x"),
                                if p.starts_with("0x") { 16 } else { 10 },
                            ),
                            u8::from_str_radix(
                                v.trim_start_matches("0x"),
                                if v.starts_with("0x") { 16 } else { 10 },
                            ),
                        ) {
                            mmu_attrs.push((pp, vv));
                        }
                    }
                }
            }
            "--preset-list" => {
                println!("Presets: {}", em6809::config::list_presets().join(", "));
                return;
            }
            "--boot-script" => {
                if let Some(v) = args.next() {
                    boot_script_path = Some(v);
                }
            }
            "--emit-boot-template" => {
                if let Some(v) = args.next() {
                    emit_boot_template_name = Some(v);
                }
            }
            _ => {
                eprintln!("Unknown arg: {arg}");
                print_usage();
                return;
            }
        }
    }

    // Debug toggles initial state from env/CLI
    if env::var("EM6809_SETTINGS_DEBUG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        debug_settings_log = true;
    }
    if env::var("EM6809_SHOW_SETTINGS_ORIGINS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        debug_show_settings_origins = true;
    }
    debug_gpio_log |= gpio_log;
    debug_mmu_log |= mmu_log_maps;

    regview::set_force_in_app_settings(force_in_app_settings);

    if !paused_start && image_path.is_none() {
        eprintln!("error: --run requires an image file.\n");
        print_usage();
        return;
    }

    // Load persisted preferences and apply precedence: CLI > persisted > defaults
    let persisted = prefs::load();
    // General
    if protect_vectors == false {
        if let Some(m) = &persisted.mmu {
            if let Some(v) = m.protect_vectors {
                protect_vectors = v;
            }
        }
    }
    if protect_vectors == false {
        if let Some(g) = &persisted.general {
            if let Some(v) = g.protect_vectors {
                protect_vectors = v;
            }
        }
    }
    // Console
    if console_base.is_none() {
        if let Some(c) = &persisted.console {
            let en = c.enable.unwrap_or(false);
            if en {
                console_base = c.base.or(Some(0xFF00));
            }
            if !console_stdin {
                console_stdin = c.stdin.unwrap_or(false);
            }
            if !console_local_echo {
                console_local_echo = c.echo.unwrap_or(false);
            }
            if console_out_path.is_none() {
                console_out_path = c.out_path.clone();
            }
            if console_flush_every.is_none() {
                console_flush_every = c.flush_every;
            }
            if !console_flush_newline {
                console_flush_newline = c.flush_newline.unwrap_or(false);
            }
            if console_rx_wm.is_none() {
                console_rx_wm = c.rx_watermark;
            }
            if console_irq_hold.is_none() {
                console_irq_hold = c.irq_hold;
            }
            if !console_tee_stderr {
                console_tee_stderr = c.tee_stderr.unwrap_or(false);
            }
        }
    }
    // Block
    if block_base.is_none() {
        if let Some(bk) = &persisted.block {
            let en = bk.enable.unwrap_or(false);
            if en {
                block_base = bk.base.or(Some(0xFF20));
            }
            if block_file_path.is_none() {
                block_file_path = bk.file_path.clone();
            }
            if !block_irq_enable {
                block_irq_enable = bk.irq_enable.unwrap_or(false);
            }
            if !block_firq {
                block_firq = bk.firq.unwrap_or(false);
            }
            if block_irq_hold.is_none() {
                block_irq_hold = bk.irq_hold;
            }
        }
    }
    // GPIO
    if gpio_base.is_none() {
        if let Some(gp) = &persisted.gpio {
            let en = gp.enable.unwrap_or(false);
            if en {
                gpio_base = gp.base.or(Some(0xFF40));
            }
            if let Some(bits) = gp.bits {
                gpio_bits = bits;
            }
            if gpio_on_color.is_none() {
                gpio_on_color = gp.on_color;
            }
        }
    }
    // Timer
    if timer_base.is_none() {
        if let Some(ti) = &persisted.timer {
            let en = ti.enable.unwrap_or(false);
            if en {
                timer_base = ti.base.or(Some(0xFF10));
            }
            if timer_reload.is_none() {
                timer_reload = ti.reload;
            }
            if !timer_irq {
                timer_irq = ti.irq_enable.unwrap_or(false);
            }
            if !timer_firq {
                timer_firq = ti.firq.unwrap_or(false);
            }
            if !timer_start {
                timer_start = ti.start.unwrap_or(false);
            }
            if timer_rate_hz.is_none() {
                timer_rate_hz = ti.rate_hz;
            }
            if let Some(ips) = ti.ips_est {
                timer_ips_est = ips.max(1);
            }
        }
    }
    // MMU
    if !use_mmu {
        if let Some(m) = &persisted.mmu {
            use_mmu = m.enable.unwrap_or(false);
        }
    }
    if let Some(m) = &persisted.mmu {
        if mmu_regs_base == 0xFFA0 {
            if let Some(v) = m.regs_base {
                mmu_regs_base = v;
            }
        }
        if mmu_phys == (1 << 20) {
            if let Some(v) = m.phys_bytes {
                mmu_phys = v;
            }
        }
        if mmu_mode_sys.is_none() {
            mmu_mode_sys = m.sys_mode;
        }
        if mmu_task == 0 {
            if let Some(v) = m.task {
                mmu_task = v & 7;
            }
        }
        if mmu_prot.is_none() {
            mmu_prot = m.prot;
        }
        if !mmu_log_maps {
            mmu_log_maps = m.log_maps.unwrap_or(false);
        }
        if mmu_maps.is_empty() {
            if let Some(frames) = &m.map_frames {
                for (i, f) in frames {
                    mmu_maps.push((*i, *f));
                }
            }
        }
        if mmu_attrs.is_empty() {
            if let Some(attrs) = &m.attr_bytes {
                for (i, a) in attrs {
                    mmu_attrs.push((*i, *a));
                }
            }
        }
        if preset_name.is_none() {
            preset_name = m.preset_name.clone();
        }
    }
    if let Some(d) = &persisted.debug {
        debug_console_log = d.console_log.unwrap_or(debug_console_log);
        debug_block_log = d.block_log.unwrap_or(debug_block_log);
        debug_gpio_log = d.gpio_log.unwrap_or(debug_gpio_log);
        debug_mmu_log = d.mmu_log.unwrap_or(debug_mmu_log);
        debug_settings_log = d.settings_log.unwrap_or(debug_settings_log);
        debug_show_settings_origins = d
            .show_settings_origins
            .unwrap_or(debug_show_settings_origins);
    }

    let (image_data, inferred_format): (Option<Vec<u8>>, Option<ImageFormat>) = if let Some(path) =
        &image_path
    {
        match fs::read(path) {
            Ok(bytes) => {
                let fmt = if let Some(fmt) = format {
                    fmt
                } else if path.ends_with(".s19") || path.ends_with(".srec") || path.ends_with(".s1")
                {
                    ImageFormat::Srec
                } else {
                    ImageFormat::Binary
                };
                (Some(bytes), Some(fmt))
            }
            Err(e) => {
                eprintln!("Failed to read {path}: {e}");
                return;
            }
        }
    } else {
        (None, None)
    };

    // Choose memory backend and build a Bus object
    let mut bus: Box<dyn bus::Bus> = if use_mmu {
        let mut mmu = Mc6829::new(mmu_phys, mmu_regs_base);
        if mmu_log_maps {
            mmu.set_log_maps(true);
        }
        mmu.set_task(mmu_task);
        if mmu_idmap {
            mmu.identity_map_current();
        }
        for (p, f) in &mmu_maps {
            mmu.set_map_entry(*p, *f);
        }
        if let Some(path) = &mmu_map_file {
            match std::fs::read_to_string(path) {
                Ok(s) => {
                    if let Err(e) = apply_mmu_config_from_str(&mut mmu, &s) {
                        eprintln!("MMU map file error: {e}");
                        return;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read {path}: {e}");
                    return;
                }
            }
        }
        if let Some(pname) = &preset_name {
            if let Err(e) = apply_preset(&mut mmu, pname) {
                eprintln!("Preset error: {e}");
                return;
            }
            // Consoleの既定
            if pname.to_lowercase().starts_with("os9l2") && console_base.is_none() {
                console_base = Some(0xFF00);
            }
        }
        if let Some(sys) = mmu_mode_sys {
            if sys {
                mmu.write8(mmu.regs_base + 0x11, 0x01);
            } else {
                mmu.write8(mmu.regs_base + 0x11, 0x00);
            }
        }
        if let Some(p) = mmu_prot {
            mmu.write8(mmu.regs_base + 0x12, p);
        }
        for (pg, val) in &mmu_attrs {
            mmu.write8(mmu.regs_base + 0x18 + (*pg as u16), *val);
        }
        if protect_vectors {
            mmu.write8(mmu.regs_base + 0x12, 0x03);
            mmu.write8(mmu.regs_base + 0x18 + 0x0F, 0x01);
        }
        let need_iobus = console_base.is_some()
            || block_base.is_some()
            || gpio_base.is_some()
            || timer_base.is_some();
        if need_iobus {
            let mut iobus = IoBus::new(mmu);
            if let Some(base) = console_base {
                let mut con = ConsoleDev::new(base);
                if let Some(wm) = console_rx_wm {
                    con.set_rx_watermark(wm);
                }
                if let Some(h) = console_irq_hold {
                    con.set_irq_hold_cycles(h);
                }
                if let Some(s) = &console_in {
                    let bytes = parse_escapes(s);
                    con.feed_bytes(&bytes);
                }
                if let Some(p) = &console_out_path {
                    con.set_out_file(p);
                }
                if console_tee_stderr {
                    con.set_tee_stderr(true);
                }
                if let Some(n) = console_flush_every {
                    con.set_flush_every(n);
                }
                if console_flush_newline {
                    con.set_flush_on_newline(true);
                }
                if console_local_echo {
                    con.set_local_echo(true);
                }
                iobus.add_device(con);
            }
            if let Some(base) = block_base {
                let mut blk = BlockDev::new(base);
                if let Some(p) = &block_file_path {
                    blk.set_backing_file(p);
                }
                if let Some(n) = block_irq_hold {
                    blk.set_irq_hold_cycles(n);
                }
                if block_irq_enable {
                    blk.set_irq_enable(true);
                }
                if block_firq {
                    blk.set_firq(true);
                }
                iobus.add_device(blk);
            }
            if let Some(base) = gpio_base {
                let gpio = GpioDev::new(base, gpio_bits);
                iobus.add_device(gpio);
            }
            if let Some(base) = timer_base {
                let mut t = TimerDev::new(base);
                if let Some(r) = timer_reload {
                    t.set_reload(r);
                } else if let Some(hz) = timer_rate_hz {
                    let ticks = (timer_ips_est / (hz as u64).max(1)).max(1);
                    let ticks16 = u16::try_from(ticks).unwrap_or(u16::MAX);
                    t.set_reload(ticks16.max(1));
                }
                if timer_irq {
                    t.set_irq_enable(true);
                }
                if timer_firq {
                    t.set_firq(true);
                }
                if timer_start {
                    t.start();
                }
                let _reload_val: u16 = timer_reload
                    .or_else(|| {
                        timer_rate_hz.map(|hz| {
                            let ticks = (timer_ips_est / (hz as u64).max(1)).max(1);
                            u16::try_from(ticks).unwrap_or(u16::MAX).max(1)
                        })
                    })
                    .unwrap_or(0);
                iobus.add_device(t);
            }
            Box::new(iobus)
        } else {
            Box::new(mmu)
        }
    } else {
        let mem = Memory::new();
        let need_iobus = console_base.is_some()
            || block_base.is_some()
            || gpio_base.is_some()
            || timer_base.is_some();
        if need_iobus {
            let mut iobus = IoBus::new(mem);
            if let Some(base) = console_base {
                let mut con = ConsoleDev::new(base);
                if let Some(wm) = console_rx_wm {
                    con.set_rx_watermark(wm);
                }
                if let Some(h) = console_irq_hold {
                    con.set_irq_hold_cycles(h);
                }
                if let Some(s) = &console_in {
                    let bytes = parse_escapes(s);
                    con.feed_bytes(&bytes);
                }
                if let Some(p) = &console_out_path {
                    con.set_out_file(p);
                }
                if console_tee_stderr {
                    con.set_tee_stderr(true);
                }
                if let Some(n) = console_flush_every {
                    con.set_flush_every(n);
                }
                if console_flush_newline {
                    con.set_flush_on_newline(true);
                }
                if console_local_echo {
                    con.set_local_echo(true);
                }
                iobus.add_device(con);
            }
            if let Some(base) = block_base {
                let mut blk = BlockDev::new(base);
                if let Some(p) = &block_file_path {
                    blk.set_backing_file(p);
                }
                if let Some(n) = block_irq_hold {
                    blk.set_irq_hold_cycles(n);
                }
                if block_irq_enable {
                    blk.set_irq_enable(true);
                }
                if block_firq {
                    blk.set_firq(true);
                }
                iobus.add_device(blk);
            }
            if let Some(base) = gpio_base {
                let gpio = GpioDev::new(base, gpio_bits);
                iobus.add_device(gpio);
            }
            if let Some(base) = timer_base {
                let mut t = TimerDev::new(base);
                if let Some(r) = timer_reload {
                    t.set_reload(r);
                } else if let Some(hz) = timer_rate_hz {
                    let ticks = (timer_ips_est / (hz as u64).max(1)).max(1);
                    let ticks16 = u16::try_from(ticks).unwrap_or(u16::MAX);
                    t.set_reload(ticks16.max(1));
                }
                if timer_irq {
                    t.set_irq_enable(true);
                }
                if timer_firq {
                    t.set_firq(true);
                }
                if timer_start {
                    t.start();
                }
                let _reload_val: u16 = timer_reload
                    .or_else(|| {
                        timer_rate_hz.map(|hz| {
                            let ticks = (timer_ips_est / (hz as u64).max(1)).max(1);
                            u16::try_from(ticks).unwrap_or(u16::MAX).max(1)
                        })
                    })
                    .unwrap_or(0);
                iobus.add_device(t);
            }
            Box::new(iobus)
        } else {
            Box::new(mem)
        }
    };

    if let Some(name) = &emit_boot_template_name {
        let t = emit_boot_template(name);
        let out = format!("{name}.boot");
        match std::fs::write(&out, t) {
            Ok(_) => {
                println!("Wrote {out}");
            }
            Err(e) => {
                eprintln!("Failed to write {out}: {e}");
            }
        }
        return;
    }

    let mut loaded: LoadedImage =
        if let (Some(bytes), Some(fmt)) = (image_data.as_ref(), inferred_format) {
            match fmt {
                ImageFormat::Binary => {
                    let base_addr = base.unwrap_or(0x0000);
                    loader::load_binary_bus(bus.as_mut(), base_addr, bytes)
                }
                ImageFormat::Srec => {
                    match loader::load_srec_bus(bus.as_mut(), &String::from_utf8_lossy(bytes)) {
                        Ok(img) => img,
                        Err(e) => {
                            eprintln!("SREC parse error: {e}");
                            return;
                        }
                    }
                }
            }
        } else {
            LoadedImage {
                loaded_ranges: Vec::new(),
                entry: None,
            }
        };

    let mut cpu = Cpu::new();
    if irq_log {
        em6809::cpu::set_irq_log(true);
    }
    if gpio_log {
        em6809::io::set_gpio_log(true);
    }
    cpu.reset(bus.as_mut());
    // Prefer reset vector. Only override with --pc or SREC entry if vector looks invalid.
    let reset_pc = cpu.r.pc;
    if let Some(pc) = force_pc {
        cpu.set_pc(pc);
    } else if let Some(entry) = loaded.entry {
        if reset_pc == 0x0000 || reset_pc == 0xFFFF {
            cpu.set_pc(entry);
        }
    } else if reset_pc == 0x0000 || reset_pc == 0xFFFF {
        // Fallback: if vector is invalid and no SREC entry, start at first loaded range
        if let Some(&(start, _)) = loaded.loaded_ranges.iter().min_by_key(|(s, _)| *s) {
            cpu.set_pc(start);
        }
    }

    // Compute and report program start and sizes (loaded bytes vs PC-containing span)
    if loaded.loaded_ranges.is_empty() {
        println!("Program: no image loaded (PC ${:04X})", cpu.r.pc);
    } else {
        let mut loaded_bytes_sum: usize = 0;
        for (s, e) in &loaded.loaded_ranges {
            loaded_bytes_sum = loaded_bytes_sum.saturating_add(e.wrapping_sub(*s) as usize);
        }
        let spans = coalesce_ranges(loaded.loaded_ranges.clone());
        let mut pc_span: Option<(u16, u16)> = None;
        for (s, e) in &spans {
            if *s <= cpu.r.pc && cpu.r.pc < *e {
                pc_span = Some((*s, *e));
                break;
            }
        }
        if let Some((s, e)) = pc_span {
            let span_bytes: usize = e.wrapping_sub(s) as usize; // e is exclusive
            println!(
                "Program: start ${:04X}, loaded {} bytes, span ${:04X}..${:04X} ({} bytes)",
                cpu.r.pc,
                loaded_bytes_sum,
                s,
                e.wrapping_sub(1),
                span_bytes
            );
        } else {
            // Fallback: print overall min..max if PC not within any span
            let (mut min_start, mut max_end) = (u16::MAX, 0u16);
            for (s, e) in &spans {
                if *s < min_start {
                    min_start = *s;
                }
                if *e > max_end {
                    max_end = *e;
                }
            }
            let span_bytes: usize = max_end.wrapping_sub(min_start) as usize;
            println!(
                "Program: start ${:04X}, loaded {} bytes, span ${:04X}..${:04X} ({} bytes)",
                cpu.r.pc,
                loaded_bytes_sum,
                min_start,
                max_end.wrapping_sub(1),
                span_bytes
            );
        }
    }

    // Always prepare the register viewer (GUI-only app).
    // On Windows we spawn it on a background thread.
    // On Unix/macOS we prepare it and run on the main thread later.
    let reg_viewer = {
        // Try to load a companion .lst file for display in the viewer
        let lst_path_opt = image_path.as_ref().and_then(|p| {
            let lst_path = Path::new(p).with_extension("lst");
            if lst_path.exists() {
                Some(lst_path)
            } else {
                None
            }
        });
        // Always generate a hex-style disassembly of the span for GUI toggle and fallback
        let spans = coalesce_ranges(loaded.loaded_ranges.clone());
        let span_for_pc = spans
            .iter()
            .find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e)
            .copied();
        let (start, end) = if let Some((s, e)) = span_for_pc {
            (s, e)
        } else if let Some((s, e)) = spans.first().copied() {
            (s, e)
        } else {
            (0x0000, 0x0100)
        };
        let lst_fallback_text: Option<String> = if start != end {
            let mut cur = start;
            let mut out = String::new();
            let mut processed: usize = 0;
            let total_len: usize = end.wrapping_sub(start) as usize;
            while processed < total_len {
                let (consumed, text) = em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                use core::fmt::Write as _;
                let _ = writeln!(&mut out, "{cur:04X}: {text}");
                let step = core::cmp::max(consumed as usize, 1);
                processed += step;
                cur = cur.wrapping_add(step as u16);
            }
            Some(out)
        } else {
            None
        };
        #[cfg(not(windows))]
        {
            let v = Some(regview::Viewer::create(
                "em6809 - MC6809 Software Emulator",
                lst_path_opt,
                lst_fallback_text,
                start,
            ));
            if let Some(view) = &v {
                if loaded.loaded_ranges.is_empty() {
                    view.set_span_policy_pc_window();
                }
            }
            if let (Some(view), Some(rgb)) = (&v, gpio_on_color) {
                view.set_gpio_on_color(rgb);
            }
            if let Some(view) = &v {
                view.init_global_cmds();
            }
            v
        }
        #[cfg(windows)]
        {
            let v = Some(regview::Viewer::spawn(
                "em6809 - MC6809 Software Emulator",
                lst_path_opt,
                lst_fallback_text,
                start,
            ));
            if let Some(view) = &v {
                if loaded.loaded_ranges.is_empty() {
                    view.set_span_policy_pc_window();
                }
            }
            if let (Some(view), Some(rgb)) = (&v, gpio_on_color) {
                view.set_gpio_on_color(rgb);
            }
            if let Some(view) = &v {
                view.init_global_cmds();
            }
            v
        }
    };

    // Send effective configuration to Settings baseline for the GUI
    if let Some(viewer) = &reg_viewer {
        let mut s = regview::SettingsState::default();
        s.protect_vectors = protect_vectors;
        let mut origins: Vec<(String, u8)> = Vec::new();
        let mut mark = |key: &str, cli: bool, saved: bool| {
            origins.push((
                key.to_string(),
                if cli {
                    2
                } else if saved {
                    1
                } else {
                    0
                },
            ));
        };
        s.use_console = console_base.is_some();
        s.console_base_hex = format!("0x{:04X}", console_base.unwrap_or(0xFF00));
        s.console_stdin = console_stdin;
        s.console_echo = console_local_echo;
        s.console_out_path = console_out_path.clone().unwrap_or_default();
        s.console_flush_every = console_flush_every
            .map(|v| v.to_string())
            .unwrap_or_default();
        s.console_flush_newline = console_flush_newline;
        s.console_rx_wm = console_rx_wm.map(|v| v.to_string()).unwrap_or_default();
        s.console_irq_hold = console_irq_hold.map(|v| v.to_string()).unwrap_or_default();
        s.console_tee_stderr = console_tee_stderr;
        // Origin markers: general + console
        let saved_protect = persisted
            .mmu
            .as_ref()
            .and_then(|m| m.protect_vectors)
            .or_else(|| persisted.general.as_ref().and_then(|g| g.protect_vectors));
        mark(
            "mmu.protect_vectors",
            cli_protect_vectors,
            saved_protect.is_some(),
        );
        let pc = persisted.console.as_ref();
        mark(
            "console.enable",
            cli_console_enable,
            pc.and_then(|c| c.enable).is_some(),
        );
        mark(
            "console.base",
            cli_console_base,
            pc.and_then(|c| c.base).is_some(),
        );
        mark(
            "console.stdin",
            cli_console_stdin,
            pc.and_then(|c| c.stdin).is_some(),
        );
        mark(
            "console.echo",
            cli_console_echo,
            pc.and_then(|c| c.echo).is_some(),
        );
        mark(
            "console.out_path",
            cli_console_out,
            pc.and_then(|c| c.out_path.as_ref()).is_some(),
        );
        mark(
            "console.flush_every",
            cli_console_flush_every,
            pc.and_then(|c| c.flush_every).is_some(),
        );
        mark(
            "console.flush_newline",
            cli_console_flush_newline,
            pc.and_then(|c| c.flush_newline).is_some(),
        );
        mark(
            "console.rx_watermark",
            cli_console_rx_wm,
            pc.and_then(|c| c.rx_watermark).is_some(),
        );
        mark(
            "console.irq_hold",
            cli_console_irq_hold,
            pc.and_then(|c| c.irq_hold).is_some(),
        );
        mark(
            "console.tee_stderr",
            cli_console_tee_stderr,
            pc.and_then(|c| c.tee_stderr).is_some(),
        );

        s.use_block = block_base.is_some();
        s.block_base_hex = format!("0x{:04X}", block_base.unwrap_or(0xFF20));
        s.block_file_path = block_file_path.clone().unwrap_or_default();
        s.block_irq_enable = block_irq_enable;
        s.block_firq = block_firq;
        s.block_irq_hold = block_irq_hold.map(|v| v.to_string()).unwrap_or_default();
        // Block origins
        let pb = persisted.block.as_ref();
        mark(
            "block.enable",
            cli_block_enable,
            pb.and_then(|b| b.enable).is_some(),
        );
        mark(
            "block.base",
            cli_block_base,
            pb.and_then(|b| b.base).is_some(),
        );
        mark(
            "block.file_path",
            cli_block_file,
            pb.and_then(|b| b.file_path.as_ref()).is_some(),
        );
        mark(
            "block.irq_enable",
            cli_block_irq,
            pb.and_then(|b| b.irq_enable).is_some(),
        );
        mark(
            "block.firq",
            cli_block_firq,
            pb.and_then(|b| b.firq).is_some(),
        );
        mark(
            "block.irq_hold",
            cli_block_irq_hold,
            pb.and_then(|b| b.irq_hold).is_some(),
        );
        s.use_gpio = gpio_base.is_some();
        s.gpio_base_hex = format!("0x{:04X}", gpio_base.unwrap_or(0xFF40));
        s.gpio_bits = format!("{}", gpio_bits);
        // GPIO origins
        let pg = persisted.gpio.as_ref();
        mark(
            "gpio.enable",
            cli_gpio_enable,
            pg.and_then(|g| g.enable).is_some(),
        );
        mark(
            "gpio.base",
            cli_gpio_base,
            pg.and_then(|g| g.base).is_some(),
        );
        mark(
            "gpio.bits",
            cli_gpio_bits,
            pg.and_then(|g| g.bits.map(|_| ())).is_some(),
        );
        mark(
            "gpio.on_color",
            cli_gpio_on_color,
            pg.and_then(|g| g.on_color).is_some(),
        );
        s.use_timer = timer_base.is_some();
        s.timer_base_hex = format!("0x{:04X}", timer_base.unwrap_or(0xFF10));
        s.timer_reload_hex = timer_reload
            .map(|v| format!("0x{:04X}", v))
            .unwrap_or_default();
        s.timer_rate_hz = timer_rate_hz.map(|v| v.to_string()).unwrap_or_default();
        s.timer_ips = timer_ips_est.to_string();
        s.timer_irq_enable = timer_irq;
        s.timer_firq = timer_firq;
        s.timer_start = timer_start;
        // Timer origins
        let pt = persisted.timer.as_ref();
        mark(
            "timer.enable",
            cli_timer_enable,
            pt.and_then(|t| t.enable).is_some(),
        );
        mark(
            "timer.base",
            cli_timer_base,
            pt.and_then(|t| t.base).is_some(),
        );
        mark(
            "timer.reload",
            cli_timer_reload,
            pt.and_then(|t| t.reload).is_some(),
        );
        mark(
            "timer.rate",
            timer_rate_hz.is_some(),
            pt.and_then(|t| t.rate_hz).is_some(),
        );
        mark(
            "timer.ips",
            cli_timer_ips,
            pt.and_then(|t| t.ips_est).is_some(),
        );
        mark(
            "timer.irq_enable",
            cli_timer_irq,
            pt.and_then(|t| t.irq_enable).is_some(),
        );
        mark(
            "timer.firq",
            cli_timer_firq,
            pt.and_then(|t| t.firq).is_some(),
        );
        mark(
            "timer.start",
            cli_timer_start,
            pt.and_then(|t| t.start).is_some(),
        );
        s.use_mmu = use_mmu;
        s.mmu_regs_base_hex = format!("0x{:04X}", mmu_regs_base);
        s.mmu_phys_bytes = format!("{}", mmu_phys);
        s.mmu_mode_sys = mmu_mode_sys.unwrap_or(false);
        s.mmu_task = format!("{}", mmu_task);
        s.mmu_prot_hex = mmu_prot.map(|v| format!("0x{:02X}", v)).unwrap_or_default();
        s.mmu_log_maps = mmu_log_maps;
        // MMU origins
        let pm = persisted.mmu.as_ref();
        mark(
            "mmu.enable",
            cli_mmu_enable || use_mmu,
            pm.and_then(|m| m.enable).is_some(),
        );
        mark(
            "mmu.regs_base",
            cli_mmu_regs_base,
            pm.and_then(|m| m.regs_base).is_some(),
        );
        mark(
            "mmu.phys_bytes",
            cli_mmu_phys,
            pm.and_then(|m| m.phys_bytes).is_some(),
        );
        mark(
            "mmu.sys_mode",
            cli_mmu_mode,
            pm.and_then(|m| m.sys_mode).is_some(),
        );
        mark(
            "mmu.task",
            cli_mmu_task,
            pm.and_then(|m| m.task.map(|_| ())).is_some(),
        );
        mark("mmu.prot", cli_mmu_prot, pm.and_then(|m| m.prot).is_some());
        mark(
            "mmu.log_maps",
            cli_mmu_log_maps,
            pm.and_then(|m| m.log_maps).is_some(),
        );
        for (i, f) in &mmu_maps {
            s.mmu_map_frames[*i] = format!("0x{:02X}", *f & 0x00FF);
        }
        for (i, a) in &mmu_attrs {
            s.mmu_attr_bytes[*i] = format!("0x{:02X}", *a);
        }
        s.mmu_preset_name = preset_name.clone().unwrap_or_default();
        // Debug flags
        s.debug_console_log = debug_console_log;
        s.debug_block_log = debug_block_log;
        s.debug_gpio_log = debug_gpio_log;
        s.debug_mmu_log = debug_mmu_log;
        s.debug_settings_log = debug_settings_log;
        s.debug_show_origins = debug_show_settings_origins;
        // debug origins
        if !origins.is_empty() {
            let mut items: Vec<String> = origins
                .iter()
                .map(|(k, v)| {
                    let src = match *v {
                        2 => "CLI",
                        1 => "Saved",
                        _ => "Default",
                    };
                    format!("{}={}", k, src)
                })
                .collect();
            items.sort();
        }
        let s_for_publish = s.clone();
        // Apply persisted/CLI settings immediately to runtime and publish to UI.
        let payload = settings_worker::SettingsPayload {
            settings: s_for_publish.clone(),
            gpio_on_color: gpio_on_color.unwrap_or(0),
            instance_id: settings_worker::next_dialog_instance(),
        };
        let _ = apply_settings_runtime(Some(viewer), bus.as_mut(), &payload);
        viewer.set_settings_baseline(payload.settings.clone(), origins, gpio_on_color);
    }

    // Initial GUI sync: push current MMU map and GPIO state so the panels show immediately
    if let Some(viewer) = &reg_viewer {
        // MMU snapshot from current bus
        if let Some(io_mmu) = bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
            let inner = io_mmu.inner_any_mut();
            if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                let (map, sys, task) = mmu.snapshot_current_map();
                viewer.sync_mmu(sys, task, map);
            }
        } else if let Some(mmu) = bus.as_mut().as_any_mut().downcast_mut::<Mc6829>() {
            let (map, sys, task) = mmu.snapshot_current_map();
            viewer.sync_mmu(sys, task, map);
        }
        // GPIO snapshot from current bus
        let mut pushed_gpio = false;
        if let Some(iomem) = bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>() {
            iomem.with_gpio_mut(|g| {
                let (o, d, b) = g.get_state();
                viewer.sync_gpio(true, b, o, d);
                pushed_gpio = true;
            });
        } else if let Some(iommu) = bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
            iommu.with_gpio_mut(|g| {
                let (o, d, b) = g.get_state();
                viewer.sync_gpio(true, b, o, d);
                pushed_gpio = true;
            });
        } else if let Some(wt) = bus.as_mut().as_any_mut().downcast_mut::<bus::WriteTrack>() {
            let inner_any = wt.inner_any_mut();
            if let Some(iomem) = inner_any.downcast_mut::<IoBus<Memory>>() {
                iomem.with_gpio_mut(|g| {
                    let (o, d, b) = g.get_state();
                    viewer.sync_gpio(true, b, o, d);
                    pushed_gpio = true;
                });
            } else if let Some(iommu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
                iommu.with_gpio_mut(|g| {
                    let (o, d, b) = g.get_state();
                    viewer.sync_gpio(true, b, o, d);
                    pushed_gpio = true;
                });
            }
        }
        let _ = pushed_gpio; // avoid unused in case no GPIO
                             // Timer snapshot from current bus
        let mut t_present = false;
        let mut t_pending = false;
        let mut t_running = false;
        if let Some(iomem) = bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>() {
            iomem.with_timer_mut(|t| {
                let (run, _irq, _firq, p) = t.get_state();
                t_running = run;
                t_present = true;
                t_pending = p;
            });
        } else if let Some(iommu) = bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
            iommu.with_timer_mut(|t| {
                let (run, _irq, _firq, p) = t.get_state();
                t_running = run;
                t_present = true;
                t_pending = p;
            });
        } else if let Some(wt) = bus.as_mut().as_any_mut().downcast_mut::<bus::WriteTrack>() {
            let inner_any = wt.inner_any_mut();
            if let Some(iomem) = inner_any.downcast_mut::<IoBus<Memory>>() {
                iomem.with_timer_mut(|t| {
                    let (run, _irq, _firq, p) = t.get_state();
                    t_running = run;
                    t_present = true;
                    t_pending = p;
                });
            } else if let Some(iommu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
                iommu.with_timer_mut(|t| {
                    let (run, _irq, _firq, p) = t.get_state();
                    t_running = run;
                    t_present = true;
                    t_pending = p;
                });
            }
        }
        if t_present {
            viewer.sync_timer(true, t_pending, t_running);
        }
        sync_block_from_bus(bus.as_mut(), viewer);
    }

    // Always GUI: legacy headless register dump removed.
    // GUI-only: CUI dumps suppressed; consider wiring these to GUI actions.

    // GUI-only: --disasm no longer prints/returns; the viewer provides listing/disasm.

    // Boot script
    let mut boot_seq: Option<BootSequencer> = if let Some(p) = &boot_script_path {
        match std::fs::read_to_string(p) {
            Ok(s) => match parse_boot_script(&s) {
                Ok(v) => Some(BootSequencer::new(v)),
                Err(e) => {
                    eprintln!("Boot script parse error: {e}");
                    None
                }
            },
            Err(e) => {
                eprintln!("Failed to read boot script {p}: {e}");
                None
            }
        }
    } else {
        None
    };

    // If --paused is specified, still enter the run loop so Pause/Step controls work,
    // but start in paused state and gate before executing any instruction.
    if paused_start {
        if let Some(viewer) = &reg_viewer {
            viewer.set_paused(true);
            let snap = regview::RegSnapshot {
                a: cpu.r.a,
                b: cpu.r.b,
                d: ((cpu.r.a as u16) << 8) | (cpu.r.b as u16),
                x: cpu.r.x,
                y: cpu.r.y,
                u: cpu.r.u,
                s: cpu.r.s,
                dp: cpu.r.dp,
                pc: cpu.r.pc,
                cc: cpu.r.cc,
            };
            viewer.push(snap);
        }
    }

    // On Unix/macOS, run GUI on the main thread and move CPU loop to a worker (GUI-only).
    #[cfg(not(windows))]
    {
        if let Some(viewer_main) = &reg_viewer {
            // Ensure SIGINT (Ctrl+C) is handled process-wide before starting GUI/worker
            signals::install();
            use std::thread;
            let viewer_cpu = viewer_main.clone();
            // Move ownership of emulator state into CPU worker thread
            let mut cpu_worker = cpu;
            let mut bus_worker = bus;
            let mut loaded_ranges_worker = loaded.loaded_ranges.clone();
            // GUI-only: no step/trace controls
            // no hex/trace toggles or CLI breakpoints in GUI-only mode
            let paused_start_flag = paused_start;
            let console_stdin_flag = console_stdin;
            let _console_rx_wm_opt = console_rx_wm;
            let _console_irq_hold_opt = console_irq_hold;
            let _console_in_str = console_in.clone();
            let _console_out_path_opt = console_out_path.clone();
            let _console_tee_stderr_flag = console_tee_stderr;
            let _console_flush_every_opt = console_flush_every;
            let _console_local_echo_flag = console_local_echo;
            let mmu_regs_base_copy = mmu_regs_base;
            let boot_script_path_opt = boot_script_path.clone();
            let print_cycles_flag = print_cycles;
            let cpu_handle = thread::spawn(move || {
                // Everything below mirrors the normal run loop path
                // no remaining step counter in GUI-only mode
                let mut run_until_target: Option<u16> = None;
                // Track last seen Timer pending state to avoid spamming UI
                let mut last_timer_pending: Option<bool> = None;
                let prev_mode = if console_stdin_flag {
                    console_mode::enter_raw()
                } else {
                    None
                };
                let rx = if console_stdin_flag {
                    use std::io::Read;
                    use std::sync::mpsc;
                    let (tx, rx) = mpsc::channel::<Vec<u8>>();
                    thread::spawn(move || {
                        let mut stdin = std::io::stdin();
                        let mut buf = [0u8; 256];
                        loop {
                            match stdin.read(&mut buf) {
                                Ok(0) => break,
                                Ok(n) => {
                                    // Detect Ctrl+C (ETX=0x03) in raw mode and request shutdown
                                    if buf[..n].iter().copied().any(|b| b == 0x03) {
                                        STOP_REQUESTED.store(true, Ordering::SeqCst);
                                        break;
                                    }
                                    let _ = tx.send(buf[..n].to_vec());
                                }
                                Err(_) => break,
                            }
                        }
                    });
                    Some(rx)
                } else {
                    None
                };

                // Boot script
                let mut boot_seq: Option<BootSequencer> = if let Some(p) = &boot_script_path_opt {
                    match std::fs::read_to_string(p) {
                        Ok(s) => parse_boot_script(&s).ok().map(BootSequencer::new),
                        Err(_) => None,
                    }
                } else {
                    None
                };

                if paused_start_flag {
                    viewer_cpu.set_paused(true);
                    let snap = regview::RegSnapshot {
                        a: cpu_worker.r.a,
                        b: cpu_worker.r.b,
                        d: ((cpu_worker.r.a as u16) << 8) | (cpu_worker.r.b as u16),
                        x: cpu_worker.r.x,
                        y: cpu_worker.r.y,
                        u: cpu_worker.r.u,
                        s: cpu_worker.r.s,
                        dp: cpu_worker.r.dp,
                        pc: cpu_worker.r.pc,
                        cc: cpu_worker.r.cc,
                    };
                    viewer_cpu.push(snap);
                }

                'run_loop: while !STOP_REQUESTED.load(Ordering::SeqCst) {
                    process_settings_worker_queue(Some(&viewer_cpu), bus_worker.as_mut());
                    if STOP_REQUESTED.load(Ordering::SeqCst) {
                        // Request GUI to close and break
                        viewer_cpu.request_close();
                        break 'run_loop;
                    }
                    if viewer_cpu.is_paused() {
                        let snap = regview::RegSnapshot {
                            a: cpu_worker.r.a,
                            b: cpu_worker.r.b,
                            d: ((cpu_worker.r.a as u16) << 8) | (cpu_worker.r.b as u16),
                            x: cpu_worker.r.x,
                            y: cpu_worker.r.y,
                            u: cpu_worker.r.u,
                            s: cpu_worker.r.s,
                            dp: cpu_worker.r.dp,
                            pc: cpu_worker.r.pc,
                            cc: cpu_worker.r.cc,
                        };
                        viewer_cpu.push(snap);
                        let (mstart, mlen, mfollow) = viewer_cpu.get_memory_window();
                        let eff_start: u16 = if mfollow {
                            cpu_worker.r.pc & 0xFFF0
                        } else {
                            mstart
                        };
                        let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                        let mut a = eff_start;
                        for _ in 0..mlen {
                            buf.push(bus_worker.as_mut().read8(a));
                            a = a.wrapping_add(1);
                        }
                        viewer_cpu.update_memory_window(eff_start, buf);
                        let (consumed, _) =
                            em6809::disasm::disasm_one_hex(bus_worker.as_mut(), cpu_worker.r.pc);
                        let clen = core::cmp::max(consumed as usize, 1);
                        viewer_cpu.set_memory_highlight(cpu_worker.r.pc, clen);
                        // While paused, keep UI responsive and update memory pane on setting changes
                        let (mut mstart, mut mlen, mut mfollow) = viewer_cpu.get_memory_window();
                        while viewer_cpu.should_pause() {
                            if STOP_REQUESTED.load(Ordering::SeqCst) {
                                break 'run_loop;
                            }
                            // Detect Memory pane setting changes (Follow PC / Start / Len)
                            let (ns, nl, nf) = viewer_cpu.get_memory_window();
                            if ns != mstart || nl != mlen || nf != mfollow {
                                mstart = ns;
                                mlen = nl;
                                mfollow = nf;
                                let eff_start: u16 = if mfollow {
                                    cpu_worker.r.pc & 0xFFF0
                                } else {
                                    mstart
                                };
                                let mut nbuf: Vec<u8> = Vec::with_capacity(mlen);
                                let mut a = eff_start;
                                for _ in 0..mlen {
                                    nbuf.push(bus_worker.as_mut().read8(a));
                                    a = a.wrapping_add(1);
                                }
                                viewer_cpu.update_memory_window(eff_start, nbuf);
                                let (consumed, _) = em6809::disasm::disasm_one_hex(
                                    bus_worker.as_mut(),
                                    cpu_worker.r.pc,
                                );
                                let clen = core::cmp::max(consumed as usize, 1);
                                viewer_cpu.set_memory_highlight(cpu_worker.r.pc, clen);
                            }
                            while let Some(cmd) = viewer_cpu.take_command_worker() {
                                match cmd {
                                    regview::Command::UiOpenSettings
                                    | regview::Command::UiOpenSettingsExternal => {}
                                    regview::Command::UiSetSettingsBaseline(_, _, _) => {}
                                    regview::Command::UiAckSettings { .. } => {}
                                    regview::Command::SetPc(a) => {
                                        cpu_worker.set_pc(a);
                                        viewer_cpu.set_memory_window(cpu_worker.r.pc, None, Some(true));
                                        let snap2 = regview::RegSnapshot {
                                            a: cpu_worker.r.a,
                                            b: cpu_worker.r.b,
                                            d: ((cpu_worker.r.a as u16) << 8)
                                                | (cpu_worker.r.b as u16),
                                            x: cpu_worker.r.x,
                                            y: cpu_worker.r.y,
                                            u: cpu_worker.r.u,
                                            s: cpu_worker.r.s,
                                            dp: cpu_worker.r.dp,
                                            pc: cpu_worker.r.pc,
                                            cc: cpu_worker.r.cc,
                                        };
                                        viewer_cpu.push(snap2);
                                    }
                                    regview::Command::RefreshHexListing => {
                                        let spans = coalesce_ranges(loaded_ranges_worker.clone());
                                        let (policy, _win_len) = viewer_cpu.get_span_policy();
                                        let span_for_pc = spans
                                            .iter()
                                            .find(|(s, e)| *s <= cpu_worker.r.pc && cpu_worker.r.pc < *e)
                                            .copied();
                                        let (start, total_len) = if policy == 1 {
                                            let (ws, wl, _) = viewer_cpu.get_memory_window();
                                            (ws, std::cmp::max(wl, 256))
                                        } else if let Some((s, e)) = span_for_pc {
                                            (s, e.wrapping_sub(s) as usize)
                                        } else {
                                            let (ws, wl, _) = viewer_cpu.get_memory_window();
                                            (ws, std::cmp::max(wl, 256))
                                        };
                                        let mut cur = start;
                                        let mut out = String::new();
                                        let mut processed: usize = 0;
                                        while processed < total_len {
                                            let (consumed, text) = em6809::disasm::disasm_one_hex(bus_worker.as_mut(), cur);
                                            use core::fmt::Write as _;
                                            let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                            let step = core::cmp::max(consumed as usize, 1);
                                            processed += step;
                                            cur = cur.wrapping_add(step as u16);
                                        }
                                        viewer_cpu.update_hex_listing(out);
                                    }
                                    regview::Command::RunUntil(a) => {
                                        run_until_target = Some(a);
                                        viewer_cpu.set_paused(false);
                                        break;
                                    }
                                    regview::Command::ApplyRegs(ns) => {
                                        cpu_worker.r.a = ns.a;
                                        cpu_worker.r.b = ns.b;
                                        cpu_worker.r.x = ns.x;
                                        cpu_worker.r.y = ns.y;
                                        cpu_worker.r.u = ns.u;
                                        cpu_worker.r.s = ns.s;
                                        cpu_worker.r.dp = ns.dp;
                                        cpu_worker.r.pc = ns.pc;
                                        cpu_worker.r.cc = ns.cc;
                                        let snap2 = regview::RegSnapshot {
                                            a: cpu_worker.r.a,
                                            b: cpu_worker.r.b,
                                            d: ((cpu_worker.r.a as u16) << 8)
                                                | (cpu_worker.r.b as u16),
                                            x: cpu_worker.r.x,
                                            y: cpu_worker.r.y,
                                            u: cpu_worker.r.u,
                                            s: cpu_worker.r.s,
                                            dp: cpu_worker.r.dp,
                                            pc: cpu_worker.r.pc,
                                            cc: cpu_worker.r.cc,
                                        };
                                        viewer_cpu.push(snap2);
                                    }
                                    regview::Command::OpenImage { path, base } => {
                                        // Handle file load on worker side (macOS/Linux GUI-main-thread mode)
                                        match std::fs::read(&path) {
                                            Ok(bytes) => {
                                                // Clear logical memory before loading new image
                                                for addr in 0u32..=0xFFFF {
                                                    bus_worker.as_mut().write8(addr as u16, 0);
                                                }
                                                let mut new_loaded = LoadedImage {
                                                    loaded_ranges: Vec::new(),
                                                    entry: None,
                                                };
                                                let lower = path
                                                    .extension()
                                                    .and_then(|s| s.to_str())
                                                    .unwrap_or("")
                                                    .to_lowercase();
                                                if lower == "s19" || lower == "srec" {
                                                    let text = String::from_utf8_lossy(&bytes);
                                                    match loader::load_srec_bus(
                                                        bus_worker.as_mut(),
                                                        &text,
                                                    ) {
                                                        Ok(li) => new_loaded = li,
                                                        Err(e) => {
                                                            eprintln!("Open failed (SREC): {e}");
                                                        }
                                                    }
                                                } else if lower == "bin" {
                                                    let base_addr = base.unwrap_or(0);
                                                    new_loaded = loader::load_binary_bus(
                                                        bus_worker.as_mut(),
                                                        base_addr,
                                                        &bytes,
                                                    );
                                                } else {
                                                    eprintln!(
                                                        "Unsupported extension for Open: {}",
                                                        lower
                                                    );
                                                }
                                                // PC init priority: reset vector if valid; else SREC entry; else first loaded range
                                                let mut new_pc: Option<u16> = None;
                                                let pch = bus_worker.as_mut().read8(0xFFFE) as u16;
                                                let pcl = bus_worker.as_mut().read8(0xFFFF) as u16;
                                                let vec_pc = (pch << 8) | pcl;
                                                if vec_pc != 0x0000 {
                                                    new_pc = Some(vec_pc);
                                                }
                                                if new_pc.is_none() {
                                                    new_pc = new_loaded.entry;
                                                }
                                                if new_pc.is_none() {
                                                    if let Some((s, _e)) =
                                                        new_loaded.loaded_ranges.first().copied()
                                                    {
                                                        new_pc = Some(s);
                                                    }
                                                }
                                                if let Some(pc) = new_pc {
                                                    cpu_worker.r.pc = pc;
                                                    // Also align memory pane to program start for visibility
                                                    viewer_cpu.set_memory_window(pc, None, Some(true));
                                                }
                                                log_program_info(&new_loaded, cpu_worker.r.pc);
                                                let snap = regview::RegSnapshot {
                                                    a: cpu_worker.r.a,
                                                    b: cpu_worker.r.b,
                                                    d: ((cpu_worker.r.a as u16) << 8)
                                                        | (cpu_worker.r.b as u16),
                                                    x: cpu_worker.r.x,
                                                    y: cpu_worker.r.y,
                                                    u: cpu_worker.r.u,
                                                    s: cpu_worker.r.s,
                                                    dp: cpu_worker.r.dp,
                                                    pc: cpu_worker.r.pc,
                                                    cc: cpu_worker.r.cc,
                                                };
                                                viewer_cpu.push(snap);
                                                // Update worker's span tracking to new image
                                                loaded_ranges_worker =
                                                    new_loaded.loaded_ranges.clone();
                                                viewer_cpu.force_hex_view();
                                                // Pause after load as requested
                                                viewer_cpu.set_paused(true);
                                                // refresh memory window and listing for new content
                                                let (mstart_w, mlen_w, mfollow_w) =
                                                    viewer_cpu.get_memory_window();
                                                let eff_start_w: u16 = if mfollow_w {
                                                    cpu_worker.r.pc & 0xFFF0
                                                } else {
                                                    mstart_w
                                                };
                                                if mfollow_w {
                                                    viewer_cpu.set_memory_window_start(
                                                        (cpu_worker.r.pc & 0xFFF0) as usize,
                                                    );
                                                }
                                                let mut nbuf: Vec<u8> = Vec::with_capacity(mlen_w);
                                                let mut a = eff_start_w;
                                                for _ in 0..mlen_w {
                                                    nbuf.push(bus_worker.as_mut().read8(a));
                                                    a = a.wrapping_add(1);
                                                }
                                                viewer_cpu.update_memory_window(eff_start_w, nbuf);
                                                let (consumed, _) = em6809::disasm::disasm_one_hex(
                                                    bus_worker.as_mut(),
                                                    cpu_worker.r.pc,
                                                );
                                                let clen = core::cmp::max(consumed as usize, 1);
                                                viewer_cpu
                                                    .set_memory_highlight(cpu_worker.r.pc, clen);
                                                // Update Hex view: always rebuild from current memory so Hex shows disassembly
                                                let spans =
                                                    coalesce_ranges(loaded.loaded_ranges.clone());
                                                if let Some((start, end)) = spans
                                                    .iter()
                                                    .find(|(s, e)| {
                                                        *s <= cpu_worker.r.pc
                                                            && cpu_worker.r.pc < *e
                                                    })
                                                    .copied()
                                                {
                                                    let mut cur = start;
                                                    let mut out = String::new();
                                                    let mut processed: usize = 0;
                                                    let total_len: usize =
                                                        end.wrapping_sub(start) as usize;
                                                    while processed < total_len {
                                                        let (consumed, text) =
                                                            em6809::disasm::disasm_one_hex(
                                                                bus_worker.as_mut(),
                                                                cur,
                                                            );
                                                        use core::fmt::Write as _;
                                                        let _ =
                                                            writeln!(&mut out, "{cur:04X}: {text}");
                                                        let step =
                                                            core::cmp::max(consumed as usize, 1);
                                                        processed += step;
                                                        cur = cur.wrapping_add(step as u16);
                                                    }
                                                    viewer_cpu.update_hex_listing(out);
                                                }
                                                sync_gpio_from_bus(
                                                    bus_worker.as_mut(),
                                                    &viewer_cpu,
                                                );
                                                sync_block_from_bus(
                                                    bus_worker.as_mut(),
                                                    &viewer_cpu,
                                                );
                                                request_reapply_settings_from_baseline();
                                                // If an adjacent .lst exists (case-insensitive), map and show it; else prefer Hex
                                                let lst_path = path.with_extension("lst");
                                                let lst_path_upper = path.with_extension("LST");
                                                if lst_path.exists() || lst_path_upper.exists() {
                                                    let p = if lst_path.exists() {
                                                        lst_path
                                                    } else {
                                                        lst_path_upper
                                                    };
                                                    viewer_cpu.request_load_lst(p);
                                                } else {
                                                    viewer_cpu.force_hex_view();
                                                }
                                            }
                                            Err(e) => eprintln!("Open failed: {e}"),
                                        }
                                    }
                                    regview::Command::UiPromptBinBase(_) => {
                                        // UI-only; handled by GUI thread
                                    }
                                    regview::Command::MemWrite(items) => {
                                        for (addr, val) in items {
                                            bus_worker.as_mut().write8(addr, val);
                                        }
                                        // Sync GPIO state immediately after edits
                                        {
                                            let mut pushed = false;
                                            if let Some(io_mem) = bus_worker
                                                .as_mut()
                                                .as_any_mut()
                                                .downcast_mut::<IoBus<Memory>>()
                                            {
                                                if io_mem.take_gpio_dirty() {
                                                    io_mem.with_gpio_mut(|g| {
                                                        let (o, d, b) = g.get_state();
                                                        viewer_cpu.sync_gpio(true, b, o, d);
                                                        pushed = true;
                                                    });
                                                }
                                            } else if let Some(io_mmu) = bus_worker
                                                .as_mut()
                                                .as_any_mut()
                                                .downcast_mut::<IoBus<Mc6829>>()
                                            {
                                                if io_mmu.take_gpio_dirty() {
                                                    io_mmu.with_gpio_mut(|g| {
                                                        let (o, d, b) = g.get_state();
                                                        viewer_cpu.sync_gpio(true, b, o, d);
                                                        pushed = true;
                                                    });
                                                }
                                            } else if let Some(wt) = bus_worker
                                                .as_mut()
                                                .as_any_mut()
                                                .downcast_mut::<bus::WriteTrack>()
                                            {
                                                let inner_any = wt.inner_any_mut();
                                                if let Some(io_mem) =
                                                    inner_any.downcast_mut::<IoBus<Memory>>()
                                                {
                                                    if io_mem.take_gpio_dirty() {
                                                        io_mem.with_gpio_mut(|g| {
                                                            let (o, d, b) = g.get_state();
                                                            viewer_cpu.sync_gpio(true, b, o, d);
                                                            pushed = true;
                                                        });
                                                    }
                                                } else if let Some(io_mmu) =
                                                    inner_any.downcast_mut::<IoBus<Mc6829>>()
                                                {
                                                    if io_mmu.take_gpio_dirty() {
                                                        io_mmu.with_gpio_mut(|g| {
                                                            let (o, d, b) = g.get_state();
                                                            viewer_cpu.sync_gpio(true, b, o, d);
                                                            pushed = true;
                                                        });
                                                    }
                                                }
                                            }
                                            let _ = pushed;
                                        }
                                        // After applying edits, refresh memory pane immediately
                                        let (mstart_w, mlen_w, mfollow_w) =
                                            viewer_cpu.get_memory_window();
                                        let eff_start_w: u16 = if mfollow_w {
                                            cpu_worker.r.pc & 0xFFF0
                                        } else {
                                            mstart_w
                                        };
                                        let mut nbuf: Vec<u8> = Vec::with_capacity(mlen_w);
                                        let mut a = eff_start_w;
                                        for _ in 0..mlen_w {
                                            nbuf.push(bus_worker.as_mut().read8(a));
                                            a = a.wrapping_add(1);
                                        }
                                        viewer_cpu.update_memory_window(eff_start_w, nbuf);
                                        // Also refresh listing pane (full span) immediately
                                        let spans = coalesce_ranges(loaded_ranges_worker.clone());
                                        if let Some((start, end)) = spans
                                            .iter()
                                            .find(|(s, e)| {
                                                *s <= cpu_worker.r.pc && cpu_worker.r.pc < *e
                                            })
                                            .copied()
                                        {
                                            let mut cur = start;
                                            let mut out = String::new();
                                            let mut processed: usize = 0;
                                            let total_len: usize = end.wrapping_sub(start) as usize;
                                            while processed < total_len {
                                                let (consumed, text) =
                                                    em6809::disasm::disasm_one_hex(
                                                        bus_worker.as_mut(),
                                                        cur,
                                                    );
                                                use core::fmt::Write as _;
                                                let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                                let step = core::cmp::max(consumed as usize, 1);
                                                processed += step;
                                                cur = cur.wrapping_add(step as u16);
                                            }
                                            viewer_cpu.update_hex_listing(out);
                                            if viewer_cpu.auto_hex_on_edit() {
                                                viewer_cpu.force_hex_view();
                                            }
                                        }
                                    }
                                    regview::Command::GpioSetOut(mask) => {
                                        if let Some(io_mem) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Memory>>()
                                        {
                                            io_mem.with_gpio_mut(|g| g.set_out(mask));
                                        } else if let Some(io_mmu) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Mc6829>>()
                                        {
                                            io_mmu.with_gpio_mut(|g| g.set_out(mask));
                                        } else if let Some(wt) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<bus::WriteTrack>()
                                        {
                                            let inner_any = wt.inner_any_mut();
                                            if let Some(io_mem) =
                                                inner_any.downcast_mut::<IoBus<Memory>>()
                                            {
                                                io_mem.with_gpio_mut(|g| g.set_out(mask));
                                            } else if let Some(io_mmu) =
                                                inner_any.downcast_mut::<IoBus<Mc6829>>()
                                            {
                                                io_mmu.with_gpio_mut(|g| g.set_out(mask));
                                            }
                                        }
                                    }
                                    regview::Command::UiSetGpio(..) => {}
                                    regview::Command::UiSetMmu(..) => {}
                                    regview::Command::UiSetTimer(..) => {}
                                    regview::Command::ApplyMmuPreset { name } => {
                                        let apply =
                                            |mmu: &mut Mc6829, viewer: Option<&regview::Viewer>| {
                                                let _ = em6809::config::apply_preset(mmu, &name);
                                                let (map, sys, task) = mmu.snapshot_current_map();
                                                if let Some(v) = viewer {
                                                    v.sync_mmu(sys, task, map);
                                                }
                                                if let Some((mut s, _origins, color_opt)) =
                                                    regview::get_latest_settings_baseline()
                                                {
                                                    s.use_mmu = true;
                                                    s.mmu_mode_sys = sys;
                                                    s.mmu_task = format!("{}", task & 0x07);
                                                    s.mmu_preset_name = name.clone();
                                                    let prot = mmu.read8(mmu.regs_base + 0x12);
                                                    s.mmu_prot_hex = format!("{:02X}", prot);
                                                    for (i, f) in map.iter().enumerate() {
                                                        s.mmu_map_frames[i] =
                                                            format!("{:02X}", f & 0x00FF);
                                                    }
                                                    for i in 0..16 {
                                                        let attr = mmu
                                                            .read8(mmu.regs_base + 0x18 + i as u16);
                                                        s.mmu_attr_bytes[i] =
                                                            format!("{:02X}", attr);
                                                    }
                                                    publish_settings_baseline(
                                                        &s,
                                                        color_opt.unwrap_or(0),
                                                    );
                                                }
                                            };
                                        if let Some(io_mmu) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Mc6829>>()
                                        {
                                            let inner = io_mmu.inner_any_mut();
                                            if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                                apply(mmu, Some(&viewer_cpu));
                                            }
                                        } else if let Some(mmu) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<Mc6829>()
                                        {
                                            apply(mmu, Some(&viewer_cpu));
                                        } else if let Some(wt) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<bus::WriteTrack>()
                                        {
                                            let inner_any = wt.inner_any_mut();
                                            if let Some(io_mmu) =
                                                inner_any.downcast_mut::<IoBus<Mc6829>>()
                                            {
                                                let inner2 = io_mmu.inner_any_mut();
                                                if let Some(mmu) = inner2.downcast_mut::<Mc6829>() {
                                                    apply(mmu, Some(&viewer_cpu));
                                                }
                                            } else if let Some(mmu) =
                                                inner_any.downcast_mut::<Mc6829>()
                                            {
                                                apply(mmu, Some(&viewer_cpu));
                                            }
                                        }
                                    }
                                    regview::Command::ApplyMmuMapFileText { text } => {
                                        let apply_text =
                                            |mmu: &mut Mc6829, viewer: Option<&regview::Viewer>| {
                                                let _ = em6809::config::apply_mmu_config_from_str(
                                                    mmu, &text,
                                                );
                                                let (map, sys, task) = mmu.snapshot_current_map();
                                                if let Some(v) = viewer {
                                                    v.sync_mmu(sys, task, map);
                                                }
                                                if let Some((mut s, _origins, color_opt)) =
                                                    regview::get_latest_settings_baseline()
                                                {
                                                    s.use_mmu = true;
                                                    s.mmu_mode_sys = sys;
                                                    s.mmu_task = format!("{}", task & 0x07);
                                                    s.mmu_preset_name.clear();
                                                    let prot = mmu.read8(mmu.regs_base + 0x12);
                                                    s.mmu_prot_hex = format!("{:02X}", prot);
                                                    for (i, f) in map.iter().enumerate() {
                                                        s.mmu_map_frames[i] =
                                                            format!("{:02X}", f & 0x00FF);
                                                    }
                                                    for i in 0..16 {
                                                        let attr = mmu
                                                            .read8(mmu.regs_base + 0x18 + i as u16);
                                                        s.mmu_attr_bytes[i] =
                                                            format!("{:02X}", attr);
                                                    }
                                                    publish_settings_baseline(
                                                        &s,
                                                        color_opt.unwrap_or(0),
                                                    );
                                                }
                                            };
                                        if let Some(io_mmu) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Mc6829>>()
                                        {
                                            let inner = io_mmu.inner_any_mut();
                                            if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                                apply_text(mmu, Some(&viewer_cpu));
                                            }
                                        } else if let Some(mmu) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<Mc6829>()
                                        {
                                            apply_text(mmu, Some(&viewer_cpu));
                                        } else if let Some(wt) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<bus::WriteTrack>()
                                        {
                                            let inner_any = wt.inner_any_mut();
                                            if let Some(io_mmu) =
                                                inner_any.downcast_mut::<IoBus<Mc6829>>()
                                            {
                                                let inner2 = io_mmu.inner_any_mut();
                                                if let Some(mmu) = inner2.downcast_mut::<Mc6829>() {
                                                    apply_text(mmu, Some(&viewer_cpu));
                                                }
                                            } else if let Some(mmu) =
                                                inner_any.downcast_mut::<Mc6829>()
                                            {
                                                apply_text(mmu, Some(&viewer_cpu));
                                            }
                                        }
                                    }
                                    regview::Command::ApplyMmu {
                                        sys_mode,
                                        task,
                                        prot,
                                        log_maps,
                                        maps,
                                        attrs,
                                    } => {
                                        // Apply settings to whichever MMU backend is active
                                        let apply = |mmu: &mut Mc6829| {
                                            if let Some(on) = log_maps {
                                                mmu.set_log_maps(on);
                                            }
                                            if let Some(t) = task {
                                                mmu.write8(mmu.regs_base + 0x10, t & 0x07);
                                            }
                                            if let Some(sys) = sys_mode {
                                                mmu.write8(
                                                    mmu.regs_base + 0x11,
                                                    if sys { 1 } else { 0 },
                                                );
                                            }
                                            if let Some(p) = prot {
                                                mmu.write8(mmu.regs_base + 0x12, p);
                                            }
                                            for (pg, fr) in &maps {
                                                mmu.set_map_entry(*pg, *fr);
                                            }
                                            for (pg, at) in &attrs {
                                                let off = 0x18u16 + (*pg as u16);
                                                mmu.write8(mmu.regs_base + off, *at);
                                            }
                                        };
                                        if let Some(io_mmu) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Mc6829>>()
                                        {
                                            let inner = io_mmu.inner_any_mut();
                                            if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                                apply(mmu);
                                            }
                                        } else if let Some(mmu) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<Mc6829>()
                                        {
                                            apply(mmu);
                                        } else if let Some(wt) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<bus::WriteTrack>()
                                        {
                                            let inner_any = wt.inner_any_mut();
                                            if let Some(io_mmu) =
                                                inner_any.downcast_mut::<IoBus<Mc6829>>()
                                            {
                                                let inner2 = io_mmu.inner_any_mut();
                                                if let Some(mmu) = inner2.downcast_mut::<Mc6829>() {
                                                    apply(mmu);
                                                }
                                            } else if let Some(mmu) =
                                                inner_any.downcast_mut::<Mc6829>()
                                            {
                                                apply(mmu);
                                            }
                                        }
                                    }
                                    regview::Command::MmuIdentityMap => {
                                        let ident = |mmu: &mut Mc6829| {
                                            mmu.identity_map_current();
                                        };
                                        if let Some(io_mmu) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Mc6829>>()
                                        {
                                            let inner = io_mmu.inner_any_mut();
                                            if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                                ident(mmu);
                                            }
                                        } else if let Some(mmu) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<Mc6829>()
                                        {
                                            ident(mmu);
                                        } else if let Some(wt) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<bus::WriteTrack>()
                                        {
                                            let inner_any = wt.inner_any_mut();
                                            if let Some(io_mmu) =
                                                inner_any.downcast_mut::<IoBus<Mc6829>>()
                                            {
                                                let inner2 = io_mmu.inner_any_mut();
                                                if let Some(mmu) = inner2.downcast_mut::<Mc6829>() {
                                                    ident(mmu);
                                                }
                                            } else if let Some(mmu) =
                                                inner_any.downcast_mut::<Mc6829>()
                                            {
                                                ident(mmu);
                                            }
                                        }
                                    }
                                    regview::Command::ApplyConsole {
                                        enable,
                                        base,
                                        out_path,
                                        stdin: _stdin,
                                        echo,
                                        flush_every,
                                        flush_newline: _,
                                        rx_watermark: _,
                                        irq_hold: _,
                                        tee_stderr: _,
                                        ..
                                    } => {
                                        if let Some(iobus) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Memory>>()
                                        {
                                            let b = base.unwrap_or(0xFF00);
                                            iobus.ensure_console(enable, b);
                                            iobus.with_console_mut_opt(|c| {
                                                if let Some(p) = &out_path {
                                                    c.set_out_file(p);
                                                }
                                                c.set_local_echo(echo);
                                                if let Some(n) = flush_every {
                                                    c.set_flush_every(n);
                                                }
                                            });
                                        } else if let Some(iobus) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Mc6829>>()
                                        {
                                            let b = base.unwrap_or(0xFF00);
                                            iobus.ensure_console(enable, b);
                                            iobus.with_console_mut_opt(|c| {
                                                if let Some(p) = &out_path {
                                                    c.set_out_file(p);
                                                }
                                                c.set_local_echo(echo);
                                                if let Some(n) = flush_every {
                                                    c.set_flush_every(n);
                                                }
                                            });
                                        }
                                    }
                                    regview::Command::ApplyBlock {
                                        enable,
                                        base,
                                        file_path,
                                        ..
                                    } => {
                                        if let Some(iobus) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Memory>>()
                                        {
                                            let b = base.unwrap_or(0xFF20);
                                            iobus.ensure_block(enable, b);
                                            if let Some(p) = file_path {
                                                iobus.with_block_mut_opt(|blk| {
                                                    blk.set_backing_file(&p)
                                                });
                                            }
                                        } else if let Some(iobus) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Mc6829>>()
                                        {
                                            let b = base.unwrap_or(0xFF20);
                                            iobus.ensure_block(enable, b);
                                            if let Some(p) = file_path {
                                                iobus.with_block_mut_opt(|blk| {
                                                    blk.set_backing_file(&p)
                                                });
                                            }
                                        }
                                        sync_block_from_bus(bus_worker.as_mut(), &viewer_cpu);
                                    }
                                    regview::Command::ApplyGpio { enable, base, bits } => {
                                        let b = base.unwrap_or(0xFF40);
                                        let k = bits.unwrap_or(8);
                                        if let Some(iobus) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Memory>>()
                                        {
                                            iobus.ensure_gpio(enable, b, k);
                                            if enable {
                                                iobus.with_gpio_mut_opt(|g| {
                                                    let (out, dir, bits) = g.get_state();
                                                    viewer_cpu.sync_gpio(true, bits, out, dir);
                                                });
                                            } else {
                                                viewer_cpu.sync_gpio(false, 0, 0, 0);
                                            }
                                        } else if let Some(iobus) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Mc6829>>()
                                        {
                                            iobus.ensure_gpio(enable, b, k);
                                            if enable {
                                                iobus.with_gpio_mut_opt(|g| {
                                                    let (out, dir, bits) = g.get_state();
                                                    viewer_cpu.sync_gpio(true, bits, out, dir);
                                                });
                                            } else {
                                                viewer_cpu.sync_gpio(false, 0, 0, 0);
                                            }
                                        }
                                    }
                                    regview::Command::ApplyTimer {
                                        enable,
                                        base,
                                        reload,
                                        irq_enable,
                                        firq,
                                        start,
                                    } => {
                                        let b = base.unwrap_or(0xFF10);
                                        if let Some(iobus) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Memory>>()
                                        {
                                            iobus.ensure_timer(enable, b);
                                            if let Some(r) = reload {
                                                iobus.with_timer_mut_opt(|t| t.set_reload(r));
                                            }
                                            iobus.with_timer_mut_opt(|t| {
                                                t.set_irq_enable(irq_enable);
                                                t.set_firq(firq);
                                                if start {
                                                    t.start();
                                                } else {
                                                    t.stop();
                                                }
                                            });
                                            if enable {
                                                iobus.with_timer_mut_opt(|t| {
                                                    let (run, _irq_en, _firq, pending) =
                                                        t.get_state();
                                                    viewer_cpu.sync_timer(true, pending, run);
                                                });
                                            } else {
                                                viewer_cpu.sync_timer(false, false, false);
                                            }
                                        } else if let Some(iobus) = bus_worker
                                            .as_mut()
                                            .as_any_mut()
                                            .downcast_mut::<IoBus<Mc6829>>()
                                        {
                                            iobus.ensure_timer(enable, b);
                                            if let Some(r) = reload {
                                                iobus.with_timer_mut_opt(|t| t.set_reload(r));
                                            }
                                            if enable {
                                                iobus.with_timer_mut_opt(|t| {
                                                    let (run, _irq_en, _firq, pending) =
                                                        t.get_state();
                                                    viewer_cpu.sync_timer(true, pending, run);
                                                });
                                            } else {
                                                viewer_cpu.sync_timer(false, false, false);
                                            }
                                        }
                                    }
                                    regview::Command::SavePrefs {
                                        settings,
                                        gpio_on_color,
                                        instance_id,
                                    } => {
                                        let payload = settings_worker::SettingsPayload {
                                            settings,
                                            gpio_on_color,
                                            instance_id,
                                        };
                                        if apply_settings_runtime(
                                            Some(&viewer_cpu),
                                            bus_worker.as_mut(),
                                            &payload,
                                        ) {
                                            persist_settings_snapshot(
                                                &payload.settings,
                                                payload.gpio_on_color,
                                            );
                                            publish_settings_baseline(
                                                &payload.settings,
                                                payload.gpio_on_color,
                                            );
                                            send_settings_ack(payload.instance_id);
                                        }
                                    }

                                    regview::Command::UiLoadLst(_) => {}
                                    regview::Command::UiSetGpioOnColor(_) => {}
                                    regview::Command::UiSetBlockStatus { .. } => {}
                                }
                            }
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                    }

                    if let Some(ref rxch) = rx {
                        if let Ok(v) = rxch.try_recv() {
                            if let Some(io_mem) = bus_worker
                                .as_mut()
                                .as_any_mut()
                                .downcast_mut::<IoBus<Memory>>()
                            {
                                io_mem.feed_console_input(&v);
                            } else if let Some(io_mmu) = bus_worker
                                .as_mut()
                                .as_any_mut()
                                .downcast_mut::<IoBus<Mc6829>>()
                            {
                                io_mmu.feed_console_input(&v);
                            } else if let Some(wt) = bus_worker
                                .as_mut()
                                .as_any_mut()
                                .downcast_mut::<bus::WriteTrack>()
                            {
                                let inner_any = wt.inner_any_mut();
                                if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
                                    io_mem.feed_console_input(&v);
                                } else if let Some(io_mmu) =
                                    inner_any.downcast_mut::<IoBus<Mc6829>>()
                                {
                                    io_mmu.feed_console_input(&v);
                                }
                            }
                        }
                    }
                    if let Some(ref mut seq) = boot_seq {
                        seq.on_pre_step(
                            bus_worker.as_mut(),
                            mmu_regs_base_copy,
                            cpu_worker.r.pc,
                            &mut cpu_worker,
                        );
                    }
                    // Push GPIO/MMU snapshots for GUI while paused
                    if viewer_cpu.is_paused() {
                        // GPIO
                        let mut present = false;
                        let mut out = 0u32;
                        let mut dir = 0xFFFF_FFFFu32;
                        let mut bits = 8u8;
                        if let Some(io_mem) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Memory>>()
                        {
                            io_mem.with_gpio_mut(|g| {
                                let (o, d, b) = g.get_state();
                                out = o;
                                dir = d;
                                bits = b;
                                present = true;
                            });
                        } else if let Some(io_mmu_io) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Mc6829>>()
                        {
                            io_mmu_io.with_gpio_mut(|g| {
                                let (o, d, b) = g.get_state();
                                out = o;
                                dir = d;
                                bits = b;
                                present = true;
                            });
                        } else if let Some(wt) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<bus::WriteTrack>()
                        {
                            let inner_any = wt.inner_any_mut();
                            if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
                                io_mem.with_gpio_mut(|g| {
                                    let (o, d, b) = g.get_state();
                                    out = o;
                                    dir = d;
                                    bits = b;
                                    present = true;
                                });
                            } else if let Some(io_mmu_io) =
                                inner_any.downcast_mut::<IoBus<Mc6829>>()
                            {
                                io_mmu_io.with_gpio_mut(|g| {
                                    let (o, d, b) = g.get_state();
                                    out = o;
                                    dir = d;
                                    bits = b;
                                    present = true;
                                });
                            }
                        }
                        if present {
                            viewer_cpu.sync_gpio(true, bits, out, dir);
                        }
                        // MMU current map
                        if let Some(io_mmu_io) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Mc6829>>()
                        {
                            let inner = io_mmu_io.inner_any_mut();
                            if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                let (map, sys, task) = mmu.snapshot_current_map();
                                viewer_cpu.sync_mmu(sys, task, map);
                            }
                        } else if let Some(mmu) =
                            bus_worker.as_mut().as_any_mut().downcast_mut::<Mc6829>()
                        {
                            let (map, sys, task) = mmu.snapshot_current_map();
                            viewer_cpu.sync_mmu(sys, task, map);
                        }
                        // Timer state
                        let mut t_present = false;
                        let mut t_pending = false;
                        let mut t_running = false;
                        if let Some(io_mem) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Memory>>()
                        {
                            io_mem.with_timer_mut(|t| {
                                let (run, _irq, _firq, p) = t.get_state();
                                t_present = true;
                                t_pending = p;
                                t_running = run;
                            });
                        } else if let Some(io_mmu_io) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Mc6829>>()
                        {
                            io_mmu_io.with_timer_mut(|t| {
                                let (run, _irq, _firq, p) = t.get_state();
                                t_present = true;
                                t_pending = p;
                                t_running = run;
                            });
                        } else if let Some(wt) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<bus::WriteTrack>()
                        {
                            let inner_any = wt.inner_any_mut();
                            if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
                                io_mem.with_timer_mut(|t| {
                                    let (run, _irq, _firq, p) = t.get_state();
                                    t_present = true;
                                    t_pending = p;
                                    t_running = run;
                                });
                            } else if let Some(io_mmu_io) =
                                inner_any.downcast_mut::<IoBus<Mc6829>>()
                            {
                                io_mmu_io.with_timer_mut(|t| {
                                    let (run, _irq, _firq, p) = t.get_state();
                                    t_present = true;
                                    t_pending = p;
                                    t_running = run;
                                });
                            }
                        }
                        if t_present {
                            viewer_cpu.sync_timer(true, t_pending, t_running);
                        }
                    }
                    // trace printing removed in GUI-only mode
                    cpu_worker.step(bus_worker.as_mut(), false);
                    // Dirty-driven memory window refresh while running
                    {
                        let (mstart, mlen, mfollow) = viewer_cpu.get_memory_window();
                        let eff_start: u16 = if mfollow {
                            cpu_worker.r.pc & 0xFFF0
                        } else {
                            mstart
                        };
                        let mut need_reload = false;
                        if mfollow && eff_start != mstart {
                            viewer_cpu.set_memory_window_start(eff_start as usize);
                            need_reload = true;
                        }
                        // Program IoBus to watch this window and refresh if dirty
                        if let Some(io_mem) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Memory>>()
                        {
                            io_mem.set_mem_watch(eff_start, mlen);
                            if io_mem.take_mem_dirty() {
                                need_reload = true;
                            }
                        } else if let Some(io_mmu) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Mc6829>>()
                        {
                            io_mmu.set_mem_watch(eff_start, mlen);
                            if io_mmu.take_mem_dirty() {
                                need_reload = true;
                            }
                        } else if let Some(wt) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<bus::WriteTrack>()
                        {
                            let inner_any = wt.inner_any_mut();
                            if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
                                io_mem.set_mem_watch(eff_start, mlen);
                                if io_mem.take_mem_dirty() {
                                    need_reload = true;
                                }
                            } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
                                io_mmu.set_mem_watch(eff_start, mlen);
                                if io_mmu.take_mem_dirty() {
                                    need_reload = true;
                                }
                            }
                        }
                        if need_reload {
                            let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                            let mut a = eff_start;
                            for _ in 0..mlen {
                                buf.push(bus_worker.as_mut().read8(a));
                                a = a.wrapping_add(1);
                            }
                            viewer_cpu.update_memory_window(eff_start, buf);
                        }
                        let (consumed, _) =
                            em6809::disasm::disasm_one_hex(bus_worker.as_mut(), cpu_worker.r.pc);
                        let clen = core::cmp::max(consumed as usize, 1);
                        viewer_cpu.set_memory_highlight(cpu_worker.r.pc, clen);
                    }
                    // While running, push GPIO changes (poll every frame)
                    {
                        if let Some(io_mem) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Memory>>()
                        {
                            io_mem.with_gpio_mut(|g| {
                                let (o, d, b) = g.get_state();
                                viewer_cpu.sync_gpio(true, b, o, d);
                            });
                        } else if let Some(io_mmu) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Mc6829>>()
                        {
                            io_mmu.with_gpio_mut(|g| {
                                let (o, d, b) = g.get_state();
                                viewer_cpu.sync_gpio(true, b, o, d);
                            });
                        } else if let Some(wt) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<bus::WriteTrack>()
                        {
                            let inner_any = wt.inner_any_mut();
                            if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
                                io_mem.with_gpio_mut(|g| {
                                    let (o, d, b) = g.get_state();
                                    viewer_cpu.sync_gpio(true, b, o, d);
                                });
                            } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
                                io_mmu.with_gpio_mut(|g| {
                                    let (o, d, b) = g.get_state();
                                    viewer_cpu.sync_gpio(true, b, o, d);
                                });
                            }
                        }
                    }
                    // Poll to keep GPIO LEDs in sync even if dirty is missed
                    sync_gpio_from_bus(bus_worker.as_mut(), &viewer_cpu);
                    {
                        let mut block_dirty = false;
                        if let Some(io_mem) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Memory>>()
                        {
                            if io_mem.take_block_dirty() {
                                block_dirty = true;
                            }
                        } else if let Some(io_mmu) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Mc6829>>()
                        {
                            if io_mmu.take_block_dirty() {
                                block_dirty = true;
                            }
                        } else if let Some(wt) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<bus::WriteTrack>()
                        {
                            let inner_any = wt.inner_any_mut();
                            if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
                                if io_mem.take_block_dirty() {
                                    block_dirty = true;
                                }
                            } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
                                if io_mmu.take_block_dirty() {
                                    block_dirty = true;
                                }
                            }
                        }
                        if block_dirty {
                            sync_block_from_bus(bus_worker.as_mut(), &viewer_cpu);
                        }
                    }
                    // While running, update Timer indicator on change (avoid flooding)
                    if let Some(viewer) = Some(&viewer_cpu) {
                        let mut present = false;
                        let mut pending = false;
                        if let Some(io_mem) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Memory>>()
                        {
                            io_mem.with_timer_mut(|t| {
                                let (_r, _i, _f, p) = t.get_state();
                                present = true;
                                pending = p;
                            });
                        } else if let Some(io_mmu) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<IoBus<Mc6829>>()
                        {
                            io_mmu.with_timer_mut(|t| {
                                let (_r, _i, _f, p) = t.get_state();
                                present = true;
                                pending = p;
                            });
                        } else if let Some(wt) = bus_worker
                            .as_mut()
                            .as_any_mut()
                            .downcast_mut::<bus::WriteTrack>()
                        {
                            let inner_any = wt.inner_any_mut();
                            if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
                                io_mem.with_timer_mut(|t| {
                                    let (_r, _i, _f, p) = t.get_state();
                                    present = true;
                                    pending = p;
                                });
                            } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
                                io_mmu.with_timer_mut(|t| {
                                    let (_r, _i, _f, p) = t.get_state();
                                    present = true;
                                    pending = p;
                                });
                            }
                        }
                        if present && last_timer_pending.map(|v| v != pending).unwrap_or(true) {
                            viewer.sync_timer(true, pending, /*running*/ true);
                            last_timer_pending = Some(pending);
                        }
                    }
                    if !viewer_cpu.is_paused() {
                        let snap = regview::RegSnapshot {
                            a: cpu_worker.r.a,
                            b: cpu_worker.r.b,
                            d: ((cpu_worker.r.a as u16) << 8) | (cpu_worker.r.b as u16),
                            x: cpu_worker.r.x,
                            y: cpu_worker.r.y,
                            u: cpu_worker.r.u,
                            s: cpu_worker.r.s,
                            dp: cpu_worker.r.dp,
                            pc: cpu_worker.r.pc,
                            cc: cpu_worker.r.cc,
                        };
                        viewer_cpu.push(snap);
                    }
                    if let Some(ref mut seq) = boot_seq {
                        seq.on_post_step(bus_worker.as_mut(), mmu_regs_base_copy);
                    }
                    if let Some(tgt) = run_until_target {
                        if cpu_worker.r.pc == tgt {
                            viewer_cpu.set_paused(true);
                            run_until_target = None;
                        }
                    }
                    // no step counter; no CLI breakpoints in GUI-only mode
                }
                if let Some(prev) = prev_mode {
                    console_mode::restore_stdin_mode(prev);
                }
                // print cycles
                if print_cycles_flag {
                    println!("cycles: {}", cpu_worker.cycles);
                }
            });

            // Watch for Ctrl+C (SIGINT) and proactively request GUI close
            let viewer_close = viewer_main.clone();
            let _sig_watch = thread::spawn(move || {
                while !STOP_REQUESTED.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                viewer_close.request_close();
            });
            // Run GUI on main thread (blocks until closed)
            viewer_main.run_blocking("em6809 - MC6809 Software Emulator");
            STOP_REQUESTED.store(true, Ordering::SeqCst);
            let _ = cpu_handle.join();
            return;
        }
    }

    if true {
        signals::install();
        // GUI-only: no --step; run until Ctrl+C / GUI close / breakpoints
        let mut run_until_target: Option<u16> = None;
        // Generic stdin feeder for dyn Bus path, feeding IoBus<Memory|Mc6829>
        // On Windows, switch stdin to raw (no echo, no line input) so keystrokes are delivered immediately
        let prev_mode = if console_stdin {
            console_mode::enter_raw()
        } else {
            None
        };
        let rx = if console_stdin {
            use std::io::Read;
            use std::sync::mpsc;
            use std::thread;
            let (tx, rx) = mpsc::channel::<Vec<u8>>();
            thread::spawn(move || {
                let mut stdin = std::io::stdin();
                let mut buf = [0u8; 256];
                loop {
                    match stdin.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let _ = tx.send(buf[..n].to_vec());
                        }
                        Err(_) => break,
                    }
                }
            });
            Some(rx)
        } else {
            None
        };
        // Event-driven updates based on write tracking
        'run_loop: while !STOP_REQUESTED.load(Ordering::SeqCst) {
            process_settings_worker_queue(reg_viewer.as_ref(), bus.as_mut());
            if let Some(viewer) = &reg_viewer {
                if viewer.is_closed() {
                    // GUI window closed -> exit
                    STOP_REQUESTED.store(true, Ordering::SeqCst);
                    break 'run_loop;
                }
                if viewer.quit_requested() {
                    STOP_REQUESTED.store(true, Ordering::SeqCst);
                    break 'run_loop;
                }
            }
            // Pre-step pause gating so initial pause prevents executing first instruction
            if let Some(viewer) = &reg_viewer {
                // Push current snapshot for initial/highlight updates while paused
                let snap = regview::RegSnapshot {
                    a: cpu.r.a,
                    b: cpu.r.b,
                    d: ((cpu.r.a as u16) << 8) | (cpu.r.b as u16),
                    x: cpu.r.x,
                    y: cpu.r.y,
                    u: cpu.r.u,
                    s: cpu.r.s,
                    dp: cpu.r.dp,
                    pc: cpu.r.pc,
                    cc: cpu.r.cc,
                };
                viewer.push(snap);
                // Handle queued commands even when not paused
                while let Some(cmd) = viewer.take_command_worker() {
                    match cmd {
                        regview::Command::UiOpenSettings
                        | regview::Command::UiOpenSettingsExternal => {}
                        regview::Command::UiSetSettingsBaseline(_, _, _) => {}
                        regview::Command::UiAckSettings { .. } => {}
                        regview::Command::UiSetGpioOnColor(_) => {}
                        regview::Command::UiSetBlockStatus { .. } => {}
                        regview::Command::SavePrefs {
                            settings,
                            gpio_on_color,
                            instance_id,
                        } => {
                            let payload = settings_worker::SettingsPayload {
                                settings,
                                gpio_on_color,
                                instance_id,
                            };
                            if apply_settings_runtime(Some(viewer), bus.as_mut(), &payload) {
                                persist_settings_snapshot(
                                    &payload.settings,
                                    payload.gpio_on_color,
                                );
                                publish_settings_baseline(
                                    &payload.settings,
                                    payload.gpio_on_color,
                                );
                                send_settings_ack(payload.instance_id);
                            }
                        }
                        regview::Command::ApplyRegs(ns) => {
                            cpu.r.a = ns.a;
                            cpu.r.b = ns.b;
                            cpu.r.x = ns.x;
                            cpu.r.y = ns.y;
                            cpu.r.u = ns.u;
                            cpu.r.s = ns.s;
                            cpu.r.dp = ns.dp;
                            cpu.r.pc = ns.pc;
                            cpu.r.cc = ns.cc;
                            let snap2 = regview::RegSnapshot {
                                a: cpu.r.a,
                                b: cpu.r.b,
                                d: ((cpu.r.a as u16) << 8) | (cpu.r.b as u16),
                                x: cpu.r.x,
                                y: cpu.r.y,
                                u: cpu.r.u,
                                s: cpu.r.s,
                                dp: cpu.r.dp,
                                pc: cpu.r.pc,
                                cc: cpu.r.cc,
                            };
                            viewer.push(snap2);
                            let (mstart2, mlen2, mfollow2) = viewer.get_memory_window();
                            let eff_start2: u16 =
                                if mfollow2 { cpu.r.pc & 0xFFF0 } else { mstart2 };
                            let mut buf2: Vec<u8> = Vec::with_capacity(mlen2);
                            let mut a2 = eff_start2;
                            for _ in 0..mlen2 {
                                buf2.push(bus.as_mut().read8(a2));
                                a2 = a2.wrapping_add(1);
                            }
                            viewer.update_memory_window(eff_start2, buf2);
                            let (consumed, _) =
                                em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc);
                            let clen2 = core::cmp::max(consumed as usize, 1);
                            viewer.set_memory_highlight(cpu.r.pc, clen2);
                        }
                        regview::Command::MemWrite(items) => {
                            for (addr, val) in items {
                                bus.as_mut().write8(addr, val);
                            }
                            let (mstart2, mlen2, mfollow2) = viewer.get_memory_window();
                            let eff_start2: u16 =
                                if mfollow2 { cpu.r.pc & 0xFFF0 } else { mstart2 };
                            let mut buf2: Vec<u8> = Vec::with_capacity(mlen2);
                            let mut a2 = eff_start2;
                            for _ in 0..mlen2 {
                                buf2.push(bus.as_mut().read8(a2));
                                a2 = a2.wrapping_add(1);
                            }
                            viewer.update_memory_window(eff_start2, buf2);
                            let spans = coalesce_ranges(loaded.loaded_ranges.clone());
                            let span_for_pc =
                                spans.iter().find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e).copied();
                            let (start, end, total_len_override) = span_for_pc
                                .map(|(s, e)| (s, e, None))
                                .unwrap_or_else(|| {
                                    let (ws, wl, _) = viewer.get_memory_window();
                                    (ws, ws.wrapping_add(wl as u16), Some(wl))
                                });
                            let mut cur = start;
                            let mut out = String::new();
                            let mut processed: usize = 0;
                            let total_len: usize = total_len_override.unwrap_or_else(|| {
                                let span = end.wrapping_sub(start) as usize;
                                std::cmp::max(span, 256)
                            });
                            while processed < total_len {
                                let (consumed, text) =
                                    em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                use core::fmt::Write as _;
                                let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                let step = core::cmp::max(consumed as usize, 1);
                                processed += step;
                                cur = cur.wrapping_add(step as u16);
                            }
                            viewer.update_hex_listing(out);
                        }
                        regview::Command::SetPc(a) => {
                            cpu.set_pc(a);
                            
                            let snap2 = regview::RegSnapshot {
                                a: cpu.r.a,
                                b: cpu.r.b,
                                d: ((cpu.r.a as u16) << 8) | (cpu.r.b as u16),
                                x: cpu.r.x,
                                y: cpu.r.y,
                                u: cpu.r.u,
                                s: cpu.r.s,
                                dp: cpu.r.dp,
                                pc: cpu.r.pc,
                                cc: cpu.r.cc,
                            };
                            viewer.push(snap2);
                            let (mstart2, mlen2, mfollow2) = viewer.get_memory_window();
                            let eff_start2: u16 =
                                if mfollow2 { cpu.r.pc & 0xFFF0 } else { mstart2 };
                            let mut buf2: Vec<u8> = Vec::with_capacity(mlen2);
                            let mut a2 = eff_start2;
                            for _ in 0..mlen2 {
                                buf2.push(bus.as_mut().read8(a2));
                                a2 = a2.wrapping_add(1);
                            }
                            viewer.update_memory_window(eff_start2, buf2);
                            let (consumed, _) =
                                em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc);
                            let clen2 = core::cmp::max(consumed as usize, 1);
                            viewer.set_memory_highlight(cpu.r.pc, clen2);
                        }
                        regview::Command::RefreshHexListing => {
                            let spans = coalesce_ranges(loaded.loaded_ranges.clone());
                            let (policy, _win_len) = viewer.get_span_policy();
                            let span_for_pc =
                                spans.iter().find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e).copied();
                            let (start, total_len) = if policy == 1 {
                                let (ws, wl, _) = viewer.get_memory_window();
                                (ws, std::cmp::max(wl, 256))
                            } else if let Some((s, e)) = span_for_pc {
                                (s, e.wrapping_sub(s) as usize)
                            } else {
                                let (ws, wl, _) = viewer.get_memory_window();
                                (ws, std::cmp::max(wl, 256))
                            };
                            let mut cur = start;
                            let mut out = String::new();
                            let mut processed: usize = 0;
                            while processed < total_len {
                                let (consumed, text) = em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                use core::fmt::Write as _;
                                let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                let step = core::cmp::max(consumed as usize, 1);
                                processed += step;
                                cur = cur.wrapping_add(step as u16);
                            }
                            viewer.update_hex_listing(out);
                        }
                        _ => {}
                    }
                }
                // Update memory pane based on current settings
                let (mut mstart, mut mlen, mut mfollow) = viewer.get_memory_window();
                let mut eff_start: u16 = if mfollow { cpu.r.pc & 0xFFF0 } else { mstart };
                let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                {
                    let mut a = eff_start;
                    for _ in 0..mlen {
                        buf.push(bus.as_mut().read8(a));
                        a = a.wrapping_add(1);
                    }
                }
                viewer.update_memory_window(eff_start, buf);
                let (consumed, _) = em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc);
                let mut clen = core::cmp::max(consumed as usize, 1);
                viewer.set_memory_highlight(cpu.r.pc, clen);
                // While paused, also process GUI commands so right-click actions take effect immediately
                while viewer.should_pause() {
                    // Drain settings worker queue while paused so Apply/SavePrefs still apply.
                    process_settings_worker_queue(reg_viewer.as_ref(), bus.as_mut());
                    if STOP_REQUESTED.load(Ordering::SeqCst) {
                        break 'run_loop;
                    }
                    if viewer.is_closed() {
                        break 'run_loop;
                    }
                    // While paused, detect Memory pane setting changes (Follow PC / Start / Len)
                    {
                        let (ns, nl, nf) = viewer.get_memory_window();
                        if ns != mstart || nl != mlen || nf != mfollow {
                            mstart = ns;
                            mlen = nl;
                            mfollow = nf;
                            eff_start = if mfollow { cpu.r.pc & 0xFFF0 } else { mstart };
                            let mut nbuf: Vec<u8> = Vec::with_capacity(mlen);
                            let mut a = eff_start;
                            for _ in 0..mlen {
                                nbuf.push(bus.as_mut().read8(a));
                                a = a.wrapping_add(1);
                            }
                            viewer.update_memory_window(eff_start, nbuf);
                            let (consumed, _) =
                                em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc);
                            clen = core::cmp::max(consumed as usize, 1);
                            viewer.set_memory_highlight(cpu.r.pc, clen);
                        }
                    }
                    // Handle queued commands even while paused
                    while let Some(cmd) = viewer.take_command_worker() {
                        match cmd {
                            regview::Command::UiOpenSettings
                            | regview::Command::UiOpenSettingsExternal => {}
                            regview::Command::UiSetSettingsBaseline(_, _, _) => {}
                            regview::Command::UiAckSettings { .. } => {}
                            regview::Command::UiSetGpioOnColor(_) => {}
                            regview::Command::UiSetBlockStatus { .. } => {}
                            regview::Command::SavePrefs {
                                settings,
                                gpio_on_color,
                                instance_id,
                            } => {
                                let payload = settings_worker::SettingsPayload {
                                    settings,
                                    gpio_on_color,
                                    instance_id,
                                };
                                if apply_settings_runtime(Some(viewer), bus.as_mut(), &payload) {
                                    persist_settings_snapshot(
                                        &payload.settings,
                                        payload.gpio_on_color,
                                    );
                                    publish_settings_baseline(
                                        &payload.settings,
                                        payload.gpio_on_color,
                                    );
                                    send_settings_ack(payload.instance_id);
                                }
                            }
                            regview::Command::ApplyRegs(ns) => {
                                cpu.r.a = ns.a;
                                cpu.r.b = ns.b;
                                cpu.r.x = ns.x;
                                cpu.r.y = ns.y;
                                cpu.r.u = ns.u;
                                cpu.r.s = ns.s;
                                cpu.r.dp = ns.dp;
                                cpu.r.pc = ns.pc;
                                cpu.r.cc = ns.cc;
                                let snap2 = regview::RegSnapshot {
                                    a: cpu.r.a,
                                    b: cpu.r.b,
                                    d: ((cpu.r.a as u16) << 8) | (cpu.r.b as u16),
                                    x: cpu.r.x,
                                    y: cpu.r.y,
                                    u: cpu.r.u,
                                    s: cpu.r.s,
                                    dp: cpu.r.dp,
                                    pc: cpu.r.pc,
                                    cc: cpu.r.cc,
                                };
                                viewer.push(snap2);
                                // Refresh memory pane after register change (e.g., PC)
                                let (mstart2, mlen2, mfollow2) = viewer.get_memory_window();
                                let eff_start2: u16 =
                                    if mfollow2 { cpu.r.pc & 0xFFF0 } else { mstart2 };
                                let mut buf2: Vec<u8> = Vec::with_capacity(mlen2);
                                let mut a2 = eff_start2;
                                for _ in 0..mlen2 {
                                    buf2.push(bus.as_mut().read8(a2));
                                    a2 = a2.wrapping_add(1);
                                }
                                viewer.update_memory_window(eff_start2, buf2);
                                let (consumed, _) =
                                    em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc);
                                let clen2 = core::cmp::max(consumed as usize, 1);
                                viewer.set_memory_highlight(cpu.r.pc, clen2);
                            }
                        regview::Command::MemWrite(items) => {
                            for (addr, val) in items {
                                bus.as_mut().write8(addr, val);
                            }
                            // Refresh memory window after edits
                            let (mstart2, mlen2, mfollow2) = viewer.get_memory_window();
                            let eff_start2: u16 =
                                if mfollow2 { cpu.r.pc & 0xFFF0 } else { mstart2 };
                            let mut buf2: Vec<u8> = Vec::with_capacity(mlen2);
                            let mut a2 = eff_start2;
                            for _ in 0..mlen2 {
                                buf2.push(bus.as_mut().read8(a2));
                                a2 = a2.wrapping_add(1);
                            }
                            viewer.update_memory_window(eff_start2, buf2);
                            // Refresh listing for current PC span
                            let spans = coalesce_ranges(loaded.loaded_ranges.clone());
                            let span_for_pc =
                                spans.iter().find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e).copied();
                            let (start, end) = span_for_pc.unwrap_or_else(|| {
                                let (ws, wl, _) = viewer.get_memory_window();
                                (ws, ws.wrapping_add(wl as u16))
                            });
                            let mut cur = start;
                            let mut out = String::new();
                            let mut processed: usize = 0;
                            let total_len: usize = end.wrapping_sub(start) as usize;
                            while processed < total_len {
                                let (consumed, text) =
                                    em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                use core::fmt::Write as _;
                                let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                let step = core::cmp::max(consumed as usize, 1);
                                processed += step;
                                cur = cur.wrapping_add(step as u16);
                            }
                            viewer.update_hex_listing(out);
                        }
                            regview::Command::SetPc(a) => {
                                cpu.set_pc(a);
                                viewer.set_memory_window(cpu.r.pc, None, Some(true));
                                // push updated snapshot
                                let snap2 = regview::RegSnapshot {
                                    a: cpu.r.a,
                                    b: cpu.r.b,
                                    d: ((cpu.r.a as u16) << 8) | (cpu.r.b as u16),
                                    x: cpu.r.x,
                                    y: cpu.r.y,
                                    u: cpu.r.u,
                                    s: cpu.r.s,
                                    dp: cpu.r.dp,
                                    pc: cpu.r.pc,
                                    cc: cpu.r.cc,
                                };
                                viewer.push(snap2);
                                // refresh memory view to reflect new PC
                                let (mstart2, mlen2, mfollow2) = viewer.get_memory_window();
                                let eff_start2: u16 =
                                    if mfollow2 { cpu.r.pc & 0xFFF0 } else { mstart2 };
                                let mut buf2: Vec<u8> = Vec::with_capacity(mlen2);
                                let mut a2 = eff_start2;
                                for _ in 0..mlen2 {
                                    buf2.push(bus.as_mut().read8(a2));
                                    a2 = a2.wrapping_add(1);
                                }
                                viewer.update_memory_window(eff_start2, buf2);
                                let (consumed, _) =
                                    em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc);
                                let clen2 = core::cmp::max(consumed as usize, 1);
                                viewer.set_memory_highlight(cpu.r.pc, clen2);
                                // refresh listing for current PC span
                                let spans = coalesce_ranges(loaded.loaded_ranges.clone());
                                if let Some((start, end)) = spans
                                    .iter()
                                    .find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e)
                                    .copied()
                                {
                                    let mut cur = start;
                                    let mut out = String::new();
                                    let mut processed: usize = 0;
                                    let total_len: usize = end.wrapping_sub(start) as usize;
                                    while processed < total_len {
                                        let (consumed, text) =
                                            em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                        use core::fmt::Write as _;
                                        let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                        let step = core::cmp::max(consumed as usize, 1);
                                        processed += step;
                                        cur = cur.wrapping_add(step as u16);
                                    }
                                    viewer.update_hex_listing(out);
                                }
                            }
                            regview::Command::RefreshHexListing => {
                                // Rebuild hex listing for current PC's loaded span
                                let spans = coalesce_ranges(loaded.loaded_ranges.clone());
                                let (policy, _win_len) = viewer.get_span_policy();
                                let span_for_pc = spans
                                    .iter()
                                    .find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e)
                                    .copied();
                                let (start, total_len) = if policy == 1 {
                                    let (ws, wl, _) = viewer.get_memory_window();
                                    (ws, std::cmp::max(wl, 256))
                                } else if let Some((s, e)) = span_for_pc {
                                    (s, e.wrapping_sub(s) as usize)
                                } else {
                                    let (ws, wl, _) = viewer.get_memory_window();
                                    (ws, std::cmp::max(wl, 256))
                                };
                                let mut cur = start;
                                let mut out = String::new();
                                let mut processed: usize = 0;
                                while processed < total_len {
                                    let (consumed, text) =
                                        em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                    use core::fmt::Write as _;
                                    let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                    let step = core::cmp::max(consumed as usize, 1);
                                    processed += step;
                                    cur = cur.wrapping_add(step as u16);
                                }
                                viewer.update_hex_listing(out);
                            }
                            regview::Command::RunUntil(a) => {
                                run_until_target = Some(a);
                                viewer.set_paused(false);
                                // break pause loop to resume execution immediately
                                break;
                            }
                            regview::Command::ApplyMmu {
                                sys_mode,
                                task,
                                prot,
                                log_maps,
                                maps,
                                attrs,
                            } => {
                                // Apply to active MMU if present; use separate scopes to avoid overlapping borrows
                                {
                                    if let Some(io_mmu) =
                                        bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                    {
                                        let inner = io_mmu.inner_any_mut();
                                        if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                            if let Some(on) = log_maps {
                                                mmu.set_log_maps(on);
                                            }
                                            if let Some(t) = task {
                                                mmu.write8(mmu.regs_base + 0x10, t & 0x07);
                                            }
                                            if let Some(sys) = sys_mode {
                                                mmu.write8(
                                                    mmu.regs_base + 0x11,
                                                    if sys { 1 } else { 0 },
                                                );
                                            }
                                            if let Some(p) = prot {
                                                mmu.write8(mmu.regs_base + 0x12, p);
                                            }
                                            for (pg, fr) in &maps {
                                                mmu.set_map_entry(*pg, *fr);
                                            }
                                            for (pg, at) in &attrs {
                                                let off = 0x18u16 + (*pg as u16);
                                                mmu.write8(mmu.regs_base + off, *at);
                                            }
                                            continue;
                                        }
                                    }
                                }
                                {
                                    if let Some(mmu) =
                                        bus.as_mut().as_any_mut().downcast_mut::<Mc6829>()
                                    {
                                        if let Some(on) = log_maps {
                                            mmu.set_log_maps(on);
                                        }
                                        if let Some(t) = task {
                                            mmu.write8(mmu.regs_base + 0x10, t & 0x07);
                                        }
                                        if let Some(sys) = sys_mode {
                                            mmu.write8(
                                                mmu.regs_base + 0x11,
                                                if sys { 1 } else { 0 },
                                            );
                                        }
                                        if let Some(p) = prot {
                                            mmu.write8(mmu.regs_base + 0x12, p);
                                        }
                                        for (pg, fr) in maps {
                                            mmu.set_map_entry(pg, fr);
                                        }
                                        for (pg, at) in attrs {
                                            let off = 0x18u16 + (pg as u16);
                                            mmu.write8(mmu.regs_base + off, at);
                                        }
                                    }
                                }
                            }
                            regview::Command::MmuIdentityMap => {
                                {
                                    if let Some(io_mmu) =
                                        bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                    {
                                        let inner = io_mmu.inner_any_mut();
                                        if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                            mmu.identity_map_current();
                                            continue;
                                        }
                                    }
                                }
                                {
                                    if let Some(mmu) =
                                        bus.as_mut().as_any_mut().downcast_mut::<Mc6829>()
                                    {
                                        mmu.identity_map_current();
                                    }
                                }
                            }
                            regview::Command::OpenImage { path, base } => {
                                match std::fs::read(&path) {
                                    Ok(bytes) => {
                                        // Clear logical memory before loading new image
                                        for addr in 0u32..=0xFFFF {
                                            bus.as_mut().write8(addr as u16, 0);
                                        }
                                        let mut new_loaded = LoadedImage {
                                            loaded_ranges: Vec::new(),
                                            entry: None,
                                        };
                                        let lower = path
                                            .extension()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("")
                                            .to_lowercase();
                                        if lower == "s19" || lower == "srec" {
                                            let text = String::from_utf8_lossy(&bytes);
                                            match loader::load_srec_bus(bus.as_mut(), &text) {
                                                Ok(li) => new_loaded = li,
                                                Err(e) => {
                                                    eprintln!("Open failed (SREC): {e}");
                                                }
                                            }
                                        } else if lower == "bin" {
                                            let base_addr = base.unwrap_or(0);
                                            new_loaded = loader::load_binary_bus(
                                                bus.as_mut(),
                                                base_addr,
                                                &bytes,
                                            );
                                        } else {
                                            eprintln!("Unsupported extension for Open: {}", lower);
                                        }
                                        // PC init priority: reset vector if valid; else SREC entry; else first loaded range
                                        let mut new_pc: Option<u16> = None;
                                        // If reset vector points to a non-0x0000 address, use it
                                        let pch = bus.as_mut().read8(0xFFFE) as u16;
                                        let pcl = bus.as_mut().read8(0xFFFF) as u16;
                                        let vec_pc = (pch << 8) | pcl;
                                        if vec_pc != 0x0000 {
                                            new_pc = Some(vec_pc);
                                        }
                                        if new_pc.is_none() {
                                            new_pc = new_loaded.entry;
                                        }
                                        if new_pc.is_none() {
                                            if let Some((s, _e)) =
                                                new_loaded.loaded_ranges.first().copied()
                                            {
                                                new_pc = Some(s);
                                            }
                                        }
                                        if let Some(pc) = new_pc {
                                            cpu.r.pc = pc;
                                        }
                                        log_program_info(&new_loaded, cpu.r.pc);
                                        let snap = regview::RegSnapshot {
                                            a: cpu.r.a,
                                            b: cpu.r.b,
                                            d: ((cpu.r.a as u16) << 8) | (cpu.r.b as u16),
                                            x: cpu.r.x,
                                            y: cpu.r.y,
                                            u: cpu.r.u,
                                            s: cpu.r.s,
                                            dp: cpu.r.dp,
                                            pc: cpu.r.pc,
                                            cc: cpu.r.cc,
                                        };
                                        viewer.push(snap);
                                        // Update global loaded image so subsequent Hex rebuilds use the new spans
                                        loaded = new_loaded;
                                        viewer.force_hex_view();
                                        // Pause after load
                                        viewer.set_paused(true);
                                        // refresh memory window and listing for new content
                                        let (mstart, mlen, mfollow) = viewer.get_memory_window();
                                        let eff_start: u16 =
                                            if mfollow { cpu.r.pc & 0xFFF0 } else { mstart };
                                        if mfollow {
                                            viewer.set_memory_window_start(
                                                (cpu.r.pc & 0xFFF0) as usize,
                                            );
                                        }
                                        let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                                        let mut a = eff_start;
                                        for _ in 0..mlen {
                                            buf.push(bus.as_mut().read8(a));
                                            a = a.wrapping_add(1);
                                        }
                                        viewer.update_memory_window(eff_start, buf);
                                        let (consumed, _) =
                                            em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc);
                                        let clen = core::cmp::max(consumed as usize, 1);
                                        viewer.set_memory_highlight(cpu.r.pc, clen);
                                        // Update Hex view: always rebuild from current memory so Hex shows disassembly
                                        let spans = coalesce_ranges(loaded.loaded_ranges.clone());
                                        if let Some((start, end)) = spans
                                            .iter()
                                            .find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e)
                                            .copied()
                                        {
                                            let mut cur = start;
                                            let mut out = String::new();
                                            let mut processed: usize = 0;
                                            let total_len: usize = end.wrapping_sub(start) as usize;
                                            while processed < total_len {
                                                let (consumed, text) =
                                                    em6809::disasm::disasm_one_hex(
                                                        bus.as_mut(),
                                                        cur,
                                                    );
                                                use core::fmt::Write as _;
                                                let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                                let step = core::cmp::max(consumed as usize, 1);
                                                processed += step;
                                                cur = cur.wrapping_add(step as u16);
                                            }
                                            viewer.update_hex_listing(out);
                                        }
                                        sync_gpio_from_bus(bus.as_mut(), viewer);
                                        sync_block_from_bus(bus.as_mut(), viewer);
                                        request_reapply_settings_from_baseline();
                                        // If an adjacent .lst exists, request GUI to map and show it; else prefer Hex
                                        let lst_path = path.with_extension("lst");
                                        let lst_path_upper = path.with_extension("LST");
                                        if lst_path.exists() || lst_path_upper.exists() {
                                            let p = if lst_path.exists() {
                                                lst_path
                                            } else {
                                                lst_path_upper
                                            };
                                            viewer.request_load_lst(p);
                                        } else {
                                            viewer.force_hex_view();
                                        }
                                    }
                                    Err(e) => eprintln!("Open failed: {e}"),
                                }
                            }
                            regview::Command::UiPromptBinBase(_) => {
                                // UI-only; handled on GUI thread to show BIN base modal
                            }
                            regview::Command::UiLoadLst(_) => {
                                // UI-only; handled by GUI thread
                            }
                            regview::Command::ApplyMmuPreset { name } => {
                                let apply = |mmu: &mut Mc6829, viewer: Option<&regview::Viewer>| {
                                    let _ = em6809::config::apply_preset(mmu, &name);
                                    let (map, sys, task) = mmu.snapshot_current_map();
                                    if let Some(v) = viewer {
                                        v.sync_mmu(sys, task, map);
                                    }
                                    if let Some((mut s, _origins, color_opt)) =
                                        regview::get_latest_settings_baseline()
                                    {
                                        s.use_mmu = true;
                                        s.mmu_mode_sys = sys;
                                        s.mmu_task = format!("{}", task & 0x07);
                                        s.mmu_preset_name = name.clone();
                                        let prot = mmu.read8(mmu.regs_base + 0x12);
                                        s.mmu_prot_hex = format!("{:02X}", prot);
                                        for (i, f) in map.iter().enumerate() {
                                            s.mmu_map_frames[i] = format!("{:02X}", f & 0x00FF);
                                        }
                                        for i in 0..16 {
                                            let attr = mmu.read8(mmu.regs_base + 0x18 + i as u16);
                                            s.mmu_attr_bytes[i] = format!("{:02X}", attr);
                                        }
                                        publish_settings_baseline(&s, color_opt.unwrap_or(0));
                                    }
                                };
                                if let Some(io_mmu) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                {
                                    let inner = io_mmu.inner_any_mut();
                                    if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                        apply(mmu, Some(viewer));
                                    }
                                } else if let Some(mmu) =
                                    bus.as_mut().as_any_mut().downcast_mut::<Mc6829>()
                                {
                                    apply(mmu, Some(viewer));
                                }
                            }
                            regview::Command::ApplyMmuMapFileText { text } => {
                                let apply_text =
                                    |mmu: &mut Mc6829, viewer: Option<&regview::Viewer>| {
                                        let _ =
                                            em6809::config::apply_mmu_config_from_str(mmu, &text);
                                        let (map, sys, task) = mmu.snapshot_current_map();
                                        if let Some(v) = viewer {
                                            v.sync_mmu(sys, task, map);
                                        }
                                        if let Some((mut s, _origins, color_opt)) =
                                            regview::get_latest_settings_baseline()
                                        {
                                            s.use_mmu = true;
                                            s.mmu_mode_sys = sys;
                                            s.mmu_task = format!("{}", task & 0x07);
                                            s.mmu_preset_name.clear();
                                            let prot = mmu.read8(mmu.regs_base + 0x12);
                                            s.mmu_prot_hex = format!("{:02X}", prot);
                                            for (i, f) in map.iter().enumerate() {
                                                s.mmu_map_frames[i] = format!("{:02X}", f & 0x00FF);
                                            }
                                            for i in 0..16 {
                                                let attr =
                                                    mmu.read8(mmu.regs_base + 0x18 + i as u16);
                                                s.mmu_attr_bytes[i] = format!("{:02X}", attr);
                                            }
                                            publish_settings_baseline(&s, color_opt.unwrap_or(0));
                                        }
                                    };
                                if let Some(io_mmu) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                {
                                    let inner = io_mmu.inner_any_mut();
                                    if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                        apply_text(mmu, Some(viewer));
                                    }
                                } else if let Some(mmu) =
                                    bus.as_mut().as_any_mut().downcast_mut::<Mc6829>()
                                {
                                    apply_text(mmu, Some(viewer));
                                }
                            }
                            regview::Command::ApplyConsole {
                                enable,
                                base,
                                out_path,
                                stdin: _stdin,
                                echo,
                                flush_every,
                                flush_newline: _,
                                rx_watermark: _,
                                irq_hold: _,
                                tee_stderr: _,
                                ..
                            } => {
                                if let Some(iobus) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                                {
                                    let b = base.unwrap_or(0xFF00);
                                    iobus.ensure_console(enable, b);
                                    iobus.with_console_mut_opt(|c| {
                                        if let Some(p) = &out_path {
                                            c.set_out_file(p);
                                        }
                                        c.set_local_echo(echo);
                                        if let Some(n) = flush_every {
                                            c.set_flush_every(n);
                                        }
                                    });
                                } else if let Some(iobus) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                {
                                    let b = base.unwrap_or(0xFF00);
                                    iobus.ensure_console(enable, b);
                                    iobus.with_console_mut_opt(|c| {
                                        if let Some(p) = &out_path {
                                            c.set_out_file(p);
                                        }
                                        c.set_local_echo(echo);
                                        if let Some(n) = flush_every {
                                            c.set_flush_every(n);
                                        }
                                    });
                                }
                            }
                            regview::Command::ApplyBlock {
                                enable,
                                base,
                                file_path,
                                ..
                            } => {
                                if let Some(iobus) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                                {
                                    let b = base.unwrap_or(0xFF20);
                                    iobus.ensure_block(enable, b);
                                    if let Some(p) = file_path {
                                        iobus.with_block_mut_opt(|blk| blk.set_backing_file(&p));
                                    }
                                } else if let Some(iobus) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                {
                                    let b = base.unwrap_or(0xFF20);
                                    iobus.ensure_block(enable, b);
                                    if let Some(p) = file_path {
                                        iobus.with_block_mut_opt(|blk| blk.set_backing_file(&p));
                                    }
                                }
                                sync_block_from_bus(bus.as_mut(), viewer);
                            }
                            regview::Command::ApplyGpio { enable, base, bits } => {
                                let b = base.unwrap_or(0xFF40);
                                let k = bits.unwrap_or(8);
                                if let Some(iobus) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                                {
                                    iobus.ensure_gpio(enable, b, k);
                                    if enable {
                                        iobus.with_gpio_mut_opt(|g| {
                                            let (out, dir, bits) = g.get_state();
                                            viewer.sync_gpio(true, bits, out, dir);
                                        });
                                    } else {
                                        viewer.sync_gpio(false, 0, 0, 0);
                                    }
                                } else if let Some(iobus) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                {
                                    iobus.ensure_gpio(enable, b, k);
                                    if enable {
                                        iobus.with_gpio_mut_opt(|g| {
                                            let (out, dir, bits) = g.get_state();
                                            viewer.sync_gpio(true, bits, out, dir);
                                        });
                                    } else {
                                        viewer.sync_gpio(false, 0, 0, 0);
                                    }
                                }
                            }
                            regview::Command::ApplyTimer {
                                enable,
                                base,
                                reload,
                                ..
                            } => {
                                let b = base.unwrap_or(0xFF10);
                                if let Some(iobus) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                                {
                                    iobus.ensure_timer(enable, b);
                                    if let Some(r) = reload {
                                        iobus.with_timer_mut_opt(|t| t.set_reload(r));
                                    }
                                    if enable {
                                        iobus.with_timer_mut_opt(|t| {
                                            let (run, _irq_en, _firq, pending) = t.get_state();
                                            viewer.sync_timer(true, pending, run);
                                        });
                                    } else {
                                        viewer.sync_timer(false, false, false);
                                    }
                                } else if let Some(iobus) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                {
                                    iobus.ensure_timer(enable, b);
                                    if let Some(r) = reload {
                                        iobus.with_timer_mut_opt(|t| t.set_reload(r));
                                    }
                                    if enable {
                                        iobus.with_timer_mut_opt(|t| {
                                            let (run, _irq_en, _firq, pending) = t.get_state();
                                            viewer.sync_timer(true, pending, run);
                                        });
                                    } else {
                                        viewer.sync_timer(false, false, false);
                                    }
                                }
                            }
                            regview::Command::GpioSetOut(mask) => {
                                if let Some(io_mem) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                                {
                                    io_mem.with_gpio_mut(|g| g.set_out(mask));
                                } else if let Some(io_mmu) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                {
                                    io_mmu.with_gpio_mut(|g| g.set_out(mask));
                                } else if let Some(wt) =
                                    bus.as_mut().as_any_mut().downcast_mut::<bus::WriteTrack>()
                                {
                                    let inner_any = wt.inner_any_mut();
                                    if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>()
                                    {
                                        io_mem.with_gpio_mut(|g| g.set_out(mask));
                                    } else if let Some(io_mmu) =
                                        inner_any.downcast_mut::<IoBus<Mc6829>>()
                                    {
                                        io_mmu.with_gpio_mut(|g| g.set_out(mask));
                                    }
                                }
                            }
                            regview::Command::UiSetGpio(_, _, _, _) => {}
                            regview::Command::UiSetMmu(_, _, _) => {}
                            regview::Command::UiSetTimer(..) => {}
                        }
                    }
                    // While paused, refresh memory pane based on current controls
                    {
                        let (mstart, mlen, mfollow) = viewer.get_memory_window();
                        let eff_start: u16 = if mfollow { cpu.r.pc & 0xFFF0 } else { mstart };
                        let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                        let mut a = eff_start;
                        for _ in 0..mlen {
                            buf.push(bus.as_mut().read8(a));
                            a = a.wrapping_add(1);
                        }
                        viewer.update_memory_window(eff_start, buf);
                        let (consumed, _) = em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc);
                        let clen = core::cmp::max(consumed as usize, 1);
                        viewer.set_memory_highlight(cpu.r.pc, clen);
                    }
                    // Small sleep to avoid busy-waiting when still paused
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            if let Some(ref rxch) = rx {
                if let Ok(v) = rxch.try_recv() {
                    // Support possibly wrapped bus (WriteTrack)
                    if let Some(io_mem) = bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                    {
                        io_mem.feed_console_input(&v);
                    } else if let Some(io_mmu) =
                        bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                    {
                        io_mmu.feed_console_input(&v);
                    } else if let Some(wt) =
                        bus.as_mut().as_any_mut().downcast_mut::<bus::WriteTrack>()
                    {
                        let inner_any = wt.inner_any_mut();
                        if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
                            io_mem.feed_console_input(&v);
                        } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
                            io_mmu.feed_console_input(&v);
                        }
                    }
                }
            }
            if let Some(ref mut seq) = boot_seq {
                seq.on_pre_step(bus.as_mut(), mmu_regs_base, cpu.r.pc, &mut cpu);
            }
            // trace printing removed in GUI-only mode
            cpu.step(bus.as_mut(), false);
            // Dirty-driven memory window refresh while running (main thread path)
            if let Some(viewer) = &reg_viewer {
                let (mstart, mlen, mfollow) = viewer.get_memory_window();
                let eff_start: u16 = if mfollow { cpu.r.pc & 0xFFF0 } else { mstart };
                if let Some(io_mem) = bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>() {
                    io_mem.set_mem_watch(eff_start, mlen);
                    if io_mem.take_mem_dirty() {
                        let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                        let mut a = eff_start;
                        for _ in 0..mlen {
                            buf.push(bus.as_mut().read8(a));
                            a = a.wrapping_add(1);
                        }
                        viewer.update_memory_window(eff_start, buf);
                    }
                } else if let Some(io_mmu) =
                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                {
                    io_mmu.set_mem_watch(eff_start, mlen);
                    if io_mmu.take_mem_dirty() {
                        let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                        let mut a = eff_start;
                        for _ in 0..mlen {
                            buf.push(bus.as_mut().read8(a));
                            a = a.wrapping_add(1);
                        }
                        viewer.update_memory_window(eff_start, buf);
                    }
                } else if let Some(wt) = bus.as_mut().as_any_mut().downcast_mut::<bus::WriteTrack>()
                {
                    let inner_any = wt.inner_any_mut();
                    if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
                        io_mem.set_mem_watch(eff_start, mlen);
                        if io_mem.take_mem_dirty() {
                            let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                            let mut a = eff_start;
                            for _ in 0..mlen {
                                buf.push(bus.as_mut().read8(a));
                                a = a.wrapping_add(1);
                            }
                            viewer.update_memory_window(eff_start, buf);
                        }
                    } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
                        io_mmu.set_mem_watch(eff_start, mlen);
                        if io_mmu.take_mem_dirty() {
                            let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                            let mut a = eff_start;
                            for _ in 0..mlen {
                                buf.push(bus.as_mut().read8(a));
                                a = a.wrapping_add(1);
                            }
                            viewer.update_memory_window(eff_start, buf);
                        }
                    }
                }
            }

            // Update register viewer after executing instruction
            if let Some(viewer) = &reg_viewer {
                let snap = regview::RegSnapshot {
                    a: cpu.r.a,
                    b: cpu.r.b,
                    d: ((cpu.r.a as u16) << 8) | (cpu.r.b as u16),
                    x: cpu.r.x,
                    y: cpu.r.y,
                    u: cpu.r.u,
                    s: cpu.r.s,
                    dp: cpu.r.dp,
                    pc: cpu.r.pc,
                    cc: cpu.r.cc,
                };
                viewer.push(snap);
                // Update memory pane after executing instruction
                {
                    let (mstart, mlen, mfollow) = viewer.get_memory_window();
                    let eff_start: u16 = if mfollow { cpu.r.pc & 0xFFF0 } else { mstart };
                    let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                    let mut a = eff_start;
                    for _ in 0..mlen {
                        buf.push(bus.as_mut().read8(a));
                        a = a.wrapping_add(1);
                    }
                    viewer.update_memory_window(eff_start, buf);
                    let (consumed, _) = em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc);
                    let clen = core::cmp::max(consumed as usize, 1);
                    viewer.set_memory_highlight(cpu.r.pc, clen);
                }
                // Handle viewer commands (context menu actions)
                while let Some(cmd) = viewer.take_command() {
                    match cmd {
                        regview::Command::UiOpenSettings
                        | regview::Command::UiOpenSettingsExternal => {}
                        regview::Command::UiSetSettingsBaseline(_, _, _) => {}
                        regview::Command::UiAckSettings { .. } => {}
                        regview::Command::UiSetBlockStatus { .. } => {}
                        regview::Command::SavePrefs {
                            settings,
                            gpio_on_color,
                            instance_id,
                        } => {
                            let payload = settings_worker::SettingsPayload {
                                settings,
                                gpio_on_color,
                                instance_id,
                            };
                            if apply_settings_runtime(Some(viewer), bus.as_mut(), &payload) {
                                persist_settings_snapshot(&payload.settings, payload.gpio_on_color);
                                publish_settings_baseline(&payload.settings, payload.gpio_on_color);
                                send_settings_ack(payload.instance_id);
                            }
                        }
                        regview::Command::SetPc(a) => {
                            cpu.set_pc(a);
                            viewer.set_memory_window(cpu.r.pc, None, Some(true));
                            // push updated snapshot immediately to reflect PC change
                            let snap2 = regview::RegSnapshot {
                                a: cpu.r.a,
                                b: cpu.r.b,
                                d: ((cpu.r.a as u16) << 8) | (cpu.r.b as u16),
                                x: cpu.r.x,
                                y: cpu.r.y,
                                u: cpu.r.u,
                                s: cpu.r.s,
                                dp: cpu.r.dp,
                                pc: cpu.r.pc,
                                cc: cpu.r.cc,
                            };
                            viewer.push(snap2);
                            // Also refresh listing pane to reflect current PC span and highlight
                            let spans = coalesce_ranges(loaded.loaded_ranges.clone());
                            let span_for_pc = spans
                                .iter()
                                .find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e)
                                .copied();
                            if let Some((start, end)) = span_for_pc {
                                let mut cur = start;
                                let mut out = String::new();
                                let mut processed: usize = 0;
                                let total_len: usize = end.wrapping_sub(start) as usize;
                                while processed < total_len {
                                    let (consumed, text) =
                                        em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                    use core::fmt::Write as _;
                                    let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                    let step = core::cmp::max(consumed as usize, 1);
                                    processed += step;
                                    cur = cur.wrapping_add(step as u16);
                                }
                                viewer.update_hex_listing(out);
                            }
                        }
                        regview::Command::UiPromptBinBase(_) => {
                            // UI-only; handled by GUI thread
                        }
                        regview::Command::UiLoadLst(_) => {
                            // UI-only; handled by GUI thread
                        }
                        regview::Command::RefreshHexListing => {
                            // Rebuild hex listing for current PC's loaded span
                            let spans = coalesce_ranges(loaded.loaded_ranges.clone());
                            let (policy, _win_len) = viewer.get_span_policy();
                            let span_for_pc = spans
                                .iter()
                                .find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e)
                                .copied();
                            let (start, total_len) = if policy == 1 {
                                let (ws, wl, _) = viewer.get_memory_window();
                                (ws, std::cmp::max(wl, 256))
                            } else if let Some((s, e)) = span_for_pc {
                                (s, e.wrapping_sub(s) as usize)
                            } else {
                                let (ws, wl, _) = viewer.get_memory_window();
                                (ws, std::cmp::max(wl, 256))
                            };
                            let mut cur = start;
                            let mut out = String::new();
                            let mut processed: usize = 0;
                            while processed < total_len {
                                let (consumed, text) =
                                    em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                use core::fmt::Write as _;
                                let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                let step = core::cmp::max(consumed as usize, 1);
                                processed += step;
                                cur = cur.wrapping_add(step as u16);
                            }
                            viewer.update_hex_listing(out);
                        }
                        regview::Command::RunUntil(a) => {
                            run_until_target = Some(a);
                            viewer.set_paused(false);
                            // Proactively refresh listing so panes reflect latest selection
                            let spans = coalesce_ranges(loaded.loaded_ranges.clone());
                            let span_for_pc = spans
                                .iter()
                                .find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e)
                                .copied();
                            if let Some((start, end)) = span_for_pc {
                                let mut cur = start;
                                let mut out = String::new();
                                let mut processed: usize = 0;
                                let total_len: usize = end.wrapping_sub(start) as usize;
                                while processed < total_len {
                                    let (consumed, text) =
                                        em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                    use core::fmt::Write as _;
                                    let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                    let step = core::cmp::max(consumed as usize, 1);
                                    processed += step;
                                    cur = cur.wrapping_add(step as u16);
                                }
                                viewer.update_hex_listing(out);
                            }
                        }
                        regview::Command::ApplyMmu {
                            sys_mode,
                            task,
                            prot,
                            log_maps,
                            maps,
                            attrs,
                        } => {
                            // Apply to active MMU on non-worker path; use scoped borrows
                            {
                                if let Some(io_mmu) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                {
                                    let inner = io_mmu.inner_any_mut();
                                    if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                        if let Some(on) = log_maps {
                                            mmu.set_log_maps(on);
                                        }
                                        if let Some(t) = task {
                                            mmu.write8(mmu.regs_base + 0x10, t & 0x07);
                                        }
                                        if let Some(sys) = sys_mode {
                                            mmu.write8(
                                                mmu.regs_base + 0x11,
                                                if sys { 1 } else { 0 },
                                            );
                                        }
                                        if let Some(p) = prot {
                                            mmu.write8(mmu.regs_base + 0x12, p);
                                        }
                                        for (pg, fr) in &maps {
                                            mmu.set_map_entry(*pg, *fr);
                                        }
                                        for (pg, at) in &attrs {
                                            let off = 0x18u16 + (*pg as u16);
                                            mmu.write8(mmu.regs_base + off, *at);
                                        }
                                        // done
                                    } else {
                                        // fallthrough
                                    }
                                } else {
                                    // fallthrough
                                }
                            }
                            {
                                if let Some(mmu) =
                                    bus.as_mut().as_any_mut().downcast_mut::<Mc6829>()
                                {
                                    if let Some(on) = log_maps {
                                        mmu.set_log_maps(on);
                                    }
                                    if let Some(t) = task {
                                        mmu.write8(mmu.regs_base + 0x10, t & 0x07);
                                    }
                                    if let Some(sys) = sys_mode {
                                        mmu.write8(mmu.regs_base + 0x11, if sys { 1 } else { 0 });
                                    }
                                    if let Some(p) = prot {
                                        mmu.write8(mmu.regs_base + 0x12, p);
                                    }
                                    for (pg, fr) in maps {
                                        mmu.set_map_entry(pg, fr);
                                    }
                                    for (pg, at) in attrs {
                                        let off = 0x18u16 + (pg as u16);
                                        mmu.write8(mmu.regs_base + off, at);
                                    }
                                }
                            }
                        }
                        regview::Command::ApplyMmuPreset { name } => {
                            let apply = |mmu: &mut Mc6829, viewer: Option<&regview::Viewer>| {
                                let _ = em6809::config::apply_preset(mmu, &name);
                                let (map, sys, task) = mmu.snapshot_current_map();
                                if let Some(v) = viewer {
                                    v.sync_mmu(sys, task, map);
                                }
                                if let Some((mut s, _origins, color_opt)) =
                                    regview::get_latest_settings_baseline()
                                {
                                    s.use_mmu = true;
                                    s.mmu_mode_sys = sys;
                                    s.mmu_task = format!("{}", task & 0x07);
                                    s.mmu_preset_name = name.clone();
                                    let prot = mmu.read8(mmu.regs_base + 0x12);
                                    s.mmu_prot_hex = format!("{:02X}", prot);
                                    for (i, f) in map.iter().enumerate() {
                                        s.mmu_map_frames[i] = format!("{:02X}", f & 0x00FF);
                                    }
                                    for i in 0..16 {
                                        let attr = mmu.read8(mmu.regs_base + 0x18 + i as u16);
                                        s.mmu_attr_bytes[i] = format!("{:02X}", attr);
                                    }
                                    publish_settings_baseline(&s, color_opt.unwrap_or(0));
                                }
                            };
                            if let Some(io_mmu) =
                                bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                            {
                                let inner = io_mmu.inner_any_mut();
                                if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                    apply(mmu, Some(viewer));
                                }
                            } else if let Some(mmu) =
                                bus.as_mut().as_any_mut().downcast_mut::<Mc6829>()
                            {
                                apply(mmu, Some(viewer));
                            }
                        }
                        regview::Command::ApplyMmuMapFileText { text } => {
                            let apply_text =
                                |mmu: &mut Mc6829, viewer: Option<&regview::Viewer>| {
                                    let _ = em6809::config::apply_mmu_config_from_str(mmu, &text);
                                    let (map, sys, task) = mmu.snapshot_current_map();
                                    if let Some(v) = viewer {
                                        v.sync_mmu(sys, task, map);
                                    }
                                    if let Some((mut s, _origins, color_opt)) =
                                        regview::get_latest_settings_baseline()
                                    {
                                        s.use_mmu = true;
                                        s.mmu_mode_sys = sys;
                                        s.mmu_task = format!("{}", task & 0x07);
                                        s.mmu_preset_name.clear();
                                        let prot = mmu.read8(mmu.regs_base + 0x12);
                                        s.mmu_prot_hex = format!("{:02X}", prot);
                                        for (i, f) in map.iter().enumerate() {
                                            s.mmu_map_frames[i] = format!("{:02X}", f & 0x00FF);
                                        }
                                        for i in 0..16 {
                                            let attr = mmu.read8(mmu.regs_base + 0x18 + i as u16);
                                            s.mmu_attr_bytes[i] = format!("{:02X}", attr);
                                        }
                                        publish_settings_baseline(&s, color_opt.unwrap_or(0));
                                    }
                                };
                            if let Some(io_mmu) =
                                bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                            {
                                let inner = io_mmu.inner_any_mut();
                                if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                    apply_text(mmu, Some(viewer));
                                }
                            } else if let Some(mmu) =
                                bus.as_mut().as_any_mut().downcast_mut::<Mc6829>()
                            {
                                apply_text(mmu, Some(viewer));
                            }
                        }
                        regview::Command::MmuIdentityMap => {
                            {
                                if let Some(io_mmu) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                {
                                    let inner = io_mmu.inner_any_mut();
                                    if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                        mmu.identity_map_current();
                                    }
                                }
                            }
                            {
                                if let Some(mmu) =
                                    bus.as_mut().as_any_mut().downcast_mut::<Mc6829>()
                                {
                                    mmu.identity_map_current();
                                }
                            }
                        }
                        regview::Command::ApplyConsole {
                            enable,
                            base,
                            out_path,
                            stdin: _stdin,
                            echo,
                            flush_every,
                            flush_newline: _,
                            rx_watermark: _,
                            irq_hold: _,
                            tee_stderr: _,
                            ..
                        } => {
                            if let Some(iobus) =
                                bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                            {
                                let b = base.unwrap_or(0xFF00);
                                iobus.ensure_console(enable, b);
                                iobus.with_console_mut_opt(|c| {
                                    if let Some(p) = &out_path {
                                        c.set_out_file(p);
                                    }
                                    c.set_local_echo(echo);
                                    if let Some(n) = flush_every {
                                        c.set_flush_every(n);
                                    }
                                });
                            } else if let Some(iobus) =
                                bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                            {
                                let b = base.unwrap_or(0xFF00);
                                iobus.ensure_console(enable, b);
                                iobus.with_console_mut_opt(|c| {
                                    if let Some(p) = &out_path {
                                        c.set_out_file(p);
                                    }
                                    c.set_local_echo(echo);
                                    if let Some(n) = flush_every {
                                        c.set_flush_every(n);
                                    }
                                });
                            }
                        }
                        regview::Command::ApplyBlock {
                            enable,
                            base,
                            file_path,
                            ..
                        } => {
                            if let Some(iobus) =
                                bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                            {
                                let b = base.unwrap_or(0xFF20);
                                iobus.ensure_block(enable, b);
                                if let Some(p) = file_path {
                                    iobus.with_block_mut_opt(|blk| blk.set_backing_file(&p));
                                }
                            } else if let Some(iobus) =
                                bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                            {
                                let b = base.unwrap_or(0xFF20);
                                iobus.ensure_block(enable, b);
                                if let Some(p) = file_path {
                                    iobus.with_block_mut_opt(|blk| blk.set_backing_file(&p));
                                }
                            }
                            sync_block_from_bus(bus.as_mut(), viewer);
                        }
                        regview::Command::ApplyGpio { enable, base, bits } => {
                            let b = base.unwrap_or(0xFF40);
                            let k = bits.unwrap_or(8);
                            if let Some(iobus) =
                                bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                            {
                                iobus.ensure_gpio(enable, b, k);
                            } else if let Some(iobus) =
                                bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                            {
                                iobus.ensure_gpio(enable, b, k);
                            }
                        }
                        regview::Command::UiSetGpioOnColor(rgb) => {
                            viewer.set_gpio_on_color(rgb);
                        }
                        regview::Command::ApplyTimer {
                            enable,
                            base,
                            reload,
                            irq_enable,
                            firq,
                            start,
                        } => {
                            let b = base.unwrap_or(0xFF10);
                            if let Some(iobus) =
                                bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                            {
                                iobus.ensure_timer(enable, b);
                                if let Some(r) = reload {
                                    iobus.with_timer_mut_opt(|t| t.set_reload(r));
                                }
                                iobus.with_timer_mut_opt(|t| {
                                    t.set_irq_enable(irq_enable);
                                    t.set_firq(firq);
                                    if start {
                                        t.start();
                                    } else {
                                        t.stop();
                                    }
                                });
                            } else if let Some(iobus) =
                                bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                            {
                                iobus.ensure_timer(enable, b);
                                if let Some(r) = reload {
                                    iobus.with_timer_mut_opt(|t| t.set_reload(r));
                                }
                                iobus.with_timer_mut_opt(|t| {
                                    t.set_irq_enable(irq_enable);
                                    t.set_firq(firq);
                                    if start {
                                        t.start();
                                    } else {
                                        t.stop();
                                    }
                                });
                            }
                        }
                        regview::Command::ApplyRegs(ns) => {
                            cpu.r.a = ns.a;
                            cpu.r.b = ns.b;
                            cpu.r.x = ns.x;
                            cpu.r.y = ns.y;
                            cpu.r.u = ns.u;
                            cpu.r.s = ns.s;
                            cpu.r.dp = ns.dp;
                            cpu.r.pc = ns.pc;
                            cpu.r.cc = ns.cc;
                            let snap2 = regview::RegSnapshot {
                                a: cpu.r.a,
                                b: cpu.r.b,
                                d: ((cpu.r.a as u16) << 8) | (cpu.r.b as u16),
                                x: cpu.r.x,
                                y: cpu.r.y,
                                u: cpu.r.u,
                                s: cpu.r.s,
                                dp: cpu.r.dp,
                                pc: cpu.r.pc,
                                cc: cpu.r.cc,
                            };
                            viewer.push(snap2);
                            // refresh memory view to reflect edits
                            let (mstart, mlen, mfollow) = viewer.get_memory_window();
                            let eff_start: u16 = if mfollow { cpu.r.pc & 0xFFF0 } else { mstart };
                            let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                            let mut a = eff_start;
                            for _ in 0..mlen {
                                buf.push(bus.as_mut().read8(a));
                                a = a.wrapping_add(1);
                            }
                            viewer.update_memory_window(eff_start, buf);
                            let (consumed, _) =
                                em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc);
                            let clen = core::cmp::max(consumed as usize, 1);
                            viewer.set_memory_highlight(cpu.r.pc, clen);
                            // Also refresh listing pane to reflect current PC span and highlight
                            let spans = coalesce_ranges(loaded.loaded_ranges.clone());
                            let span_for_pc = spans
                                .iter()
                                .find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e)
                                .copied();
                            if let Some((start, end)) = span_for_pc {
                                let mut cur = start;
                                let mut out = String::new();
                                let mut processed: usize = 0;
                                let total_len: usize = end.wrapping_sub(start) as usize;
                                while processed < total_len {
                                    let (consumed, text) =
                                        em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                    use core::fmt::Write as _;
                                    let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                    let step = core::cmp::max(consumed as usize, 1);
                                    processed += step;
                                    cur = cur.wrapping_add(step as u16);
                                }
                                viewer.update_hex_listing(out);
                            }
                        }
                        regview::Command::MemWrite(items) => {
                            for (addr, val) in items {
                                bus.as_mut().write8(addr, val);
                            }
                            // Sync GPIO state immediately after edits
                            {
                                let mut pushed = false;
                                if let Some(io_mem) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                                {
                                    if io_mem.take_gpio_dirty() {
                                        io_mem.with_gpio_mut(|g| {
                                            let (o, d, b) = g.get_state();
                                            viewer.sync_gpio(true, b, o, d);
                                            pushed = true;
                                        });
                                    }
                                } else if let Some(io_mmu) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                {
                                    if io_mmu.take_gpio_dirty() {
                                        io_mmu.with_gpio_mut(|g| {
                                            let (o, d, b) = g.get_state();
                                            viewer.sync_gpio(true, b, o, d);
                                            pushed = true;
                                        });
                                    }
                                } else if let Some(wt) =
                                    bus.as_mut().as_any_mut().downcast_mut::<bus::WriteTrack>()
                                {
                                    let inner_any = wt.inner_any_mut();
                                    if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>()
                                    {
                                        if io_mem.take_gpio_dirty() {
                                            io_mem.with_gpio_mut(|g| {
                                                let (o, d, b) = g.get_state();
                                                viewer.sync_gpio(true, b, o, d);
                                                pushed = true;
                                            });
                                        }
                                    } else if let Some(io_mmu) =
                                        inner_any.downcast_mut::<IoBus<Mc6829>>()
                                    {
                                        if io_mmu.take_gpio_dirty() {
                                            io_mmu.with_gpio_mut(|g| {
                                                let (o, d, b) = g.get_state();
                                                viewer.sync_gpio(true, b, o, d);
                                                pushed = true;
                                            });
                                        }
                                    }
                                }
                                let _ = pushed;
                            }
                            // Refresh memory pane after applying edits
                            let (mstart, mlen, mfollow) = viewer.get_memory_window();
                            let eff_start: u16 = if mfollow { cpu.r.pc & 0xFFF0 } else { mstart };
                            let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                            let mut a = eff_start;
                            for _ in 0..mlen {
                                buf.push(bus.as_mut().read8(a));
                                a = a.wrapping_add(1);
                            }
                            viewer.update_memory_window(eff_start, buf);
                            // Also refresh listing pane (full span) immediately
                            let spans = coalesce_ranges(loaded.loaded_ranges.clone());
                            let span_for_pc = spans
                                .iter()
                                .find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e)
                                .copied();
                            if let Some((start, end)) = span_for_pc {
                                let mut cur = start;
                                let mut out = String::new();
                                let mut processed: usize = 0;
                                let total_len: usize = end.wrapping_sub(start) as usize;
                                while processed < total_len {
                                    let (consumed, text) =
                                        em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                    use core::fmt::Write as _;
                                    let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                    let step = core::cmp::max(consumed as usize, 1);
                                    processed += step;
                                    cur = cur.wrapping_add(step as u16);
                                }
                                viewer.update_hex_listing(out);
                                if viewer.auto_hex_on_edit() {
                                    viewer.force_hex_view();
                                }
                                // While running (GUI main-thread path), push GPIO changes (poll every frame)
                                {
                                    if let Some(io_mem) =
                                        bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                                    {
                                        io_mem.with_gpio_mut(|g| {
                                            let (o, d, b) = g.get_state();
                                            viewer.sync_gpio(true, b, o, d);
                                        });
                                    } else if let Some(io_mmu) =
                                        bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                    {
                                        io_mmu.with_gpio_mut(|g| {
                                            let (o, d, b) = g.get_state();
                                            viewer.sync_gpio(true, b, o, d);
                                        });
                                    } else if let Some(wt) =
                                        bus.as_mut().as_any_mut().downcast_mut::<bus::WriteTrack>()
                                    {
                                        let inner_any = wt.inner_any_mut();
                                        if let Some(io_mem) =
                                            inner_any.downcast_mut::<IoBus<Memory>>()
                                        {
                                            io_mem.with_gpio_mut(|g| {
                                                let (o, d, b) = g.get_state();
                                                viewer.sync_gpio(true, b, o, d);
                                            });
                                        } else if let Some(io_mmu) =
                                            inner_any.downcast_mut::<IoBus<Mc6829>>()
                                        {
                                            io_mmu.with_gpio_mut(|g| {
                                                let (o, d, b) = g.get_state();
                                                viewer.sync_gpio(true, b, o, d);
                                            });
                                        }
                                    }
                                }
                                // Poll fallback to keep LEDs in sync
                                sync_gpio_from_bus(bus.as_mut(), viewer);
                                {
                                    let mut block_dirty = false;
                                    if let Some(io_mem) =
                                        bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                                    {
                                        if io_mem.take_block_dirty() {
                                            block_dirty = true;
                                        }
                                    } else if let Some(io_mmu) =
                                        bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                    {
                                        if io_mmu.take_block_dirty() {
                                            block_dirty = true;
                                        }
                                    } else if let Some(wt) =
                                        bus.as_mut().as_any_mut().downcast_mut::<bus::WriteTrack>()
                                    {
                                        let inner_any = wt.inner_any_mut();
                                        if let Some(io_mem) =
                                            inner_any.downcast_mut::<IoBus<Memory>>()
                                        {
                                            if io_mem.take_block_dirty() {
                                                block_dirty = true;
                                            }
                                        } else if let Some(io_mmu) =
                                            inner_any.downcast_mut::<IoBus<Mc6829>>()
                                        {
                                            if io_mmu.take_block_dirty() {
                                                block_dirty = true;
                                            }
                                        }
                                    }
                                    if block_dirty {
                                        sync_block_from_bus(bus.as_mut(), viewer);
                                    }
                                }
                            }
                        }
                        regview::Command::GpioSetOut(mask) => {
                            if let Some(io_mem) =
                                bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                            {
                                io_mem.with_gpio_mut(|g| g.set_out(mask));
                            } else if let Some(io_mmu) =
                                bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                            {
                                io_mmu.with_gpio_mut(|g| g.set_out(mask));
                            } else if let Some(wt) =
                                bus.as_mut().as_any_mut().downcast_mut::<bus::WriteTrack>()
                            {
                                let inner_any = wt.inner_any_mut();
                                if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
                                    io_mem.with_gpio_mut(|g| g.set_out(mask));
                                } else if let Some(io_mmu) =
                                    inner_any.downcast_mut::<IoBus<Mc6829>>()
                                {
                                    io_mmu.with_gpio_mut(|g| g.set_out(mask));
                                }
                            }
                        }
                        regview::Command::UiSetGpio(_, _, _, _) => {}
                        regview::Command::UiSetMmu(_, _, _) => {}
                        regview::Command::UiSetTimer(..) => {}
                        regview::Command::OpenImage { .. } => {}
                    }
                }
                // Event-driven recompute of HEX disassembly when tracked writes occurred
                if let Some(wt) = bus.as_mut().as_any_mut().downcast_mut::<bus::WriteTrack>() {
                    let (policy, win_len) = viewer.get_span_policy();
                    if policy == 0 {
                        // Full span tracking
                        let spans = coalesce_ranges(loaded.loaded_ranges.clone());
                        let span_for_pc = spans
                            .iter()
                            .find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e)
                            .copied();
                        let (start, end) = if let Some((s, e)) = span_for_pc {
                            (s, e)
                        } else if let Some((s, e)) = spans.first().copied() {
                            (s, e)
                        } else {
                            (cpu.r.pc, cpu.r.pc)
                        };
                        wt.set_span(if start != end {
                            Some((start, end))
                        } else {
                            None
                        });
                        let dirty = wt.take_dirty_addrs();
                        if !dirty.is_empty() && start != end {
                            let mut cur = start;
                            let mut out = String::new();
                            let mut processed: usize = 0;
                            let total_len: usize = end.wrapping_sub(start) as usize;
                            while processed < total_len {
                                let (consumed, text) =
                                    em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                use core::fmt::Write as _;
                                let _ = writeln!(&mut out, "{cur:04X}: {text}");
                                let step = core::cmp::max(consumed as usize, 1);
                                processed += step;
                                cur = cur.wrapping_add(step as u16);
                            }
                            viewer.update_hex_listing(out);
                            viewer.force_hex_view();
                        }
                    } else {
                        // PC window: track all writes, re-disassemble around changed addresses only
                        wt.set_span(None);
                        let mut dirty = wt.take_dirty_addrs();
                        if !dirty.is_empty() {
                            dirty.sort_unstable();
                            dirty.dedup();
                            let back = 8usize;
                            let half = win_len.saturating_sub(1) / 2;
                            let mut replacements: Vec<(u16, String)> = Vec::new();
                            for &addr in &dirty {
                                let start = addr.wrapping_sub((back as u16).max(half as u16));
                                let mut cur = start;
                                let mut processed: usize = 0;
                                let total_len: usize = win_len.max(32);
                                while processed < total_len {
                                    let (consumed, text) =
                                        em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                    let line = format!("{cur:04X}: {text}");
                                    replacements.push((cur, line));
                                    let step = core::cmp::max(consumed as usize, 1);
                                    processed += step;
                                    cur = cur.wrapping_add(step as u16);
                                }
                            }
                            if !replacements.is_empty() {
                                viewer.update_hex_lines(replacements);
                                viewer.force_hex_view();
                            }
                        }
                    }
                }
            }
            if let Some(ref mut seq) = boot_seq {
                seq.on_post_step(bus.as_mut(), mmu_regs_base);
            }
            // Auto-pause when run-until target reached
            if let Some(tgt) = run_until_target {
                if cpu.r.pc == tgt {
                    if let Some(viewer) = &reg_viewer {
                        viewer.set_paused(true);
                    }
                    run_until_target = None;
                }
            }
            // no step counter in GUI-only mode
            // no CLI breakpoints in GUI-only mode
        }
        // Restore console mode if we switched to raw
        if let Some(prev) = prev_mode {
            console_mode::restore_stdin_mode(prev);
        }
    }

    if print_cycles {
        println!("cycles: {}", cpu.cycles);
    }
}

fn request_reapply_settings_from_baseline() {
    if let Some((settings, _origins, gpio_color)) = regview::get_latest_settings_baseline() {
        let payload = settings_worker::SettingsPayload {
            settings,
            gpio_on_color: gpio_color.unwrap_or(0),
            instance_id: settings_worker::next_dialog_instance(),
        };
        settings_worker::push_worker_command(settings_worker::WorkerCommand::Apply(payload));
    }
}

fn process_settings_worker_queue(viewer: Option<&regview::Viewer>, bus: &mut dyn bus::Bus) {
    settings_worker::drain_worker_commands(|cmd| match cmd {
        settings_worker::WorkerCommand::Apply(payload) => {
            if apply_settings_runtime(viewer, bus, &payload) {
                publish_settings_baseline(&payload.settings, payload.gpio_on_color);
            }
            // Always acknowledge, even if runtime apply fails, to unblock UI.
            send_settings_ack(payload.instance_id);
        }
        settings_worker::WorkerCommand::SavePrefs(payload) => {
            if apply_settings_runtime(viewer, bus, &payload) {
                persist_settings_snapshot(&payload.settings, payload.gpio_on_color);
                publish_settings_baseline(&payload.settings, payload.gpio_on_color);
            }
            // Always acknowledge, even if runtime apply/persist fails, to unblock UI.
            send_settings_ack(payload.instance_id);
        }
        settings_worker::WorkerCommand::CloseNative {
            reason,
            instance_id,
        } => {
            if settings_worker::is_stale_instance(instance_id) {
                return;
            }
            settings_native::notify_native_close_from_worker(reason);
        }
        _ => {}
    });
}

fn apply_settings_runtime(
    viewer: Option<&regview::Viewer>,
    bus: &mut dyn bus::Bus,
    payload: &settings_worker::SettingsPayload,
) -> bool {
    let settings = &payload.settings;
    // Debug toggles (immediate)
    em6809::io::set_console_log(settings.debug_console_log);
    em6809::io::set_block_log(settings.debug_block_log);
    em6809::io::set_gpio_log(settings.debug_gpio_log);
    regview::gui::set_show_settings_origins(settings.debug_show_origins);
    settings_native::set_settings_debug_log(settings.debug_settings_log);
    // Console
    let console_base = parse_settings_u16(&settings.console_base_hex).unwrap_or(0xFF00);
    let console_out_path = if settings.console_out_path.trim().is_empty() {
        None
    } else {
        Some(settings.console_out_path.clone())
    };
    let console_flush_every = parse_settings_usize(&settings.console_flush_every);
    let configure_console = |iobus: &mut dyn core::any::Any| {
        if let Some(io_mem) = iobus.downcast_mut::<IoBus<Memory>>() {
            apply_console_config(
                io_mem,
                settings.use_console,
                console_base,
                console_out_path.as_deref(),
                settings.console_echo,
                console_flush_every,
            );
        } else if let Some(io_mmu) = iobus.downcast_mut::<IoBus<Mc6829>>() {
            apply_console_config(
                io_mmu,
                settings.use_console,
                console_base,
                console_out_path.as_deref(),
                settings.console_echo,
                console_flush_every,
            );
        }
    };
    if let Some(io_mem) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() {
        apply_console_config(
            io_mem,
            settings.use_console,
            console_base,
            console_out_path.as_deref(),
            settings.console_echo,
            console_flush_every,
        );
    } else if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        apply_console_config(
            io_mmu,
            settings.use_console,
            console_base,
            console_out_path.as_deref(),
            settings.console_echo,
            console_flush_every,
        );
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        configure_console(inner_any);
    }

    // Block device
    let block_base = parse_settings_u16(&settings.block_base_hex).unwrap_or(0xFF20);
    let block_path = if settings.block_file_path.trim().is_empty() {
        None
    } else {
        Some(settings.block_file_path.clone())
    };
    if let Some(io_mem) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() {
        apply_block_config(
            io_mem,
            settings.use_block,
            block_base,
            block_path.as_deref(),
        );
    } else if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        apply_block_config(
            io_mmu,
            settings.use_block,
            block_base,
            block_path.as_deref(),
        );
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
            apply_block_config(
                io_mem,
                settings.use_block,
                block_base,
                block_path.as_deref(),
            );
        } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            apply_block_config(
                io_mmu,
                settings.use_block,
                block_base,
                block_path.as_deref(),
            );
        }
    }
    if let Some(view) = viewer {
        sync_block_from_bus(bus, view);
    }

    // GPIO
    let gpio_base = parse_settings_u16(&settings.gpio_base_hex).unwrap_or(0xFF40);
    let gpio_bits = parse_settings_u8(&settings.gpio_bits).unwrap_or(8);
    if let Some(io_mem) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() {
        apply_gpio_config(io_mem, settings.use_gpio, gpio_base, gpio_bits);
    } else if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        apply_gpio_config(io_mmu, settings.use_gpio, gpio_base, gpio_bits);
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
            apply_gpio_config(io_mem, settings.use_gpio, gpio_base, gpio_bits);
        } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            apply_gpio_config(io_mmu, settings.use_gpio, gpio_base, gpio_bits);
        }
    }
    if let Some(view) = viewer {
        sync_gpio_from_bus(bus, view);
    }

    // Timer
    let timer_base = parse_settings_u16(&settings.timer_base_hex).unwrap_or(0xFF10);
    let timer_reload = parse_settings_u16(&settings.timer_reload_hex);
    if let Some(io_mem) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() {
        apply_timer_config(
            io_mem,
            settings.use_timer,
            timer_base,
            timer_reload,
            settings.timer_irq_enable,
            settings.timer_firq,
            settings.timer_start,
            viewer,
        );
    } else if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        apply_timer_config(
            io_mmu,
            settings.use_timer,
            timer_base,
            timer_reload,
            settings.timer_irq_enable,
            settings.timer_firq,
            settings.timer_start,
            viewer,
        );
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
            apply_timer_config(
                io_mem,
                settings.use_timer,
                timer_base,
                timer_reload,
                settings.timer_irq_enable,
                settings.timer_firq,
                settings.timer_start,
                viewer,
            );
        } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            apply_timer_config(
                io_mmu,
                settings.use_timer,
                timer_base,
                timer_reload,
                settings.timer_irq_enable,
                settings.timer_firq,
                settings.timer_start,
                viewer,
            );
        }
    }

    // MMU
    let mut maps: Vec<(usize, u16)> = Vec::new();
    for (idx, entry) in settings.mmu_map_frames.iter().enumerate() {
        if let Some(v) = parse_settings_u16(entry) {
            maps.push((idx, v));
        }
    }
    let mut attrs: Vec<(usize, u8)> = Vec::new();
    for (idx, entry) in settings.mmu_attr_bytes.iter().enumerate() {
        if let Some(v) = parse_settings_u16(entry) {
            attrs.push((idx, (v & 0xFF) as u8));
        }
    }
    let task_opt = parse_settings_usize(&settings.mmu_task).map(|v| (v as u8) & 0x07);
    let prot_opt = parse_settings_u16(&settings.mmu_prot_hex).map(|v| (v & 0xFF) as u8);
    set_mmu_log(bus, settings.debug_mmu_log);
    apply_mmu_config(
        bus,
        settings.mmu_mode_sys,
        task_opt,
        prot_opt,
        settings.mmu_log_maps,
        &maps,
        &attrs,
    );

    if let Some(view) = viewer {
        // Reflect current MMU map/state back to the viewer after applying changes.
        let push_map = |mmu: &mut Mc6829| {
            let (mut map, sys, task) = mmu.snapshot_current_map();
            // If settings provided explicit attr bytes, ensure they are merged into the snapshot.
            for (idx, entry) in settings.mmu_attr_bytes.iter().enumerate() {
                if let Some(v) = parse_settings_u16(entry) {
                    let attr = (v & 0xFF) as u16;
                    map[idx] = (map[idx] & 0x00FF) | (attr << 8);
                }
            }
            view.sync_mmu(sys, task, map);
        };
        if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
            let inner = io_mmu.inner_any_mut();
            if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                push_map(mmu);
            }
        } else if let Some(mmu) = bus.as_any_mut().downcast_mut::<Mc6829>() {
            push_map(mmu);
        } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
            let inner_any = wt.inner_any_mut();
            if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
                let inner2 = io_mmu.inner_any_mut();
                if let Some(mmu) = inner2.downcast_mut::<Mc6829>() {
                    push_map(mmu);
                }
            } else if let Some(mmu) = inner_any.downcast_mut::<Mc6829>() {
                push_map(mmu);
            }
        }
        view.set_gpio_on_color(payload.gpio_on_color);
    }
    true
}

fn apply_console_config<B: bus::Bus>(
    iobus: &mut IoBus<B>,
    enable: bool,
    base: u16,
    out_path: Option<&str>,
    echo: bool,
    flush_every: Option<usize>,
) {
    iobus.ensure_console(enable, base);
    iobus.with_console_mut_opt(|c| {
        if let Some(path) = out_path {
            c.set_out_file(path);
        }
        c.set_local_echo(echo);
        if let Some(n) = flush_every {
            c.set_flush_every(n);
        }
    });
}

fn apply_block_config<B: bus::Bus>(
    iobus: &mut IoBus<B>,
    enable: bool,
    base: u16,
    file_path: Option<&str>,
) {
    iobus.ensure_block(enable, base);
    if let Some(path) = file_path {
        iobus.with_block_mut_opt(|blk| blk.set_backing_file(path));
    }
}

fn apply_gpio_config<B: bus::Bus>(iobus: &mut IoBus<B>, enable: bool, base: u16, bits: u8) {
    iobus.ensure_gpio(enable, base, bits);
}

fn apply_timer_config<B: bus::Bus>(
    iobus: &mut IoBus<B>,
    enable: bool,
    base: u16,
    reload: Option<u16>,
    irq_enable: bool,
    firq: bool,
    start: bool,
    viewer: Option<&regview::Viewer>,
) {
    iobus.ensure_timer(enable, base);
    if let Some(r) = reload {
        iobus.with_timer_mut_opt(|t| t.set_reload(r));
    }
    iobus.with_timer_mut_opt(|t| {
        t.set_irq_enable(irq_enable);
        t.set_firq(firq);
        if start {
            t.start();
        } else {
            t.stop();
        }
    });
    if let Some(view) = viewer {
        if enable {
            iobus.with_timer_mut_opt(|t| {
                let (run, _irq_en, _firq, pending) = t.get_state();
                view.sync_timer(true, pending, run);
            });
        } else {
            view.sync_timer(false, false, false);
        }
    }
}

fn apply_mmu_config(
    bus: &mut dyn bus::Bus,
    sys_mode: bool,
    task: Option<u8>,
    prot: Option<u8>,
    log_maps: bool,
    maps: &[(usize, u16)],
    attrs: &[(usize, u8)],
) {
    let apply = |mmu: &mut Mc6829| {
        mmu.set_log_maps(log_maps);
        if let Some(t) = task {
            mmu.write8(mmu.regs_base + 0x10, t & 0x07);
        }
        mmu.write8(mmu.regs_base + 0x11, if sys_mode { 1 } else { 0 });
        if let Some(p) = prot {
            mmu.write8(mmu.regs_base + 0x12, p);
        }
        for (pg, fr) in maps {
            mmu.set_map_entry(*pg, *fr);
        }
        for (pg, at) in attrs {
            let off = 0x18u16 + (*pg as u16);
            mmu.write8(mmu.regs_base + off, *at);
        }
    };
    if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        let inner = io_mmu.inner_any_mut();
        if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
            apply(mmu);
            return;
        }
    } else if let Some(mmu) = bus.as_any_mut().downcast_mut::<Mc6829>() {
        apply(mmu);
        return;
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            let inner2 = io_mmu.inner_any_mut();
            if let Some(mmu) = inner2.downcast_mut::<Mc6829>() {
                apply(mmu);
                return;
            }
        } else if let Some(mmu) = inner_any.downcast_mut::<Mc6829>() {
            apply(mmu);
            return;
        }
    }
}

fn set_mmu_log(bus: &mut dyn bus::Bus, on: bool) {
    if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        let inner = io_mmu.inner_any_mut();
        if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
            mmu.set_log_maps(on);
            return;
        }
    } else if let Some(mmu) = bus.as_any_mut().downcast_mut::<Mc6829>() {
        mmu.set_log_maps(on);
        return;
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            let inner2 = io_mmu.inner_any_mut();
            if let Some(mmu) = inner2.downcast_mut::<Mc6829>() {
                mmu.set_log_maps(on);
                return;
            }
        } else if let Some(mmu) = inner_any.downcast_mut::<Mc6829>() {
            mmu.set_log_maps(on);
            return;
        }
    }
}

fn persist_settings_snapshot(state: &regview::SettingsState, gpio_on_color: u32) {
    let mut p = prefs::Prefs::default();
    p.general = None;
    p.console = Some(prefs::Console {
        enable: Some(state.use_console),
        base: parse_settings_u16(&state.console_base_hex),
        stdin: Some(state.console_stdin),
        echo: Some(state.console_echo),
        out_path: if state.console_out_path.is_empty() {
            None
        } else {
            Some(state.console_out_path.clone())
        },
        flush_every: parse_settings_usize(&state.console_flush_every),
        flush_newline: Some(state.console_flush_newline),
        rx_watermark: parse_settings_usize(&state.console_rx_wm),
        irq_hold: parse_settings_u32(&state.console_irq_hold),
        tee_stderr: Some(state.console_tee_stderr),
    });
    p.block = Some(prefs::Block {
        enable: Some(state.use_block),
        base: parse_settings_u16(&state.block_base_hex),
        file_path: if state.block_file_path.is_empty() {
            None
        } else {
            Some(state.block_file_path.clone())
        },
        irq_enable: Some(state.block_irq_enable),
        firq: Some(state.block_firq),
        irq_hold: parse_settings_u32(&state.block_irq_hold),
    });
    p.gpio = Some(prefs::Gpio {
        enable: Some(state.use_gpio),
        base: parse_settings_u16(&state.gpio_base_hex),
        bits: parse_settings_u8(&state.gpio_bits),
        on_color: Some(gpio_on_color),
    });
    p.timer = Some(prefs::Timer {
        enable: Some(state.use_timer),
        base: parse_settings_u16(&state.timer_base_hex),
        reload: parse_settings_u16(&state.timer_reload_hex),
        irq_enable: Some(state.timer_irq_enable),
        firq: Some(state.timer_firq),
        start: Some(state.timer_start),
        rate_hz: parse_settings_u32(&state.timer_rate_hz),
        ips_est: parse_settings_usize(&state.timer_ips).map(|v| v as u64),
    });
    let mut frames: Vec<(usize, u16)> = Vec::new();
    for (idx, entry) in state.mmu_map_frames.iter().enumerate() {
        if let Some(v) = parse_settings_u16(entry) {
            frames.push((idx, v & 0x00FF));
        }
    }
    let mut attrs: Vec<(usize, u8)> = Vec::new();
    for (idx, entry) in state.mmu_attr_bytes.iter().enumerate() {
        if let Some(v) = parse_settings_u16(entry) {
            attrs.push((idx, (v & 0xFF) as u8));
        }
    }
    p.mmu = Some(prefs::Mmu {
        enable: Some(state.use_mmu),
        regs_base: parse_settings_u16(&state.mmu_regs_base_hex),
        phys_bytes: parse_settings_usize(&state.mmu_phys_bytes),
        sys_mode: Some(state.mmu_mode_sys),
        task: parse_settings_u8(&state.mmu_task),
        prot: parse_settings_u16(&state.mmu_prot_hex).map(|v| (v & 0xFF) as u8),
        protect_vectors: Some(state.protect_vectors),
        log_maps: Some(state.mmu_log_maps),
        map_frames: Some(frames),
        attr_bytes: Some(attrs),
        preset_name: if state.mmu_preset_name.is_empty() {
            None
        } else {
            Some(state.mmu_preset_name.clone())
        },
    });
    p.debug = Some(prefs::Debug {
        console_log: Some(state.debug_console_log),
        block_log: Some(state.debug_block_log),
        gpio_log: Some(state.debug_gpio_log),
        mmu_log: Some(state.debug_mmu_log),
        settings_log: Some(state.debug_settings_log),
        show_settings_origins: Some(state.debug_show_origins),
    });
    if let Err(e) = prefs::save(&p) {
        eprintln!("prefs::save failed: {e}");
    }
}

fn publish_settings_baseline(state: &regview::SettingsState, gpio_on_color: u32) {
    let origins_vec: Vec<(String, u8)> =
        if let Some((_, origins_map, _)) = regview::get_latest_settings_baseline() {
            let mut entries: Vec<(String, u8)> = origins_map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            entries
        } else {
            Vec::new()
        };
    let _ = regview::push_global_command(regview::Command::UiSetSettingsBaseline(
        state.clone(),
        origins_vec,
        Some(gpio_on_color),
    ));
}

fn send_settings_ack(instance_id: u64) {
    let _ = regview::push_global_command(regview::Command::UiAckSettings { instance_id });
}

fn parse_settings_u16(s: &str) -> Option<u16> {
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

fn parse_settings_usize(s: &str) -> Option<usize> {
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
    usize::from_str_radix(digits, base).ok()
}

fn parse_settings_u32(s: &str) -> Option<u32> {
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
    u32::from_str_radix(digits, base).ok()
}

fn parse_settings_u8(s: &str) -> Option<u8> {
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
        .map(|v| (v & 0xFF) as u8)
}

fn sync_gpio_from_bus(bus: &mut dyn bus::Bus, viewer: &regview::Viewer) {
    let mut synced = false;
    if let Some(iobus) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() {
        iobus.with_gpio_mut_opt(|g| {
            let (out, dir, bits) = g.get_state();
            regview::publish_gpio_snapshot(true, bits, out, dir);
            viewer.sync_gpio(true, bits, out, dir);
            synced = true;
        });
    } else if let Some(iobus) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        iobus.with_gpio_mut_opt(|g| {
            let (out, dir, bits) = g.get_state();
            regview::publish_gpio_snapshot(true, bits, out, dir);
            viewer.sync_gpio(true, bits, out, dir);
            synced = true;
        });
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
            io_mem.with_gpio_mut_opt(|g| {
                let (out, dir, bits) = g.get_state();
                regview::publish_gpio_snapshot(true, bits, out, dir);
                viewer.sync_gpio(true, bits, out, dir);
                synced = true;
            });
        } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            io_mmu.with_gpio_mut_opt(|g| {
                let (out, dir, bits) = g.get_state();
                regview::publish_gpio_snapshot(true, bits, out, dir);
                viewer.sync_gpio(true, bits, out, dir);
                synced = true;
            });
        }
    }
    if !synced {
        regview::publish_gpio_snapshot(false, 0, 0, 0);
        viewer.sync_gpio(false, 0, 0, 0);
    }
}

fn sync_block_from_bus(bus: &mut dyn bus::Bus, viewer: &regview::Viewer) {
    fn block_info_from_iobus<B: bus::Bus>(
        iobus: &mut IoBus<B>,
    ) -> Option<(u16, Option<String>, u8, u8, u8)> {
        let mut info: Option<(u16, Option<String>, u8, u8, u8)> = None;
        iobus.with_block_mut_opt(|blk| {
            info = Some((
                blk.base,
                blk.backing_file().map(|s| s.to_string()),
                blk.last_cmd(),
                blk.last_data(),
                blk.status(),
            ));
        });
        info
    }
    let mut info: Option<(u16, Option<String>, u8, u8, u8)> = None;
    if let Some(io_mem) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() {
        info = block_info_from_iobus(io_mem);
    } else if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        info = block_info_from_iobus(io_mmu);
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() {
            info = block_info_from_iobus(io_mem);
        } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            info = block_info_from_iobus(io_mmu);
        }
    }
    if let Some((base, path_opt, last_cmd, last_data, status)) = info {
        let online = path_opt
            .as_ref()
            .map(|p| Path::new(p).exists())
            .unwrap_or(false);
        viewer.sync_block_status(
            true,
            Some(base),
            path_opt,
            online,
            last_cmd,
            last_data,
            status,
        );
    } else {
        viewer.sync_block_status(false, None, None, false, 0, 0, 0);
    }
}
