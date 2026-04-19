use std::env;

use em6809::loader::ImageFormat;

pub fn print_usage() {
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
    println!("  --console-mc6850          Use MC6850 ACIA register layout (status@+0, data@+1)");
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

#[allow(clippy::struct_excessive_bools)]
pub struct CliOptions {
    pub force_in_app_settings: bool,
    pub image_path: Option<String>,
    pub format: Option<ImageFormat>,
    pub base: Option<u16>,
    pub force_pc: Option<u16>,
    pub paused_start: bool,
    pub print_cycles: bool,
    pub use_mmu: bool,
    pub cli_protect_vectors: bool,
    pub mmu_phys: usize,
    pub mmu_regs_base: u16,
    pub mmu_task: u8,
    pub mmu_idmap: bool,
    pub mmu_log_maps: bool,
    pub console_base: Option<u16>,
    pub cli_console_enable: bool,
    pub cli_console_base: bool,
    /// Console device flavour: `false` = simple ConsoleDev (data@+0, status@+1);
    /// `true` = MC6850 ACIA (status@+0, data@+1).  Needed for programs
    /// targeting the Motorola MC6850 register layout (Hha Forth / Lisp,
    /// MVME147 boot, NetBSD, etc.).
    pub console_mc6850: bool,
    pub block_base: Option<u16>,
    pub cli_block_enable: bool,
    pub cli_block_base: bool,
    pub console_in: Option<String>,
    pub console_stdin: bool,
    pub cli_console_stdin: bool,
    pub block_file_path: Option<String>,
    pub cli_block_file: bool,
    pub block_irq_enable: bool,
    pub cli_block_irq: bool,
    pub block_firq: bool,
    pub cli_block_firq: bool,
    pub block_irq_hold: Option<u32>,
    pub cli_block_irq_hold: bool,
    pub gpio_base: Option<u16>,
    pub cli_gpio_enable: bool,
    pub cli_gpio_base: bool,
    pub gpio_bits: u8,
    pub cli_gpio_bits: bool,
    pub gpio_on_color: Option<u32>,
    pub cli_gpio_on_color: bool,
    pub mmu_maps: Vec<(usize, u16)>,
    pub mmu_map_file: Option<String>,
    pub preset_name: Option<String>,
    pub protect_vectors: bool,
    pub cli_mmu_enable: bool,
    pub cli_mmu_regs_base: bool,
    pub cli_mmu_phys: bool,
    pub cli_mmu_task: bool,
    pub cli_mmu_mode: bool,
    pub cli_mmu_prot: bool,
    pub cli_mmu_log_maps: bool,
    pub mmu_mode_sys: Option<bool>,
    pub mmu_prot: Option<u8>,
    pub mmu_attrs: Vec<(usize, u8)>,
    pub console_rx_wm: Option<usize>,
    pub cli_console_rx_wm: bool,
    pub console_irq_hold: Option<u32>,
    pub cli_console_irq_hold: bool,
    pub boot_script_path: Option<String>,
    pub console_out_path: Option<String>,
    pub cli_console_out: bool,
    pub console_tee_stderr: bool,
    pub cli_console_tee_stderr: bool,
    pub console_flush_every: Option<usize>,
    pub cli_console_flush_every: bool,
    pub console_flush_newline: bool,
    pub cli_console_flush_newline: bool,
    pub console_local_echo: bool,
    pub cli_console_echo: bool,
    pub emit_boot_template_name: Option<String>,
    pub irq_log: bool,
    pub gpio_log: bool,
    pub timer_base: Option<u16>,
    pub cli_timer_enable: bool,
    pub cli_timer_base: bool,
    pub timer_reload: Option<u16>,
    pub cli_timer_reload: bool,
    pub timer_rate_hz: Option<u32>,
    pub timer_ips_est: u64,
    pub cli_timer_ips: bool,
    pub timer_irq: bool,
    pub cli_timer_irq: bool,
    pub timer_firq: bool,
    pub cli_timer_firq: bool,
    pub timer_start: bool,
    pub cli_timer_start: bool,
    pub debug_console_log: bool,
    pub debug_block_log: bool,
    pub debug_gpio_log: bool,
    pub debug_mmu_log: bool,
    pub debug_settings_log: bool,
    pub debug_show_settings_origins: bool,
}

pub fn parse_args() -> Option<CliOptions> {
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
            return None;
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
    let mut paused_start: bool = true;
    let mut print_cycles = false;
    let mut use_mmu = false;
    let mut cli_protect_vectors = false;
    let mut mmu_phys: usize = 1 << 20;
    let mut mmu_regs_base: u16 = 0xFFA0;
    let mut mmu_task: u8 = 0;
    let mut mmu_idmap = false;
    let mut mmu_log_maps = false;
    let mut console_base: Option<u16> = None;
    let mut cli_console_enable = false;
    let mut cli_console_base = false;
    let mut console_mc6850 = false;
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
    let mut console_local_echo: bool = false;
    let mut cli_console_echo = false;
    let mut emit_boot_template_name: Option<String> = None;
    let mut irq_log: bool = false;
    let mut gpio_log: bool = false;
    let mut timer_base: Option<u16> = None;
    let mut cli_timer_enable = false;
    let mut cli_timer_base = false;
    let mut timer_reload: Option<u16> = None;
    let mut cli_timer_reload = false;
    let mut timer_rate_hz: Option<u32> = None;
    let mut timer_ips_est: u64 = 1_000_000;
    let mut cli_timer_ips = false;
    let mut timer_irq: bool = false;
    let mut cli_timer_irq = false;
    let mut timer_firq: bool = false;
    let mut cli_timer_firq = false;
    let mut timer_start: bool = false;
    let mut cli_timer_start = false;
    let debug_console_log: bool = false;
    let debug_block_log: bool = false;
    let debug_gpio_log: bool = false;
    let debug_mmu_log: bool = false;
    let debug_settings_log: bool = false;
    let debug_show_settings_origins: bool = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => {
                if let Some(f) = args.next() {
                    format = match f.as_str() {
                        "bin" => Some(ImageFormat::Binary),
                        "srec" => Some(ImageFormat::Srec),
                        other => {
                            eprintln!("Unknown format: {other}");
                            return None;
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
            "--paused" => {}
            "--dump-regs" => {}
            "--dump-mem" => {
                let _ = (args.next(), args.next());
            }
            "--dump-mem-ascii" => {
                let _ = (args.next(), args.next());
            }
            "--disasm" => {}
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
            "--console-mc6850" => {
                console_mc6850 = true;
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
                return None;
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
                return None;
            }
        }
    }

    Some(CliOptions {
        force_in_app_settings,
        image_path,
        format,
        base,
        force_pc,
        paused_start,
        print_cycles,
        use_mmu,
        cli_protect_vectors,
        mmu_phys,
        mmu_regs_base,
        mmu_task,
        mmu_idmap,
        mmu_log_maps,
        console_base,
        cli_console_enable,
        cli_console_base,
        console_mc6850,
        block_base,
        cli_block_enable,
        cli_block_base,
        console_in,
        console_stdin,
        cli_console_stdin,
        block_file_path,
        cli_block_file,
        block_irq_enable,
        cli_block_irq,
        block_firq,
        cli_block_firq,
        block_irq_hold,
        cli_block_irq_hold,
        gpio_base,
        cli_gpio_enable,
        cli_gpio_base,
        gpio_bits,
        cli_gpio_bits,
        gpio_on_color,
        cli_gpio_on_color,
        mmu_maps,
        mmu_map_file,
        preset_name,
        protect_vectors,
        cli_mmu_enable,
        cli_mmu_regs_base,
        cli_mmu_phys,
        cli_mmu_task,
        cli_mmu_mode,
        cli_mmu_prot,
        cli_mmu_log_maps,
        mmu_mode_sys,
        mmu_prot,
        mmu_attrs,
        console_rx_wm,
        cli_console_rx_wm,
        console_irq_hold,
        cli_console_irq_hold,
        boot_script_path,
        console_out_path,
        cli_console_out,
        console_tee_stderr,
        cli_console_tee_stderr,
        console_flush_every,
        cli_console_flush_every,
        console_flush_newline,
        cli_console_flush_newline,
        console_local_echo,
        cli_console_echo,
        emit_boot_template_name,
        irq_log,
        gpio_log,
        timer_base,
        cli_timer_enable,
        cli_timer_base,
        timer_reload,
        cli_timer_reload,
        timer_rate_hz,
        timer_ips_est,
        cli_timer_ips,
        timer_irq,
        cli_timer_irq,
        timer_firq,
        cli_timer_firq,
        timer_start,
        cli_timer_start,
        debug_console_log,
        debug_block_log,
        debug_gpio_log,
        debug_mmu_log,
        debug_settings_log,
        debug_show_settings_origins,
    })
}
