// Use library modules instead of duplicating

use std::env;
use std::fs;

use mc6809emu::bus::{self, Memory};
use mc6809emu::bus::Bus;
use mc6809emu::cpu::Cpu;
use mc6809emu::loader::{self, ImageFormat, LoadedImage};
use mc6809emu::disasm::disasm_one;
use mc6809emu::mmu::Mc6829;
use mc6809emu::io::{IoBus, ConsoleDev};
use mc6809emu::config::{apply_mmu_config_from_str, apply_preset, list_presets};
use mc6809emu::bootscript::{parse_boot_script, BootSequencer, emit_boot_template};
use mc6809emu::debug;

fn print_usage() {
    eprintln!(
        "Usage: mc6809emu <image> [--format bin|srec] [--base 0xADDR] [--pc 0xADDR] [--trace] [--step N] [--dump-regs] [--dump-mem 0xADDR LEN]"
    );
}

fn parse_hex(s: &str) -> Option<u16> {
    let t = s.trim_start_matches("0x");
    u16::from_str_radix(t, 16).ok()
}

fn main() {
    let mut args = env::args().skip(1);
    let Some(image_path) = args.next() else { print_usage(); return; };

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
    let mut use_mmu = false;
    let mut mmu_phys: usize = 1<<20; // 1MiB default
    let mut mmu_regs_base: u16 = 0xFFA0;
    let mut mmu_task: u8 = 0;
    let mut mmu_idmap = false;
    let mut console_base: Option<u16> = None;
    let mut console_in: Option<String> = None;
    let mut console_stdin = false;
    let mut mmu_maps: Vec<(usize,u16)> = Vec::new();
    let mut mmu_map_file: Option<String> = None;
    let mut preset_name: Option<String> = None;
    let mut protect_vectors = false;
    let mut mmu_mode_sys: Option<bool> = None;
    let mut mmu_prot: Option<u8> = None;
    let mut mmu_attrs: Vec<(usize,u8)> = Vec::new();
    let mut console_rx_wm: Option<usize> = None;
    let mut console_irq_hold: Option<u32> = None;
    let mut boot_script_path: Option<String> = None;
    let mut emit_boot_template_name: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => {
                if let Some(f) = args.next() {
                    format = match f.as_str() {
                        "bin" => Some(ImageFormat::Binary),
                        "srec" => Some(ImageFormat::Srec),
                        other => { eprintln!("Unknown format: {}", other); return; }
                    };
                }
            }
            "--base" => {
                if let Some(v) = args.next() { base = parse_hex(&v); }
            }
            "--pc" => {
                if let Some(v) = args.next() { force_pc = parse_hex(&v); }
            }
            "--trace" => trace = true,
            "--cycles" => print_cycles = true,
            "--step" => {
                if let Some(v) = args.next() { steps = v.parse::<usize>().ok(); }
            }
            "--dump-regs" => dump_regs = true,
            "--dump-mem" => {
                if let Some(addr) = args.next() { 
                    if let Some(len) = args.next() {
                        if let (Some(a), Ok(l)) = (parse_hex(&addr), len.parse::<usize>()) { dump_mem = Some((a, l)); }
                    }
                }
            }
            "--dump-mem-ascii" => {
                if let Some(addr) = args.next() { 
                    if let Some(len) = args.next() {
                        if let (Some(a), Ok(l)) = (parse_hex(&addr), len.parse::<usize>()) { dump_mem_ascii = Some((a, l)); }
                    }
                }
            }
            "--disasm" => {
                if let (Some(addr), Some(len)) = (args.next(), args.next()) {
                    if let (Some(a), Ok(l)) = (parse_hex(&addr), len.parse::<usize>()) { disasm_req = Some((a, l)); }
                }
            }
            "--hex" => { disasm_hex = true; }
            "--bp" => {
                if let Some(v) = args.next() { if let Some(a) = parse_hex(&v) { breakpoints.push(a); } }
            }
            "--mmu" => use_mmu = true,
            "--mmu-phys" => { if let Some(v) = args.next() { if let Ok(n) = v.parse::<usize>() { mmu_phys = n; } } }
            "--mmu-reg-base" => { if let Some(v) = args.next() { if let Some(a) = parse_hex(&v) { mmu_regs_base = a; } } }
            "--mmu-task" => { if let Some(v) = args.next() { if let Ok(t) = v.parse::<u8>() { mmu_task = t; } } }
            "--mmu-idmap" => mmu_idmap = true,
            "--mmu-map" => {
                if let Some(v) = args.next() {
                    // format: PAGE=FRAME (hex like 0x3=0x42, or decimal)
                    if let Some(eq) = v.find('=') {
                        let (p, f) = v.split_at(eq);
                        let f = &f[1..];
                        let page = p.strip_prefix("0x").or(Some(p)).unwrap();
                        let frame = f.strip_prefix("0x").or(Some(f)).unwrap();
                        if let (Ok(pp), Ok(ff)) = (usize::from_str_radix(page, if p.starts_with("0x"){16}else{10}), u16::from_str_radix(frame, if f.starts_with("0x"){16}else{10})) {
                            mmu_maps.push((pp, ff));
                        }
                    }
                }
            }
            "--console" => { if let Some(v) = args.next() { console_base = parse_hex(&v); } }
            "--console-in" => { if let Some(v) = args.next() { console_in = Some(v); } }
            "--console-stdin" => { console_stdin = true; }
            "--console-rx-wm" => { if let Some(v) = args.next() { if let Ok(n) = v.parse::<usize>() { console_rx_wm = Some(n); } } }
            "--console-irq-hold" => { if let Some(v) = args.next() { if let Ok(n) = v.parse::<u32>() { console_irq_hold = Some(n); } } }
            "--mmu-map-file" => { if let Some(v) = args.next() { mmu_map_file = Some(v); } }
            "--preset" | "--mmu-template" => { if let Some(v) = args.next() { preset_name = Some(v); use_mmu = true; } }
            "--protect-vectors" => { protect_vectors = true; }
            "--mmu-mode" => { if let Some(v)=args.next(){ let s=v.to_lowercase(); if s=="sys" || s=="system" { mmu_mode_sys=Some(true);} else if s=="user"{ mmu_mode_sys=Some(false);} } }
            "--prot" => { if let Some(v)=args.next(){ if let Some(p)=parse_hex(&v){ mmu_prot = Some((p & 0xFF) as u8); } } }
            "--attr" => { if let Some(pv)=args.next(){ if let Some(eq)=pv.find('='){ let (p,v)=pv.split_at(eq); let v=&v[1..]; if let (Ok(pp), Ok(vv)) = (usize::from_str_radix(p.trim_start_matches("0x"), if p.starts_with("0x"){16}else{10}), u8::from_str_radix(v.trim_start_matches("0x"), if v.starts_with("0x"){16}else{10})) { mmu_attrs.push((pp, vv)); } } } }
            "--preset-list" => { println!("Presets: {}", mc6809emu::config::list_presets().join(", ")); return; }
            "--boot-script" => { if let Some(v)=args.next(){ boot_script_path = Some(v); } }
            "--emit-boot-template" => { if let Some(v)=args.next(){ emit_boot_template_name = Some(v); } }
            _ => { eprintln!("Unknown arg: {}", arg); print_usage(); return; }
        }
    }

    let data = match fs::read(&image_path) {
        Ok(v) => v,
        Err(e) => { eprintln!("Failed to read {}: {}", image_path, e); return; }
    };

    let inferred = if let Some(fmt) = format { fmt } else {
        if image_path.ends_with(".s19") || image_path.ends_with(".srec") || image_path.ends_with(".s1") { ImageFormat::Srec } else { ImageFormat::Binary }
    };

    // Choose memory backend and build a Bus object
    let mut bus: Box<dyn bus::Bus> = if use_mmu {
        let mut mmu = Mc6829::new(mmu_phys, mmu_regs_base);
        mmu.set_task(mmu_task);
        if mmu_idmap { mmu.identity_map_current(); }
        for (p,f) in &mmu_maps { mmu.set_map_entry(*p, *f); }
        if let Some(path) = &mmu_map_file {
            match std::fs::read_to_string(path) { Ok(s) => { if let Err(e)=apply_mmu_config_from_str(&mut mmu, &s) { eprintln!("MMU map file error: {}", e); return; } }, Err(e) => { eprintln!("Failed to read {}: {}", path, e); return; } }
        }
        if let Some(pname) = &preset_name {
            if let Err(e) = apply_preset(&mut mmu, pname) { eprintln!("Preset error: {}", e); return; }
            // Consoleの既定
            if pname.to_lowercase().starts_with("os9l2") && console_base.is_none() { console_base = Some(0xFF00); }
        }
        if let Some(sys)=mmu_mode_sys { if sys { mmu.write8(mmu.regs_base + 0x11, 0x01); } else { mmu.write8(mmu.regs_base + 0x11, 0x00); } }
        if let Some(p)=mmu_prot { mmu.write8(mmu.regs_base + 0x12, p); }
        for (pg, val) in &mmu_attrs { mmu.write8(mmu.regs_base + 0x18 + (*pg as u16), *val); }
        if protect_vectors { mmu.write8(mmu.regs_base + 0x12, 0x03); mmu.write8(mmu.regs_base + 0x18 + 0x0F, 0x01); }
        if let Some(base) = console_base {
            let mut iobus = IoBus::new(mmu);
            let mut con = ConsoleDev::new(base);
            if let Some(wm) = console_rx_wm { con.set_rx_watermark(wm); }
            if let Some(h) = console_irq_hold { con.set_irq_hold_cycles(h); }
            if let Some(s) = &console_in { con.feed_bytes(s.as_bytes()); }
            iobus.add_device(con);
            Box::new(iobus)
        } else { Box::new(mmu) }
    } else {
        let mem = Memory::new();
        if let Some(base) = console_base {
            let mut iobus = IoBus::new(mem);
            let mut con = ConsoleDev::new(base);
            if let Some(wm) = console_rx_wm { con.set_rx_watermark(wm); }
            if let Some(h) = console_irq_hold { con.set_irq_hold_cycles(h); }
            if let Some(s) = &console_in { con.feed_bytes(s.as_bytes()); }
            iobus.add_device(con);
            Box::new(iobus)
        } else { Box::new(mem) }
    };

    if let Some(name) = &emit_boot_template_name {
        let t = emit_boot_template(name);
        let out = format!("{}.boot", name);
        match std::fs::write(&out, t) { Ok(_) => { println!("Wrote {}", out); }, Err(e)=>{ eprintln!("Failed to write {}: {}", out, e); } }
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
                Err(e) => { eprintln!("SREC parse error: {}", e); return; }
            }
        }
    };

    let mut cpu = Cpu::new();
    cpu.reset(bus.as_mut());
    if let Some(pc) = force_pc.or(loaded.entry) { cpu.set_pc(pc); }

    if dump_regs { debug::dump_registers(&cpu); }
    if let Some((addr, len)) = dump_mem { debug::dump_memory_bus(bus.as_mut(), addr, len); }
    if let Some((addr, len)) = dump_mem_ascii { debug::dump_memory_ascii(bus.as_mut(), addr, len); }

    if let Some((mut pc, total_len)) = disasm_req {
        let mut processed: usize = 0;
        while processed < total_len {
            let (consumed, text) = if disasm_hex { mc6809emu::disasm::disasm_one_hex(bus.as_mut(), pc) } else { disasm_one(bus.as_mut(), pc) };
            println!("{:04X}: {}", pc, text);
            pc = pc.wrapping_add(consumed);
            processed += consumed as usize;
        }
        return;
    }

    // Boot script
    let mut boot_seq: Option<BootSequencer> = if let Some(p) = &boot_script_path { match std::fs::read_to_string(p){ Ok(s)=> match parse_boot_script(&s){ Ok(v)=> Some(BootSequencer::new(v)), Err(e)=>{ eprintln!("Boot script parse error: {}", e); None } }, Err(e)=>{ eprintln!("Failed to read boot script {}: {}", p, e); None } } } else { None };

    if steps.is_some() || trace || !breakpoints.is_empty() {
        let mut remaining = steps.unwrap_or(usize::MAX);
        // Generic stdin feeder for dyn Bus path, feeding IoBus<Memory|Mc6829>
        let rx = if console_stdin { use std::io::Read; use std::sync::mpsc; use std::thread; let (tx, rx)=mpsc::channel::<Vec<u8>>(); thread::spawn(move || { let mut stdin=std::io::stdin(); let mut buf=[0u8;256]; loop { match stdin.read(&mut buf){ Ok(0)=>break, Ok(n)=>{ let _=tx.send(buf[..n].to_vec()); }, Err(_)=>break } } }); Some(rx) } else { None };
        while remaining > 0 {
            if let Some(ref rxch) = rx { if let Ok(v) = rxch.try_recv() { if let Some(io_mem) = bus.as_mut().as_any_mut().downcast_mut::<IoBus<Memory>>() { io_mem.feed_console_input(&v); } else if let Some(io_mmu) = bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>() { io_mmu.feed_console_input(&v); } } }
            if let Some(ref mut seq) = boot_seq { seq.on_pre_step(bus.as_mut(), mmu_regs_base, cpu.r.pc, &mut cpu); }
            if trace {
                let (len, text) = if disasm_hex { mc6809emu::disasm::disasm_one_hex(bus.as_mut(), cpu.r.pc) } else { disasm_one(bus.as_mut(), cpu.r.pc) };
                println!("{:04X}: {}", cpu.r.pc, text);
            }
            cpu.step(bus.as_mut(), false);
            if let Some(ref mut seq) = boot_seq { seq.on_post_step(bus.as_mut(), mmu_regs_base); }
            remaining = remaining.saturating_sub(1);
            if !breakpoints.is_empty() && breakpoints.contains(&cpu.r.pc) {
                println!("Breakpoint hit at {:04X}", cpu.r.pc);
                break;
            }
        }
    }

    if print_cycles {
        println!("cycles: {}", cpu.cycles);
    }
}
