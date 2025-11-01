// Use library modules instead of duplicating

use std::env;
use std::fs;
use std::path::Path;

use em6809::bootscript::{emit_boot_template, parse_boot_script, BootSequencer};
use em6809::bus::Bus;
use em6809::bus::{self, Memory};
use em6809::config::{apply_mmu_config_from_str, apply_preset};
use em6809::cpu::Cpu;
use em6809::io::{ConsoleDev, IoBus};
use em6809::loader::{self, ImageFormat, LoadedImage};
use em6809::mmu::Mc6829;

// GUI register viewer (no-op unless built with `--features gui`)
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

fn print_usage() {
    println!("em6809 - MC6809 software emulator\n");
    println!("Usage:\n  em6809 <image> [options]\n");
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
    println!("  --console-flush-every N   Flush every N chars (also flush on newline)");
    println!("  --console-echo            Local echo on RX read (default OFF)");
    println!("  --console-rx-wm N         RX IRQ watermark (if used)");
    println!("  --console-irq-hold N      IRQ hold cycles");
    println!("\nMMU / Presets:");
    println!("  --mmu                     Use MC6829 MMU backend");
    println!("  --mmu-phys BYTES          Physical size (e.g., 1048576)");
    println!("  --mmu-reg-base 0xA        MMU regs base address");
    println!("  --mmu-task N              Task register");
    println!("  --mmu-idmap               Identity map current task");
    println!("  --mmu-map P=F             Map page P to frame F (hex or dec)");
    println!("  --mmu-map-file PATH       Apply map/config from file");
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
    use libc::{c_int, tcgetattr, tcsetattr, termios, ECHO, ICANON, STDIN_FILENO, TCSANOW, VMIN, VTIME};

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
    let mut args = env::args().skip(1).peekable();
    // Help early-exit
    if let Some(first) = args.peek() {
        if first == "--help" || first == "-h" {
            print_usage();
            return;
        }
    }
    let Some(image_path) = args.next() else {
        print_usage();
        return;
    };

    let mut format: Option<ImageFormat> = None;
    let mut base: Option<u16> = None;
    let mut force_pc: Option<u16> = None;
    // GUI-only: trace/step removed; pause is default
    let paused_start: bool = true;
    // GUI-only: legacy CUI dump flags removed
    let mut print_cycles = false;
    // GUI-only: legacy CUI disasm request removed
    // GUI-only: --run and --bp removed
    let mut use_mmu = false;
    let mut mmu_phys: usize = 1 << 20; // 1MiB default
    let mut mmu_regs_base: u16 = 0xFFA0;
    let mut mmu_task: u8 = 0;
    let mut mmu_idmap = false;
    let mut console_base: Option<u16> = None;
    let mut console_in: Option<String> = None;
    let mut console_stdin = false;
    let mut mmu_maps: Vec<(usize, u16)> = Vec::new();
    let mut mmu_map_file: Option<String> = None;
    let mut preset_name: Option<String> = None;
    let mut protect_vectors = false;
    let mut mmu_mode_sys: Option<bool> = None;
    let mut mmu_prot: Option<u8> = None;
    let mut mmu_attrs: Vec<(usize, u8)> = Vec::new();
    let mut console_rx_wm: Option<usize> = None;
    let mut console_irq_hold: Option<u32> = None;
    let mut boot_script_path: Option<String> = None;
    let mut console_out_path: Option<String> = None;
    let mut console_tee_stderr: bool = false;
    let mut console_flush_every: Option<usize> = None;
    let mut console_local_echo: bool = false; // default OFF
    let mut emit_boot_template_name: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => {
                if let Some(f) = args.next() {
                    format = match f.as_str() {
                        "bin" => Some(ImageFormat::Binary),
                        "srec" => Some(ImageFormat::Srec),
                        other => {
                            eprintln!("Unknown format: {}", other);
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
            // --run removed in GUI-only mode
            // --paused is default in GUI-only mode; ignore if present
            "--paused" => {}
            "--dump-regs" => { /* ignored in GUI-only mode */ }
            "--dump-mem" => { let _ = (args.next(), args.next()); /* ignored */ }
            "--dump-mem-ascii" => { let _ = (args.next(), args.next()); /* ignored */ }
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
                        let page = p.strip_prefix("0x").or(Some(p)).unwrap();
                        let frame = f.strip_prefix("0x").or(Some(f)).unwrap();
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
                }
            }
            "--console-in" => {
                if let Some(v) = args.next() {
                    console_in = Some(v);
                }
            }
            "--console-stdin" => {
                console_stdin = true;
            }
            "--console-out" => {
                if let Some(v) = args.next() {
                    console_out_path = Some(v);
                }
            }
            "--console-tee-stderr" => {
                console_tee_stderr = true;
            }
            "--console-flush-every" => {
                if let Some(v) = args.next() {
                    if let Ok(n) = v.parse::<usize>() {
                        console_flush_every = Some(n);
                    }
                }
            }
            "--console-echo" => {
                console_local_echo = true;
            }
            "--console-rx-wm" => {
                if let Some(v) = args.next() {
                    if let Ok(n) = v.parse::<usize>() {
                        console_rx_wm = Some(n);
                    }
                }
            }
            "--console-irq-hold" => {
                if let Some(v) = args.next() {
                    if let Ok(n) = v.parse::<u32>() {
                        console_irq_hold = Some(n);
                    }
                }
            }
            "--mmu-map-file" => {
                if let Some(v) = args.next() {
                    mmu_map_file = Some(v);
                }
            }
            "--preset" | "--mmu-template" => {
                if let Some(v) = args.next() {
                    preset_name = Some(v);
                    use_mmu = true;
                }
            }
            "--protect-vectors" => {
                protect_vectors = true;
            }
            "--mmu-mode" => {
                if let Some(v) = args.next() {
                    let s = v.to_lowercase();
                    if s == "sys" || s == "system" {
                        mmu_mode_sys = Some(true);
                    } else if s == "user" {
                        mmu_mode_sys = Some(false);
                    }
                }
            }
            "--prot" => {
                if let Some(v) = args.next() {
                    if let Some(p) = parse_hex(&v) {
                        mmu_prot = Some((p & 0xFF) as u8);
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
                eprintln!("Unknown arg: {}", arg);
                print_usage();
                return;
            }
        }
    }

    let data = match fs::read(&image_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to read {}: {}", image_path, e);
            return;
        }
    };

    let inferred = if let Some(fmt) = format {
        fmt
    } else {
        if image_path.ends_with(".s19")
            || image_path.ends_with(".srec")
            || image_path.ends_with(".s1")
        {
            ImageFormat::Srec
        } else {
            ImageFormat::Binary
        }
    };

    // Choose memory backend and build a Bus object
    let mut bus: Box<dyn bus::Bus> = if use_mmu {
        let mut mmu = Mc6829::new(mmu_phys, mmu_regs_base);
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
                        eprintln!("MMU map file error: {}", e);
                        return;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read {}: {}", path, e);
                    return;
                }
            }
        }
        if let Some(pname) = &preset_name {
            if let Err(e) = apply_preset(&mut mmu, pname) {
                eprintln!("Preset error: {}", e);
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
        if let Some(base) = console_base {
            let mut iobus = IoBus::new(mmu);
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
            if console_local_echo {
                con.set_local_echo(true);
            }
            iobus.add_device(con);
            Box::new(iobus)
        } else {
            Box::new(mmu)
        }
    } else {
        let mem = Memory::new();
        if let Some(base) = console_base {
            let mut iobus = IoBus::new(mem);
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
            if console_local_echo {
                con.set_local_echo(true);
            }
            iobus.add_device(con);
            Box::new(iobus)
        } else {
            Box::new(mem)
        }
    };

    if let Some(name) = &emit_boot_template_name {
        let t = emit_boot_template(name);
        let out = format!("{}.boot", name);
        match std::fs::write(&out, t) {
            Ok(_) => {
                println!("Wrote {}", out);
            }
            Err(e) => {
                eprintln!("Failed to write {}: {}", out, e);
            }
        }
        return;
    }

    let loaded: LoadedImage = match inferred {
        ImageFormat::Binary => {
            let base_addr = base.unwrap_or(0x0000);
            loader::load_binary_bus(bus.as_mut(), base_addr, &data)
        }
        ImageFormat::Srec => {
            match loader::load_srec_bus(bus.as_mut(), &String::from_utf8_lossy(&data)) {
                Ok(img) => img,
                Err(e) => {
                    eprintln!("SREC parse error: {}", e);
                    return;
                }
            }
        }
    };

    let mut cpu = Cpu::new();
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

    // Always prepare the register viewer (GUI-only app).
    // On Windows we spawn it on a background thread.
    // On Unix/macOS we prepare it and run on the main thread later.
    let reg_viewer = {
        // Try to load a companion .lst file for display in the viewer
        let lst_path_opt = {
            let lst_path = Path::new(&image_path).with_extension("lst");
            if lst_path.exists() {
                Some(lst_path)
            } else {
                None
            }
        };
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
            (cpu.r.pc, cpu.r.pc)
        };
        let lst_fallback_text: Option<String> = if start != end {
            let mut cur = start;
            let mut out = String::new();
            let mut processed: usize = 0;
            let total_len: usize = end.wrapping_sub(start) as usize;
            while processed < total_len {
                let (consumed, text) = em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                use core::fmt::Write as _;
                let _ = writeln!(&mut out, "{:04X}: {}", cur, text);
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
        Some(regview::Viewer::create(
                "em6809 - MC6809 Software Emulator",
                lst_path_opt,
                lst_fallback_text,
                start,
            ))
        }
        #[cfg(windows)]
        {
            Some(regview::Viewer::spawn(
                "em6809 - MC6809 Software Emulator",
                lst_path_opt,
                lst_fallback_text,
                start,
            ))
        }
    };

    // Always GUI: legacy headless register dump removed.
    // GUI-only: CUI dumps suppressed; consider wiring these to GUI actions.

    // GUI-only: --disasm no longer prints/returns; the viewer provides listing/disasm.

    // Boot script
    let mut boot_seq: Option<BootSequencer> = if let Some(p) = &boot_script_path {
        match std::fs::read_to_string(p) {
            Ok(s) => match parse_boot_script(&s) {
                Ok(v) => Some(BootSequencer::new(v)),
                Err(e) => {
                    eprintln!("Boot script parse error: {}", e);
                    None
                }
            },
            Err(e) => {
                eprintln!("Failed to read boot script {}: {}", p, e);
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
            let _loaded_img = loaded;
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
                } else { None };

                // Boot script
                let mut boot_seq: Option<BootSequencer> = if let Some(p) = &boot_script_path_opt {
                    match std::fs::read_to_string(p) {
                        Ok(s) => parse_boot_script(&s).ok().map(BootSequencer::new),
                        Err(_) => None,
                    }
                } else { None };

                if paused_start_flag {
                    viewer_cpu.set_paused(true);
                    let snap = regview::RegSnapshot {
                        a: cpu_worker.r.a, b: cpu_worker.r.b,
                        d: ((cpu_worker.r.a as u16) << 8) | (cpu_worker.r.b as u16),
                        x: cpu_worker.r.x, y: cpu_worker.r.y,
                        u: cpu_worker.r.u, s: cpu_worker.r.s,
                        dp: cpu_worker.r.dp, pc: cpu_worker.r.pc, cc: cpu_worker.r.cc,
                    };
                    viewer_cpu.push(snap);
                }

                'run_loop: while !STOP_REQUESTED.load(Ordering::SeqCst) {
                    if STOP_REQUESTED.load(Ordering::SeqCst) {
                        // Request GUI to close and break
                        viewer_cpu.request_close();
                        break 'run_loop;
                    }
                    if viewer_cpu.is_paused() {
                        let snap = regview::RegSnapshot { a: cpu_worker.r.a, b: cpu_worker.r.b, d: ((cpu_worker.r.a as u16) << 8) | (cpu_worker.r.b as u16), x: cpu_worker.r.x, y: cpu_worker.r.y, u: cpu_worker.r.u, s: cpu_worker.r.s, dp: cpu_worker.r.dp, pc: cpu_worker.r.pc, cc: cpu_worker.r.cc };
                        viewer_cpu.push(snap);
                        let (mstart, mlen, mfollow) = viewer_cpu.get_memory_window();
                        let eff_start: u16 = if mfollow { cpu_worker.r.pc & 0xFFF0 } else { mstart };
                        let mut buf: Vec<u8> = Vec::with_capacity(mlen);
                        let mut a = eff_start;
                        for _ in 0..mlen { buf.push(bus_worker.as_mut().read8(a)); a = a.wrapping_add(1); }
                        viewer_cpu.update_memory_window(eff_start, buf);
                        let (consumed, _) = em6809::disasm::disasm_one_hex(bus_worker.as_mut(), cpu_worker.r.pc);
                        let clen = core::cmp::max(consumed as usize, 1);
                        viewer_cpu.set_memory_highlight(cpu_worker.r.pc, clen);
                        while viewer_cpu.should_pause() {
                            if STOP_REQUESTED.load(Ordering::SeqCst) { break 'run_loop; }
                            while let Some(cmd) = viewer_cpu.take_command() {
                                match cmd {
                                    regview::Command::SetPc(a) => {
                                        cpu_worker.set_pc(a);
                                        let snap2 = regview::RegSnapshot { a: cpu_worker.r.a, b: cpu_worker.r.b, d: ((cpu_worker.r.a as u16) << 8) | (cpu_worker.r.b as u16), x: cpu_worker.r.x, y: cpu_worker.r.y, u: cpu_worker.r.u, s: cpu_worker.r.s, dp: cpu_worker.r.dp, pc: cpu_worker.r.pc, cc: cpu_worker.r.cc };
                                        viewer_cpu.push(snap2);
                                    }
                                    regview::Command::RunUntil(a) => { run_until_target = Some(a); viewer_cpu.set_paused(false); break; }
                                    regview::Command::ApplyRegs(ns) => {
                                        cpu_worker.r.a = ns.a; cpu_worker.r.b = ns.b; cpu_worker.r.x = ns.x; cpu_worker.r.y = ns.y; cpu_worker.r.u = ns.u; cpu_worker.r.s = ns.s; cpu_worker.r.dp = ns.dp; cpu_worker.r.pc = ns.pc; cpu_worker.r.cc = ns.cc;
                                        let snap2 = regview::RegSnapshot { a: cpu_worker.r.a, b: cpu_worker.r.b, d: ((cpu_worker.r.a as u16) << 8) | (cpu_worker.r.b as u16), x: cpu_worker.r.x, y: cpu_worker.r.y, u: cpu_worker.r.u, s: cpu_worker.r.s, dp: cpu_worker.r.dp, pc: cpu_worker.r.pc, cc: cpu_worker.r.cc };
                                        viewer_cpu.push(snap2);
                                    }
                                }
                            }
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                    }

                    if let Some(ref rxch) = rx { if let Ok(v) = rxch.try_recv() { if let Some(io_mem) = bus_worker.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>() { io_mem.feed_console_input(&v); } else if let Some(io_mmu) = bus_worker.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>() { io_mmu.feed_console_input(&v); } else if let Some(wt) = bus_worker.as_mut().as_any_mut().downcast_mut::<bus::WriteTrack>() { let inner_any = wt.inner_any_mut(); if let Some(io_mem) = inner_any.downcast_mut::<IoBus<Memory>>() { io_mem.feed_console_input(&v); } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() { io_mmu.feed_console_input(&v); } } } }
                    if let Some(ref mut seq) = boot_seq { seq.on_pre_step(bus_worker.as_mut(), mmu_regs_base_copy, cpu_worker.r.pc, &mut cpu_worker); }
                    // trace printing removed in GUI-only mode
                    cpu_worker.step(bus_worker.as_mut(), false);
                    if viewer_cpu.is_paused() == false { let snap = regview::RegSnapshot { a: cpu_worker.r.a, b: cpu_worker.r.b, d: ((cpu_worker.r.a as u16) << 8) | (cpu_worker.r.b as u16), x: cpu_worker.r.x, y: cpu_worker.r.y, u: cpu_worker.r.u, s: cpu_worker.r.s, dp: cpu_worker.r.dp, pc: cpu_worker.r.pc, cc: cpu_worker.r.cc }; viewer_cpu.push(snap); }
                    if let Some(ref mut seq) = boot_seq { seq.on_post_step(bus_worker.as_mut(), mmu_regs_base_copy); }
                    if let Some(tgt) = run_until_target { if cpu_worker.r.pc == tgt { viewer_cpu.set_paused(true); run_until_target = None; } }
                    // no step counter; no CLI breakpoints in GUI-only mode
                }
                if let Some(prev) = prev_mode { console_mode::restore_stdin_mode(prev); }
                // print cycles
                if print_cycles_flag { println!("cycles: {}", cpu_worker.cycles); }
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
            if let Some(viewer) = &reg_viewer {
                if viewer.is_closed() {
                    // GUI window closed -> exit
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
                // Update memory pane based on current settings
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
                // While paused, also process GUI commands so right-click actions take effect immediately
                while viewer.should_pause() {
                    if STOP_REQUESTED.load(Ordering::SeqCst) {
                        break 'run_loop;
                    }
                    if viewer.is_closed() {
                        break 'run_loop;
                    }
                    // Handle queued commands even while paused
                    while let Some(cmd) = viewer.take_command() {
                        match cmd {
                            regview::Command::SetPc(a) => {
                                cpu.set_pc(a);
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
                                let (mstart, mlen, mfollow) = viewer.get_memory_window();
                                let eff_start: u16 =
                                    if mfollow { cpu.r.pc & 0xFFF0 } else { mstart };
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
                                        let _ = writeln!(&mut out, "{:04X}: {}", cur, text);
                                        let step = core::cmp::max(consumed as usize, 1);
                                        processed += step;
                                        cur = cur.wrapping_add(step as u16);
                                    }
                                    viewer.update_hex_listing(out);
                                }
                            }
                            regview::Command::RunUntil(a) => {
                                run_until_target = Some(a);
                                viewer.set_paused(false);
                                // break pause loop to resume execution immediately
                                break;
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
                                let (consumed, _) = em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc);
                                let clen = core::cmp::max(consumed as usize, 1);
                                viewer.set_memory_highlight(cpu.r.pc, clen);
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
                                        let (consumed, text) = em6809::disasm::disasm_one_hex(bus.as_mut(), cur);
                                        use core::fmt::Write as _;
                                        let _ = writeln!(&mut out, "{:04X}: {}", cur, text);
                                        let step = core::cmp::max(consumed as usize, 1);
                                        processed += step;
                                        cur = cur.wrapping_add(step as u16);
                                    }
                                    viewer.update_hex_listing(out);
                                }
                            }
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
                        regview::Command::SetPc(a) => {
                            cpu.set_pc(a);
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
                                    let _ = writeln!(&mut out, "{:04X}: {}", cur, text);
                                    let step = core::cmp::max(consumed as usize, 1);
                                    processed += step;
                                    cur = cur.wrapping_add(step as u16);
                                }
                                viewer.update_hex_listing(out);
                            }
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
                                    let _ = writeln!(&mut out, "{:04X}: {}", cur, text);
                                    let step = core::cmp::max(consumed as usize, 1);
                                    processed += step;
                                    cur = cur.wrapping_add(step as u16);
                                }
                                viewer.update_hex_listing(out);
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
                            let (consumed, _) = em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc);
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
                                    let _ = writeln!(&mut out, "{:04X}: {}", cur, text);
                                    let step = core::cmp::max(consumed as usize, 1);
                                    processed += step;
                                    cur = cur.wrapping_add(step as u16);
                                }
                                viewer.update_hex_listing(out);
                            }
                        }
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
                                let _ = writeln!(&mut out, "{:04X}: {}", cur, text);
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
                                    let line = format!("{:04X}: {}", cur, text);
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
