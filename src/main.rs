// Use library modules instead of duplicating

use std::env;
use std::fs;

use mc6809emu::bootscript::{emit_boot_template, parse_boot_script, BootSequencer};
use mc6809emu::bus::Bus;
use mc6809emu::bus::{self, Memory};
use mc6809emu::config::{apply_mmu_config_from_str, apply_preset};
use mc6809emu::cpu::Cpu;
use mc6809emu::debug;
use mc6809emu::disasm::disasm_one;
use mc6809emu::io::{ConsoleDev, IoBus};
use mc6809emu::loader::{self, ImageFormat, LoadedImage};
use mc6809emu::mmu::Mc6829;

fn print_usage() {
    println!("mc6809emu - MC6809 software emulator\n");
    println!("Usage:\n  mc6809emu <image> [options]\n");
    println!("Image Options:");
    println!("  --format bin|srec         Input format override");
    println!("  --base 0xADDR             Base address for raw binary");
    println!("  --pc 0xADDR               Override reset PC");
    println!("\nExecution / Debug:");
    println!("  --trace                   Trace instructions");
    println!("  --step N                  Execute N steps (required to run)\n  --cycles                 Print cycle count at end");
    println!("  --run                     Run until Ctrl+C (ignores --step)");
    println!("  --dump-regs               Dump registers");
    println!("  --dump-mem 0xADDR LEN     Dump memory range");
    println!("  --dump-mem-ascii 0xA LEN  Dump ASCII view");
    println!("  --disasm 0xADDR LEN       Disassemble range");
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
    let mut dump_regs = false;
    let mut dump_mem: Option<(u16, usize)> = None;
    let mut dump_mem_ascii: Option<(u16, usize)> = None;
    let mut print_cycles = false;
    let mut disasm_req: Option<(u16, usize)> = None;
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
                if let (Some(addr), Some(len)) = (args.next(), args.next()) {
                    if let (Some(a), Ok(l)) = (parse_hex(&addr), len.parse::<usize>()) {
                        disasm_req = Some((a, l));
                    }
                }
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
                println!("Presets: {}", mc6809emu::config::list_presets().join(", "));
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

    if dump_regs {
        debug::dump_registers(&cpu);
    }
    if let Some((addr, len)) = dump_mem {
        debug::dump_memory_bus(bus.as_mut(), addr, len);
    }
    if let Some((addr, len)) = dump_mem_ascii {
        debug::dump_memory_ascii(bus.as_mut(), addr, len);
    }

    if let Some((mut pc, total_len)) = disasm_req {
        let mut processed: usize = 0;
        while processed < total_len {
            let (consumed, text) = if disasm_hex {
                mc6809emu::disasm::disasm_one_hex(bus.as_mut(), pc)
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

    if run_forever || steps.is_some() || trace || !breakpoints.is_empty() {
        signals::install();
        // If --run, ignore --step and run until Ctrl+C
        let mut remaining = if run_forever {
            usize::MAX
        } else {
            steps.unwrap_or(usize::MAX)
        };
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
        while !STOP_REQUESTED.load(Ordering::SeqCst) {
            if let Some(ref rxch) = rx {
                if let Ok(v) = rxch.try_recv() {
                    if let Some(io_mem) = bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>()
                    {
                        io_mem.feed_console_input(&v);
                    } else if let Some(io_mmu) =
                        bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                    {
                        io_mmu.feed_console_input(&v);
                    }
                }
            }
            if let Some(ref mut seq) = boot_seq {
                seq.on_pre_step(bus.as_mut(), mmu_regs_base, cpu.r.pc, &mut cpu);
            }
            if trace {
                let (_len, text) = if disasm_hex {
                    mc6809emu::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc)
                } else {
                    disasm_one(bus.as_mut(), cpu.r.pc)
                };
                println!("{:04X}: {}", cpu.r.pc, text);
            }
            cpu.step(bus.as_mut(), false);
            if let Some(ref mut seq) = boot_seq {
                seq.on_post_step(bus.as_mut(), mmu_regs_base);
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
