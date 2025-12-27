// Use library modules instead of duplicating

use std::env;
use std::fs;
use std::path::Path;

use em6809::bootscript::{emit_boot_template, parse_boot_script, BootSequencer};
use em6809::bus::Bus;
use em6809::bus::{self, Memory};
use em6809::cpu::Cpu;
use em6809::io::IoBus;
use em6809::loader::{self, ImageFormat, LoadedImage};
use em6809::mmu::Mc6829;
use em6809::prefs;
mod cli;
mod devices;
mod settings_native;
mod settings_runtime;
mod settings_worker;

// GUI register viewer (no-op unless built with `--features gui`)
mod i18n;
mod loader_util;
mod regview;

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

fn clear_logical_memory(bus: &mut dyn Bus) {
    if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        let inner = io_mmu.inner_any_mut();
        if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
            mmu.clear_physical(0);
            return;
        }
        if let Some(mem) = inner.downcast_mut::<Memory>() {
            mem.clear(0);
            return;
        }
    } else if let Some(mmu) = bus.as_any_mut().downcast_mut::<Mc6829>() {
        mmu.clear_physical(0);
        return;
    } else if let Some(mem) = bus.as_any_mut().downcast_mut::<Memory>() {
        mem.clear(0);
        return;
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner = wt.inner_any_mut();
        if let Some(io_mmu) = inner.downcast_mut::<IoBus<Mc6829>>() {
            let inner2 = io_mmu.inner_any_mut();
            if let Some(mmu) = inner2.downcast_mut::<Mc6829>() {
                mmu.clear_physical(0);
                return;
            }
            if let Some(mem) = inner2.downcast_mut::<Memory>() {
                mem.clear(0);
                return;
            }
        } else if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
            mmu.clear_physical(0);
            return;
        } else if let Some(mem) = inner.downcast_mut::<Memory>() {
            mem.clear(0);
            return;
        }
    }
    for addr in 0u32..=0xFFFF {
        bus.write8(addr as u16, 0);
    }
}

