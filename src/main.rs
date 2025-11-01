// Use library modules instead of duplicating

use std::env;
use std::fs;
use std::path::Path;

use em6809::bootscript::{emit_boot_template, parse_boot_script, BootSequencer};
use em6809::bus::Bus;
use em6809::bus::{self, Memory};
use em6809::config::{apply_mmu_config_from_str, apply_preset};
use em6809::cpu::Cpu;
use em6809::debug;
use em6809::disasm::disasm_one;
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
    println!("  --trace                   Trace instructions");
    println!("  --step N                  Execute N steps (required to run)\n  --cycles                 Print cycle count at end");
    println!("  --run                     Run until Ctrl+C (ignores --step)");
    println!("  --paused                  Start paused at program entry (overrides --run/--step/--trace/breakpoints until resumed)");
    println!("  --dump-regs               Dump registers");
    println!("  --dump-mem 0xADDR LEN     Dump memory range");
    println!("  --dump-mem-ascii 0xA LEN  Dump ASCII view");
    println!("  --disasm [ADDR] [LEN]     Disassemble range (ADDR defaults to program start PC; LEN defaults to program size if both omitted, or 64 if only ADDR given; numbers accept 0x, 0o, 0b or decimal)");
    println!("  --hex                     Use hex-style disassembly");
    println!("  --bp 0xADDR               Breakpoint (repeatable)");
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

fn parse_num_u16_any(s: &str) -> Option<u16> {
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

fn parse_num_usize_any(s: &str) -> Option<usize> {
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
    pub type Dummy = ();
    pub fn enter_raw() -> Option<()> {
        None
    }
    pub fn restore_stdin_mode(_: ()) {}
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
    let mut trace = false;
    let mut steps: Option<usize> = None;
    let mut paused_start: bool = false;
    let mut dump_regs = false;
    let mut dump_mem: Option<(u16, usize)> = None;
    let mut dump_mem_ascii: Option<(u16, usize)> = None;
    let mut print_cycles = false;
    // Disassembler request: (optional start address, optional length)
    let mut disasm_req: Option<(Option<u16>, Option<usize>)> = None;
    let mut disasm_hex = false;
    let mut breakpoints: Vec<u16> = Vec::new();
    let mut run_forever: bool = false; // --run: execute until Ctrl+C
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
            "--trace" => trace = true,
            "--cycles" => print_cycles = true,
            "--step" => {
                if let Some(v) = args.next() {
                    steps = v.parse::<usize>().ok();
                }
            }
            "--run" => {
                run_forever = true;
            }
            "--paused" => {
                paused_start = true;
            }
            "--dump-regs" => dump_regs = true,
            "--dump-mem" => {
                if let Some(addr) = args.next() {
                    if let Some(len) = args.next() {
                        if let (Some(a), Ok(l)) = (parse_hex(&addr), len.parse::<usize>()) {
                            dump_mem = Some((a, l));
                        }
                    }
                }
            }
            "--dump-mem-ascii" => {
                if let Some(addr) = args.next() {
                    if let Some(len) = args.next() {
                        if let (Some(a), Ok(l)) = (parse_hex(&addr), len.parse::<usize>()) {
                            dump_mem_ascii = Some((a, l));
                        }
                    }
                }
            }
            "--disasm" => {
                // Accept zero, one, or two params; autodetect bases (0x, 0o, 0b, or decimal)
                let mut addr_opt: Option<u16> = None;
                let mut len_opt: Option<usize> = None;
                if let Some(peek) = args.peek() {
                    if !peek.starts_with("--") {
                        let tok = args.next().unwrap();
                        // First positional is address; allow decimal too
                        addr_opt = parse_num_u16_any(&tok).or_else(|| {
                            parse_num_usize_any(&tok).and_then(|v| {
                                if v <= 0xFFFF {
                                    Some(v as u16)
                                } else {
                                    None
                                }
                            })
                        });
                    }
                }
                if let Some(peek) = args.peek() {
                    if !peek.starts_with("--") {
                        let tok = args.next().unwrap();
                        len_opt = parse_num_usize_any(&tok);
                    }
                }
                disasm_req = Some((addr_opt, len_opt));
            }
            "--hex" => {
                disasm_hex = true;
            }
            "--bp" => {
                if let Some(v) = args.next() {
                    if let Some(a) = parse_hex(&v) {
                        breakpoints.push(a);
                    }
                }
            }
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

    // If GUI is enabled and --dump-regs is set, launch the register viewer.
    // The viewer API is a no-op in non-GUI builds.
    let reg_viewer = if dump_regs {
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
        Some(regview::Viewer::spawn(
            "MC6809 Registers",
            lst_path_opt,
            lst_fallback_text,
            start,
        ))
    } else {
        None
    };

    // Keep legacy immediate dump only for headless (non-GUI) builds.
    #[cfg(not(feature = "gui"))]
    {
        if dump_regs {
            debug::dump_registers(&cpu);
        }
    }
    if let Some((addr, len)) = dump_mem {
        debug::dump_memory_bus(bus.as_mut(), addr, len);
    }
    if let Some((addr, len)) = dump_mem_ascii {
        debug::dump_memory_ascii(bus.as_mut(), addr, len);
    }

    if let Some((addr_opt, len_opt)) = disasm_req {
        let mut pc: u16 = addr_opt.unwrap_or(cpu.r.pc);
        // Default length for --disasm when omitted:
        // choose the coalesced span that contains the chosen start PC
        let spans = coalesce_ranges(loaded.loaded_ranges.clone());
        let span_for_pc = spans.iter().find(|(s, e)| *s <= pc && pc < *e).copied();
        let default_full_len = span_for_pc
            .map(|(s, e)| e.wrapping_sub(s) as usize)
            .unwrap_or(64);
        let total_len: usize = match (addr_opt, len_opt) {
            (None, None) => default_full_len,
            (Some(_), None) => 64,
            (_, Some(n)) => n,
        };
        let mut processed: usize = 0;
        while processed < total_len {
            let (consumed, text) = if disasm_hex {
                em6809::disasm::disasm_one_hex(bus.as_mut(), pc)
            } else {
                disasm_one(bus.as_mut(), pc)
            };
            println!("{:04X}: {}", pc, text);
            pc = pc.wrapping_add(consumed);
            processed += consumed as usize;
        }
        return;
    }

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

    if run_forever || steps.is_some() || trace || !breakpoints.is_empty() || paused_start {
        signals::install();
        // If --run, ignore --step and run until Ctrl+C
        let mut remaining = if run_forever {
            usize::MAX
        } else {
            steps.unwrap_or(usize::MAX)
        };
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
            if trace {
                let (_len, text) = if disasm_hex {
                    em6809::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc)
                } else {
                    disasm_one(bus.as_mut(), cpu.r.pc)
                };
                println!("{:04X}: {}", cpu.r.pc, text);
            }
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
            if !run_forever {
                remaining = remaining.saturating_sub(1);
                if remaining == 0 {
                    break;
                }
            }
            if !breakpoints.is_empty() && breakpoints.contains(&cpu.r.pc) {
                println!("Breakpoint hit at {:04X}", cpu.r.pc);
                break;
            }
        }
        // Restore console mode (Windows) if we disabled echo
        if let Some(prev) = prev_mode {
            #[cfg(windows)]
            {
                console_mode::restore_stdin_mode(prev);
            }
        }
    }

    if print_cycles {
        println!("cycles: {}", cpu.cycles);
    }
}