fn main() {
    // Initialize i18n (Fluent) from environment/OS locale
    i18n::init_from_env();
    let cli_opts = match cli::parse_args() {
        Some(opts) => opts,
        None => return,
    };

    #[allow(unused_mut)]
    let cli::CliOptions {
        mut force_in_app_settings,
        mut image_path,
        mut format,
        mut base,
        mut force_pc,
        mut paused_start,
        mut print_cycles,
        mut use_mmu,
        mut cli_protect_vectors,
        mut mmu_phys,
        mut mmu_regs_base,
        mut mmu_task,
        mut mmu_idmap,
        mut mmu_log_maps,
        mut console_base,
        mut cli_console_enable,
        mut cli_console_base,
        mut block_base,
        mut cli_block_enable,
        mut cli_block_base,
        mut console_in,
        mut console_stdin,
        mut cli_console_stdin,
        mut block_file_path,
        mut cli_block_file,
        mut block_irq_enable,
        mut cli_block_irq,
        mut block_firq,
        mut cli_block_firq,
        mut block_irq_hold,
        mut cli_block_irq_hold,
        mut gpio_base,
        mut cli_gpio_enable,
        mut cli_gpio_base,
        mut gpio_bits,
        mut cli_gpio_bits,
        mut gpio_on_color,
        mut cli_gpio_on_color,
        mut mmu_maps,
        mut mmu_map_file,
        mut preset_name,
        mut protect_vectors,
        mut cli_mmu_enable,
        mut cli_mmu_regs_base,
        mut cli_mmu_phys,
        mut cli_mmu_task,
        mut cli_mmu_mode,
        mut cli_mmu_prot,
        mut cli_mmu_log_maps,
        mut mmu_mode_sys,
        mut mmu_prot,
        mut mmu_attrs,
        mut console_rx_wm,
        mut cli_console_rx_wm,
        mut console_irq_hold,
        mut cli_console_irq_hold,
        mut boot_script_path,
        mut console_out_path,
        mut cli_console_out,
        mut console_tee_stderr,
        mut cli_console_tee_stderr,
        mut console_flush_every,
        mut cli_console_flush_every,
        mut console_flush_newline,
        mut cli_console_flush_newline,
        mut console_local_echo,
        mut cli_console_echo,
        mut emit_boot_template_name,
        mut irq_log,
        mut gpio_log,
        mut timer_base,
        mut cli_timer_enable,
        mut cli_timer_base,
        mut timer_reload,
        mut cli_timer_reload,
        mut timer_rate_hz,
        mut timer_ips_est,
        mut cli_timer_ips,
        mut timer_irq,
        mut cli_timer_irq,
        mut timer_firq,
        mut cli_timer_firq,
        mut timer_start,
        mut cli_timer_start,
        mut debug_console_log,
        mut debug_block_log,
        mut debug_gpio_log,
        mut debug_mmu_log,
        mut debug_settings_log,
        mut debug_show_settings_origins,
    } = cli_opts;

    let mut mmu_maps_sys: Vec<(usize, u16)> = Vec::new();
    let mut mmu_maps_user: Vec<(usize, u16)> = Vec::new();
    let mut mmu_attrs_sys: Vec<(usize, u8)> = Vec::new();
    let mut mmu_attrs_user: Vec<(usize, u8)> = Vec::new();

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
        cli::print_usage();
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
        if mmu_maps_sys.is_empty() {
            if let Some(frames) = &m.map_frames_sys {
                for (i, f) in frames {
                    mmu_maps_sys.push((*i, *f));
                }
            }
        }
        if mmu_maps_user.is_empty() {
            if let Some(frames) = &m.map_frames_user {
                for (i, f) in frames {
                    mmu_maps_user.push((*i, *f));
                }
            }
        }
        if mmu_attrs_sys.is_empty() {
            if let Some(attrs) = &m.attr_bytes_sys {
                for (i, a) in attrs {
                    mmu_attrs_sys.push((*i, *a));
                }
            }
        }
        if mmu_attrs_user.is_empty() {
            if let Some(attrs) = &m.attr_bytes_user {
                for (i, a) in attrs {
                    mmu_attrs_user.push((*i, *a));
                }
            }
        }
        if mmu_maps_sys.is_empty() && mmu_maps_user.is_empty() {
            if let Some(frames) = &m.map_frames {
                for (i, f) in frames {
                    mmu_maps_sys.push((*i, *f));
                    mmu_maps_user.push((*i, *f));
                }
            }
        }
        if mmu_attrs_sys.is_empty() && mmu_attrs_user.is_empty() {
            if let Some(attrs) = &m.attr_bytes {
                for (i, a) in attrs {
                    mmu_attrs_sys.push((*i, *a));
                    mmu_attrs_user.push((*i, *a));
                }
            }
        }
        if preset_name.is_none() {
            preset_name = m.preset_name.clone();
        }
    }
    if !mmu_maps.is_empty() {
        mmu_maps_sys = mmu_maps.clone();
        mmu_maps_user = mmu_maps.clone();
    }
    if !mmu_attrs.is_empty() {
        mmu_attrs_sys = mmu_attrs.clone();
        mmu_attrs_user = mmu_attrs.clone();
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

    let bus_opts = devices::BusOptions {
        use_mmu,
        mmu_phys,
        mmu_regs_base,
        mmu_task,
        mmu_idmap,
        mmu_log_maps,
        mmu_maps_sys: mmu_maps_sys.clone(),
        mmu_maps_user: mmu_maps_user.clone(),
        mmu_map_file: mmu_map_file.clone(),
        preset_name: preset_name.clone(),
        mmu_mode_sys,
        mmu_prot,
        mmu_attrs_sys: mmu_attrs_sys.clone(),
        mmu_attrs_user: mmu_attrs_user.clone(),
        protect_vectors,
        console_base,
        console_rx_wm,
        console_irq_hold,
        console_in: console_in.clone(),
        console_out_path: console_out_path.clone(),
        console_flush_every,
        console_flush_newline,
        console_tee_stderr,
        console_local_echo,
        block_base,
        block_file_path: block_file_path.clone(),
        block_irq_enable,
        block_firq,
        block_irq_hold,
        gpio_base,
        gpio_bits,
        timer_base,
        timer_reload,
        timer_rate_hz,
        timer_ips_est,
        timer_irq,
        timer_firq,
        timer_start,
    };
    let (mut bus, console_base) = match devices::build_bus(bus_opts) {
        Some(res) => res,
        None => return,
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
    } else if let Some(entry) = loaded.entry.and_then(|e| {
        if e == 0x0000
            && !loaded
                .loaded_ranges
                .iter()
                .any(|(s, e2)| *s <= e && e < *e2)
        {
            None
        } else {
            Some(e)
        }
    }) {
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
        let spans = loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
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
        let spans = loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
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
        s.mmu_edit_sys = s.mmu_mode_sys;
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
        for (i, f) in &mmu_maps_sys {
            s.mmu_map_frames_sys[*i] = format!("0x{:02X}", *f & 0x00FF);
        }
        for (i, f) in &mmu_maps_user {
            s.mmu_map_frames_user[*i] = format!("0x{:02X}", *f & 0x00FF);
        }
        for (i, a) in &mmu_attrs_sys {
            s.mmu_attr_bytes_sys[*i] = format!("0x{:02X}", *a);
        }
        for (i, a) in &mmu_attrs_user {
            s.mmu_attr_bytes_user[*i] = format!("0x{:02X}", *a);
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
        let _ = settings_runtime::apply_settings_runtime(Some(viewer), bus.as_mut(), &payload);
        viewer.set_settings_baseline(payload.settings.clone(), origins, gpio_on_color);
    }

    // Initial GUI sync: push current MMU map and GPIO state so the panels show immediately
    if let Some(viewer) = &reg_viewer {
        // MMU snapshot from current bus
        if let Some(io_mmu) = bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
            let inner = io_mmu.inner_any_mut();
            if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                let (map_sys, map_user, sys, task) = mmu.snapshot_maps();
                viewer.sync_mmu(sys, task, map_sys, map_user);
            }
        } else if let Some(mmu) = bus.as_mut().as_any_mut().downcast_mut::<Mc6829>() {
            let (map_sys, map_user, sys, task) = mmu.snapshot_maps();
            viewer.sync_mmu(sys, task, map_sys, map_user);
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
        settings_runtime::sync_block_from_bus(bus.as_mut(), viewer);
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
            let spawn_cpu = move || {
                thread::spawn(move || {
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
                                        viewer_cpu.set_memory_window(
                                            cpu_worker.r.pc,
                                            None,
                                            Some(true),
                                        );
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
                                        let spans = loader_util::coalesce_ranges(
                                            loaded_ranges_worker.clone(),
                                        );
                                        let (policy, _win_len) = viewer_cpu.get_span_policy();
                                        let span_for_pc = spans
                                            .iter()
                                            .find(|(s, e)| {
                                                *s <= cpu_worker.r.pc && cpu_worker.r.pc < *e
                                            })
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
                                            let (consumed, text) = em6809::disasm::disasm_one_hex(
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
                                                // Clear memory without clobbering MMU regs
                                                clear_logical_memory(bus_worker.as_mut());
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
                                                    new_pc = new_loaded.entry.and_then(|e| {
                                                        if e == 0x0000
                                                            && !new_loaded.loaded_ranges.iter().any(
                                                                |(s, e2)| *s <= e && e < *e2,
                                                            )
                                                        {
                                                            None
                                                        } else {
                                                            Some(e)
                                                        }
                                                    });
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
                                                    // Align memory pane to program start without changing follow state.
                                                    viewer_cpu.set_memory_window(pc, None, None);
                                                }
                                                loader_util::log_program_info(
                                                    &new_loaded,
                                                    cpu_worker.r.pc,
                                                );
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
                                                let spans = loader_util::coalesce_ranges(
                                                    loaded.loaded_ranges.clone(),
                                                );
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
                                                settings_runtime::sync_gpio_from_bus(
                                                    bus_worker.as_mut(),
                                                    &viewer_cpu,
                                                );
                                                settings_runtime::sync_block_from_bus(
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
                                        let spans = loader_util::coalesce_ranges(
                                            loaded_ranges_worker.clone(),
                                        );
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
                                                let (map_sys, map_user, sys, task) =
                                                    mmu.snapshot_maps();
                                                if let Some(v) = viewer {
                                                    v.sync_mmu(sys, task, map_sys, map_user);
                                                }
                                                if let Some((mut s, _origins, color_opt)) =
                                                    regview::get_latest_settings_baseline()
                                                {
                                                    s.use_mmu = true;
                                                    s.mmu_mode_sys = sys;
                                                    s.mmu_edit_sys = sys;
                                                    s.mmu_task = format!("{}", task & 0x07);
                                                    s.mmu_preset_name = name.clone();
                                                    let prot = mmu.read8(mmu.regs_base + 0x12);
                                                    s.mmu_prot_hex = format!("{:02X}", prot);
                                                    let map_view = if sys { &map_sys } else { &map_user };
                                                    let map_target = if sys {
                                                        &mut s.mmu_map_frames_sys
                                                    } else {
                                                        &mut s.mmu_map_frames_user
                                                    };
                                                    for (i, f) in map_view.iter().enumerate() {
                                                        map_target[i] = format!("{:02X}", f & 0x00FF);
                                                    }
                                                    let attr_target = if sys {
                                                        &mut s.mmu_attr_bytes_sys
                                                    } else {
                                                        &mut s.mmu_attr_bytes_user
                                                    };
                                                    for i in 0..16 {
                                                        let attr = mmu
                                                            .read8(mmu.regs_base + 0x18 + i as u16);
                                                        attr_target[i] = format!("{:02X}", attr);
                                                    }
                                                    settings_runtime::publish_settings_baseline(
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
                                                let (map_sys, map_user, sys, task) =
                                                    mmu.snapshot_maps();
                                                if let Some(v) = viewer {
                                                    v.sync_mmu(sys, task, map_sys, map_user);
                                                }
                                                if let Some((mut s, _origins, color_opt)) =
                                                    regview::get_latest_settings_baseline()
                                                {
                                                    s.use_mmu = true;
                                                    s.mmu_mode_sys = sys;
                                                    s.mmu_edit_sys = sys;
                                                    s.mmu_task = format!("{}", task & 0x07);
                                                    s.mmu_preset_name.clear();
                                                    let prot = mmu.read8(mmu.regs_base + 0x12);
                                                    s.mmu_prot_hex = format!("{:02X}", prot);
                                                    let map_view = if sys { &map_sys } else { &map_user };
                                                    let map_target = if sys {
                                                        &mut s.mmu_map_frames_sys
                                                    } else {
                                                        &mut s.mmu_map_frames_user
                                                    };
                                                    for (i, f) in map_view.iter().enumerate() {
                                                        map_target[i] = format!("{:02X}", f & 0x00FF);
                                                    }
                                                    let attr_target = if sys {
                                                        &mut s.mmu_attr_bytes_sys
                                                    } else {
                                                        &mut s.mmu_attr_bytes_user
                                                    };
                                                    for i in 0..16 {
                                                        let attr = mmu
                                                            .read8(mmu.regs_base + 0x18 + i as u16);
                                                        attr_target[i] = format!("{:02X}", attr);
                                                    }
                                                    settings_runtime::publish_settings_baseline(
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
                                        maps_sys,
                                        maps_user,
                                        attrs_sys,
                                        attrs_user,
                                    } => {
                                        // Apply settings to whichever MMU backend is active
                                        let apply = |mmu: &mut Mc6829| {
                                            let prev_mode = mmu.read8(mmu.regs_base + 0x11) & 0x01;
                                            if let Some(on) = log_maps {
                                                mmu.set_log_maps(on);
                                            }
                                            if let Some(t) = task {
                                                mmu.write8(mmu.regs_base + 0x10, t & 0x07);
                                            }
                                            if let Some(p) = prot {
                                                mmu.write8(mmu.regs_base + 0x12, p);
                                            }
                                            mmu.write8(mmu.regs_base + 0x11, 0x01);
                                            for (pg, fr) in &maps_sys {
                                                mmu.set_map_entry(*pg, *fr);
                                            }
                                            for (pg, at) in &attrs_sys {
                                                let off = 0x18u16 + (*pg as u16);
                                                mmu.write8(mmu.regs_base + off, *at);
                                            }
                                            mmu.write8(mmu.regs_base + 0x11, 0x00);
                                            for (pg, fr) in &maps_user {
                                                mmu.set_map_entry(*pg, *fr);
                                            }
                                            for (pg, at) in &attrs_user {
                                                let off = 0x18u16 + (*pg as u16);
                                                mmu.write8(mmu.regs_base + off, *at);
                                            }
                                            if let Some(sys) = sys_mode {
                                                mmu.write8(
                                                    mmu.regs_base + 0x11,
                                                    if sys { 1 } else { 0 },
                                                );
                                            } else {
                                                mmu.write8(mmu.regs_base + 0x11, prev_mode);
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
                                        settings_runtime::sync_block_from_bus(
                                            bus_worker.as_mut(),
                                            &viewer_cpu,
                                        );
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
                                        if settings_runtime::apply_settings_runtime(
                                            Some(&viewer_cpu),
                                            bus_worker.as_mut(),
                                            &payload,
                                        ) {
                                            settings_runtime::persist_settings_snapshot(
                                                &payload.settings,
                                                payload.gpio_on_color,
                                            );
                                            settings_runtime::publish_settings_baseline(
                                                &payload.settings,
                                                payload.gpio_on_color,
                                            );
                                            settings_runtime::send_settings_ack(
                                                payload.instance_id,
                                            );
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
                                let (map_sys, map_user, sys, task) = mmu.snapshot_maps();
                                viewer_cpu.sync_mmu(sys, task, map_sys, map_user);
                            }
                        } else if let Some(mmu) =
                            bus_worker.as_mut().as_any_mut().downcast_mut::<Mc6829>()
                        {
                            let (map_sys, map_user, sys, task) = mmu.snapshot_maps();
                            viewer_cpu.sync_mmu(sys, task, map_sys, map_user);
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
                    settings_runtime::sync_gpio_from_bus(bus_worker.as_mut(), &viewer_cpu);
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
                            settings_runtime::sync_block_from_bus(bus_worker.as_mut(), &viewer_cpu);
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
                })
            };

            // Watch for Ctrl+C (SIGINT) and proactively request GUI close
            let viewer_close = viewer_main.clone();
            let _sig_watch = thread::spawn(move || {
                while !STOP_REQUESTED.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                viewer_close.request_close();
            });
            #[cfg(target_os = "macos")]
            {
                let cpu_handle_cell = std::sync::Arc::new(std::sync::Mutex::new(
                    None::<std::thread::JoinHandle<()>>,
                ));
                let cpu_handle_cell_init = cpu_handle_cell.clone();
                let init = Box::new(move || {
                    let handle = spawn_cpu();
                    if let Ok(mut guard) = cpu_handle_cell_init.lock() {
                        *guard = Some(handle);
                    }
                });
                // Run GUI on main thread (blocks until closed)
                viewer_main.run_blocking_with_init(
                    "em6809 - MC6809 Software Emulator",
                    Some(init),
                );
                STOP_REQUESTED.store(true, Ordering::SeqCst);
                if let Ok(mut guard) = cpu_handle_cell.lock() {
                    if let Some(handle) = guard.take() {
                        let _ = handle.join();
                    }
                }
                return;
            }
            #[cfg(all(not(windows), not(target_os = "macos")))]
            {
                let cpu_handle = spawn_cpu();
                // Run GUI on main thread (blocks until closed)
                viewer_main.run_blocking("em6809 - MC6809 Software Emulator");
                STOP_REQUESTED.store(true, Ordering::SeqCst);
                let _ = cpu_handle.join();
                return;
            }
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
                            if settings_runtime::apply_settings_runtime(
                                Some(viewer),
                                bus.as_mut(),
                                &payload,
                            ) {
                                settings_runtime::persist_settings_snapshot(
                                    &payload.settings,
                                    payload.gpio_on_color,
                                );
                                settings_runtime::publish_settings_baseline(
                                    &payload.settings,
                                    payload.gpio_on_color,
                                );
                                settings_runtime::send_settings_ack(payload.instance_id);
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
                            let spans = loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
                            let span_for_pc = spans
                                .iter()
                                .find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e)
                                .copied();
                            let (start, end, total_len_override) =
                                span_for_pc.map(|(s, e)| (s, e, None)).unwrap_or_else(|| {
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
                            let spans = loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
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
                                if settings_runtime::apply_settings_runtime(
                                    Some(viewer),
                                    bus.as_mut(),
                                    &payload,
                                ) {
                                    settings_runtime::persist_settings_snapshot(
                                        &payload.settings,
                                        payload.gpio_on_color,
                                    );
                                    settings_runtime::publish_settings_baseline(
                                        &payload.settings,
                                        payload.gpio_on_color,
                                    );
                                    settings_runtime::send_settings_ack(payload.instance_id);
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
                                let spans =
                                    loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
                                let span_for_pc = spans
                                    .iter()
                                    .find(|(s, e)| *s <= cpu.r.pc && cpu.r.pc < *e)
                                    .copied();
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
                                let spans =
                                    loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
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
                                let spans =
                                    loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
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
                                maps_sys,
                                maps_user,
                                attrs_sys,
                                attrs_user,
                            } => {
                                // Apply to active MMU if present; use separate scopes to avoid overlapping borrows
                                {
                                    if let Some(io_mmu) =
                                        bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                    {
                                        let inner = io_mmu.inner_any_mut();
                                        if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                            let prev_mode =
                                                mmu.read8(mmu.regs_base + 0x11) & 0x01;
                                            if let Some(on) = log_maps {
                                                mmu.set_log_maps(on);
                                            }
                                            if let Some(t) = task {
                                                mmu.write8(mmu.regs_base + 0x10, t & 0x07);
                                            }
                                            if let Some(p) = prot {
                                                mmu.write8(mmu.regs_base + 0x12, p);
                                            }
                                            mmu.write8(mmu.regs_base + 0x11, 0x01);
                                            for (pg, fr) in &maps_sys {
                                                mmu.set_map_entry(*pg, *fr);
                                            }
                                            for (pg, at) in &attrs_sys {
                                                let off = 0x18u16 + (*pg as u16);
                                                mmu.write8(mmu.regs_base + off, *at);
                                            }
                                            mmu.write8(mmu.regs_base + 0x11, 0x00);
                                            for (pg, fr) in &maps_user {
                                                mmu.set_map_entry(*pg, *fr);
                                            }
                                            for (pg, at) in &attrs_user {
                                                let off = 0x18u16 + (*pg as u16);
                                                mmu.write8(mmu.regs_base + off, *at);
                                            }
                                            if let Some(sys) = sys_mode {
                                                mmu.write8(
                                                    mmu.regs_base + 0x11,
                                                    if sys { 1 } else { 0 },
                                                );
                                            } else {
                                                mmu.write8(mmu.regs_base + 0x11, prev_mode);
                                            }
                                            continue;
                                        }
                                    }
                                }
                                {
                                    if let Some(mmu) =
                                        bus.as_mut().as_any_mut().downcast_mut::<Mc6829>()
                                    {
                                        let prev_mode = mmu.read8(mmu.regs_base + 0x11) & 0x01;
                                        if let Some(on) = log_maps {
                                            mmu.set_log_maps(on);
                                        }
                                        if let Some(t) = task {
                                            mmu.write8(mmu.regs_base + 0x10, t & 0x07);
                                        }
                                        if let Some(p) = prot {
                                            mmu.write8(mmu.regs_base + 0x12, p);
                                        }
                                        mmu.write8(mmu.regs_base + 0x11, 0x01);
                                        for (pg, fr) in maps_sys {
                                            mmu.set_map_entry(pg, fr);
                                        }
                                        for (pg, at) in attrs_sys {
                                            let off = 0x18u16 + (pg as u16);
                                            mmu.write8(mmu.regs_base + off, at);
                                        }
                                        mmu.write8(mmu.regs_base + 0x11, 0x00);
                                        for (pg, fr) in maps_user {
                                            mmu.set_map_entry(pg, fr);
                                        }
                                        for (pg, at) in attrs_user {
                                            let off = 0x18u16 + (pg as u16);
                                            mmu.write8(mmu.regs_base + off, at);
                                        }
                                        if let Some(sys) = sys_mode {
                                            mmu.write8(
                                                mmu.regs_base + 0x11,
                                                if sys { 1 } else { 0 },
                                            );
                                        } else {
                                            mmu.write8(mmu.regs_base + 0x11, prev_mode);
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
                                        // Clear memory without clobbering MMU regs
                                        clear_logical_memory(bus.as_mut());
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
                                            new_pc = new_loaded.entry.and_then(|e| {
                                                if e == 0x0000
                                                    && !new_loaded.loaded_ranges.iter().any(
                                                        |(s, e2)| *s <= e && e < *e2,
                                                    )
                                                {
                                                    None
                                                } else {
                                                    Some(e)
                                                }
                                            });
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
                                        loader_util::log_program_info(&new_loaded, cpu.r.pc);
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
                                        let spans = loader_util::coalesce_ranges(
                                            loaded.loaded_ranges.clone(),
                                        );
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
                                        settings_runtime::sync_gpio_from_bus(bus.as_mut(), viewer);
                                        settings_runtime::sync_block_from_bus(bus.as_mut(), viewer);
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
                                    let (map_sys, map_user, sys, task) = mmu.snapshot_maps();
                                    if let Some(v) = viewer {
                                        v.sync_mmu(sys, task, map_sys, map_user);
                                    }
                                    if let Some((mut s, _origins, color_opt)) =
                                        regview::get_latest_settings_baseline()
                                    {
                                        s.use_mmu = true;
                                        s.mmu_mode_sys = sys;
                                        s.mmu_edit_sys = sys;
                                        s.mmu_task = format!("{}", task & 0x07);
                                        s.mmu_preset_name = name.clone();
                                        let prot = mmu.read8(mmu.regs_base + 0x12);
                                        s.mmu_prot_hex = format!("{:02X}", prot);
                                        let map_view = if sys { &map_sys } else { &map_user };
                                        let map_target = if sys {
                                            &mut s.mmu_map_frames_sys
                                        } else {
                                            &mut s.mmu_map_frames_user
                                        };
                                        for (i, f) in map_view.iter().enumerate() {
                                            map_target[i] = format!("{:02X}", f & 0x00FF);
                                        }
                                        let attr_target = if sys {
                                            &mut s.mmu_attr_bytes_sys
                                        } else {
                                            &mut s.mmu_attr_bytes_user
                                        };
                                        for i in 0..16 {
                                            let attr = mmu.read8(mmu.regs_base + 0x18 + i as u16);
                                            attr_target[i] = format!("{:02X}", attr);
                                        }
                                        settings_runtime::publish_settings_baseline(
                                            &s,
                                            color_opt.unwrap_or(0),
                                        );
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
                                        let (map_sys, map_user, sys, task) = mmu.snapshot_maps();
                                        if let Some(v) = viewer {
                                            v.sync_mmu(sys, task, map_sys, map_user);
                                        }
                                        if let Some((mut s, _origins, color_opt)) =
                                            regview::get_latest_settings_baseline()
                                        {
                                            s.use_mmu = true;
                                            s.mmu_mode_sys = sys;
                                            s.mmu_edit_sys = sys;
                                            s.mmu_task = format!("{}", task & 0x07);
                                            s.mmu_preset_name.clear();
                                            let prot = mmu.read8(mmu.regs_base + 0x12);
                                            s.mmu_prot_hex = format!("{:02X}", prot);
                                            let map_view = if sys { &map_sys } else { &map_user };
                                            let map_target = if sys {
                                                &mut s.mmu_map_frames_sys
                                            } else {
                                                &mut s.mmu_map_frames_user
                                            };
                                            for (i, f) in map_view.iter().enumerate() {
                                                map_target[i] = format!("{:02X}", f & 0x00FF);
                                            }
                                            let attr_target = if sys {
                                                &mut s.mmu_attr_bytes_sys
                                            } else {
                                                &mut s.mmu_attr_bytes_user
                                            };
                                            for i in 0..16 {
                                                let attr =
                                                    mmu.read8(mmu.regs_base + 0x18 + i as u16);
                                                attr_target[i] = format!("{:02X}", attr);
                                            }
                                            settings_runtime::publish_settings_baseline(
                                                &s,
                                                color_opt.unwrap_or(0),
                                            );
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
                                settings_runtime::sync_block_from_bus(bus.as_mut(), viewer);
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
                            regview::Command::UiSetMmu(_, _, _, _) => {}
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
                            if settings_runtime::apply_settings_runtime(
                                Some(viewer),
                                bus.as_mut(),
                                &payload,
                            ) {
                                settings_runtime::persist_settings_snapshot(
                                    &payload.settings,
                                    payload.gpio_on_color,
                                );
                                settings_runtime::publish_settings_baseline(
                                    &payload.settings,
                                    payload.gpio_on_color,
                                );
                                settings_runtime::send_settings_ack(payload.instance_id);
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
                            let spans = loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
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
                            let spans = loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
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
                            let spans = loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
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
                            maps_sys,
                            maps_user,
                            attrs_sys,
                            attrs_user,
                        } => {
                            // Apply to active MMU on non-worker path; use scoped borrows
                            {
                                if let Some(io_mmu) =
                                    bus.as_mut().as_any_mut().downcast_mut::<IoBus<Mc6829>>()
                                {
                                    let inner = io_mmu.inner_any_mut();
                                    if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                                        let prev_mode =
                                            mmu.read8(mmu.regs_base + 0x11) & 0x01;
                                        if let Some(on) = log_maps {
                                            mmu.set_log_maps(on);
                                        }
                                        if let Some(t) = task {
                                            mmu.write8(mmu.regs_base + 0x10, t & 0x07);
                                        }
                                        if let Some(p) = prot {
                                            mmu.write8(mmu.regs_base + 0x12, p);
                                        }
                                        mmu.write8(mmu.regs_base + 0x11, 0x01);
                                        for (pg, fr) in &maps_sys {
                                            mmu.set_map_entry(*pg, *fr);
                                        }
                                        for (pg, at) in &attrs_sys {
                                            let off = 0x18u16 + (*pg as u16);
                                            mmu.write8(mmu.regs_base + off, *at);
                                        }
                                        mmu.write8(mmu.regs_base + 0x11, 0x00);
                                        for (pg, fr) in &maps_user {
                                            mmu.set_map_entry(*pg, *fr);
                                        }
                                        for (pg, at) in &attrs_user {
                                            let off = 0x18u16 + (*pg as u16);
                                            mmu.write8(mmu.regs_base + off, *at);
                                        }
                                        if let Some(sys) = sys_mode {
                                            mmu.write8(
                                                mmu.regs_base + 0x11,
                                                if sys { 1 } else { 0 },
                                            );
                                        } else {
                                            mmu.write8(mmu.regs_base + 0x11, prev_mode);
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
                                    let prev_mode = mmu.read8(mmu.regs_base + 0x11) & 0x01;
                                    if let Some(on) = log_maps {
                                        mmu.set_log_maps(on);
                                    }
                                    if let Some(t) = task {
                                        mmu.write8(mmu.regs_base + 0x10, t & 0x07);
                                    }
                                    if let Some(p) = prot {
                                        mmu.write8(mmu.regs_base + 0x12, p);
                                    }
                                    mmu.write8(mmu.regs_base + 0x11, 0x01);
                                    for (pg, fr) in maps_sys {
                                        mmu.set_map_entry(pg, fr);
                                    }
                                    for (pg, at) in attrs_sys {
                                        let off = 0x18u16 + (pg as u16);
                                        mmu.write8(mmu.regs_base + off, at);
                                    }
                                    mmu.write8(mmu.regs_base + 0x11, 0x00);
                                    for (pg, fr) in maps_user {
                                        mmu.set_map_entry(pg, fr);
                                    }
                                    for (pg, at) in attrs_user {
                                        let off = 0x18u16 + (pg as u16);
                                        mmu.write8(mmu.regs_base + off, at);
                                    }
                                    if let Some(sys) = sys_mode {
                                        mmu.write8(mmu.regs_base + 0x11, if sys { 1 } else { 0 });
                                    } else {
                                        mmu.write8(mmu.regs_base + 0x11, prev_mode);
                                    }
                                }
                            }
                        }
                        regview::Command::ApplyMmuPreset { name } => {
                            let apply = |mmu: &mut Mc6829, viewer: Option<&regview::Viewer>| {
                                let _ = em6809::config::apply_preset(mmu, &name);
                                let (map_sys, map_user, sys, task) = mmu.snapshot_maps();
                                if let Some(v) = viewer {
                                    v.sync_mmu(sys, task, map_sys, map_user);
                                }
                                if let Some((mut s, _origins, color_opt)) =
                                    regview::get_latest_settings_baseline()
                                {
                                    s.use_mmu = true;
                                    s.mmu_mode_sys = sys;
                                    s.mmu_edit_sys = sys;
                                    s.mmu_task = format!("{}", task & 0x07);
                                    s.mmu_preset_name = name.clone();
                                        let prot = mmu.read8(mmu.regs_base + 0x12);
                                        s.mmu_prot_hex = format!("{:02X}", prot);
                                        let map_view = if sys { &map_sys } else { &map_user };
                                        let map_target = if sys {
                                            &mut s.mmu_map_frames_sys
                                        } else {
                                            &mut s.mmu_map_frames_user
                                        };
                                        for (i, f) in map_view.iter().enumerate() {
                                            map_target[i] = format!("{:02X}", f & 0x00FF);
                                        }
                                    let attr_target = if sys {
                                        &mut s.mmu_attr_bytes_sys
                                    } else {
                                        &mut s.mmu_attr_bytes_user
                                    };
                                    for i in 0..16 {
                                        let attr = mmu.read8(mmu.regs_base + 0x18 + i as u16);
                                        attr_target[i] = format!("{:02X}", attr);
                                    }
                                    settings_runtime::publish_settings_baseline(
                                        &s,
                                        color_opt.unwrap_or(0),
                                    );
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
                                    let (map_sys, map_user, sys, task) = mmu.snapshot_maps();
                                    if let Some(v) = viewer {
                                        v.sync_mmu(sys, task, map_sys, map_user);
                                    }
                                    if let Some((mut s, _origins, color_opt)) =
                                        regview::get_latest_settings_baseline()
                                    {
                                        s.use_mmu = true;
                                        s.mmu_mode_sys = sys;
                                        s.mmu_edit_sys = sys;
                                        s.mmu_task = format!("{}", task & 0x07);
                                        s.mmu_preset_name.clear();
                                        let prot = mmu.read8(mmu.regs_base + 0x12);
                                        s.mmu_prot_hex = format!("{:02X}", prot);
                                        let map_view = if sys { &map_sys } else { &map_user };
                                        let map_target = if sys {
                                            &mut s.mmu_map_frames_sys
                                        } else {
                                            &mut s.mmu_map_frames_user
                                        };
                                        for (i, f) in map_view.iter().enumerate() {
                                            map_target[i] = format!("{:02X}", f & 0x00FF);
                                        }
                                        let attr_target = if sys {
                                            &mut s.mmu_attr_bytes_sys
                                        } else {
                                            &mut s.mmu_attr_bytes_user
                                        };
                                        for i in 0..16 {
                                            let attr = mmu.read8(mmu.regs_base + 0x18 + i as u16);
                                            attr_target[i] = format!("{:02X}", attr);
                                        }
                                        settings_runtime::publish_settings_baseline(
                                            &s,
                                            color_opt.unwrap_or(0),
                                        );
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
                            settings_runtime::sync_block_from_bus(bus.as_mut(), viewer);
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
                            let spans = loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
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
                            let spans = loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
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
                                settings_runtime::sync_gpio_from_bus(bus.as_mut(), viewer);
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
                                        settings_runtime::sync_block_from_bus(bus.as_mut(), viewer);
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
                        regview::Command::UiSetMmu(_, _, _, _) => {}
                        regview::Command::UiSetTimer(..) => {}
                        regview::Command::OpenImage { .. } => {}
                    }
                }
                // Event-driven recompute of HEX disassembly when tracked writes occurred
                if let Some(wt) = bus.as_mut().as_any_mut().downcast_mut::<bus::WriteTrack>() {
                    let (policy, win_len) = viewer.get_span_policy();
                    if policy == 0 {
                        // Full span tracking
                        let spans = loader_util::coalesce_ranges(loaded.loaded_ranges.clone());
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
            if settings_runtime::apply_settings_runtime(viewer, bus, &payload) {
                settings_runtime::publish_settings_baseline(
                    &payload.settings,
                    payload.gpio_on_color,
                );
            }
            // Always acknowledge, even if runtime apply fails, to unblock UI.
            settings_runtime::send_settings_ack(payload.instance_id);
        }
        settings_worker::WorkerCommand::SavePrefs(payload) => {
            if settings_runtime::apply_settings_runtime(viewer, bus, &payload) {
                settings_runtime::persist_settings_snapshot(
                    &payload.settings,
                    payload.gpio_on_color,
                );
                settings_runtime::publish_settings_baseline(
                    &payload.settings,
                    payload.gpio_on_color,
                );
            }
            // Always acknowledge, even if runtime apply/persist fails, to unblock UI.
            settings_runtime::send_settings_ack(payload.instance_id);
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
