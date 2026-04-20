use em6809::bus::{self, Bus};
use em6809::config::{apply_mmu_config_from_str, apply_preset};
use em6809::io::{BlockDev, ConsoleDev, GpioDev, IoBus, Mc6850Dev};
use em6809::mmu::Mc6829;
use em6809::timer::TimerDev;

// Both ConsoleDev and Mc6850Dev expose the same setter API, so a single
// macro configures either one from the same options.  Used at both the
// MMU and the no-MMU IoBus attachment sites to pick the device flavour
// at runtime from `opts.console_mc6850`.
macro_rules! attach_console_device {
    ($iobus:expr, $opts:expr, $base:expr) => {{
        fn configure<D>(dev: &mut D, opts: &BusOptions)
        where
            D: ConsoleSetters,
        {
            if let Some(wm) = opts.console_rx_wm {
                dev.set_rx_watermark(wm);
            }
            if let Some(h) = opts.console_irq_hold {
                dev.set_irq_hold_cycles(h);
            }
            if let Some(s) = &opts.console_in {
                let bytes = parse_escapes(s);
                dev.feed_bytes(&bytes);
            }
            if let Some(p) = &opts.console_out_path {
                dev.set_out_file(p);
            }
            if opts.console_tee_stderr {
                dev.set_tee_stderr(true);
            }
            if let Some(n) = opts.console_flush_every {
                dev.set_flush_every(n);
            }
            if opts.console_flush_newline {
                dev.set_flush_on_newline(true);
            }
            if opts.console_local_echo {
                dev.set_local_echo(true);
            }
        }
        if $opts.console_mc6850 {
            let mut con = Mc6850Dev::new($base);
            configure(&mut con, $opts);
            $iobus.add_device(con);
        } else {
            let mut con = ConsoleDev::new($base);
            configure(&mut con, $opts);
            $iobus.add_device(con);
        }
    }};
}

// Small setter trait satisfied by both ConsoleDev and Mc6850Dev.  Not
// exposed as a public interface — it exists only so the macro above can
// share one `configure` helper.
trait ConsoleSetters {
    fn set_rx_watermark(&mut self, wm: usize);
    fn set_irq_hold_cycles(&mut self, n: u32);
    fn feed_bytes(&mut self, bytes: &[u8]);
    fn set_out_file(&mut self, path: &str);
    fn set_tee_stderr(&mut self, on: bool);
    fn set_flush_every(&mut self, n: usize);
    fn set_flush_on_newline(&mut self, on: bool);
    fn set_local_echo(&mut self, on: bool);
}

impl ConsoleSetters for ConsoleDev {
    fn set_rx_watermark(&mut self, wm: usize) {
        ConsoleDev::set_rx_watermark(self, wm)
    }
    fn set_irq_hold_cycles(&mut self, n: u32) {
        ConsoleDev::set_irq_hold_cycles(self, n)
    }
    fn feed_bytes(&mut self, b: &[u8]) {
        ConsoleDev::feed_bytes(self, b)
    }
    fn set_out_file(&mut self, p: &str) {
        ConsoleDev::set_out_file(self, p)
    }
    fn set_tee_stderr(&mut self, on: bool) {
        ConsoleDev::set_tee_stderr(self, on)
    }
    fn set_flush_every(&mut self, n: usize) {
        ConsoleDev::set_flush_every(self, n)
    }
    fn set_flush_on_newline(&mut self, on: bool) {
        ConsoleDev::set_flush_on_newline(self, on)
    }
    fn set_local_echo(&mut self, on: bool) {
        ConsoleDev::set_local_echo(self, on)
    }
}

impl ConsoleSetters for Mc6850Dev {
    fn set_rx_watermark(&mut self, wm: usize) {
        Mc6850Dev::set_rx_watermark(self, wm)
    }
    fn set_irq_hold_cycles(&mut self, n: u32) {
        Mc6850Dev::set_irq_hold_cycles(self, n)
    }
    fn feed_bytes(&mut self, b: &[u8]) {
        Mc6850Dev::feed_bytes(self, b)
    }
    fn set_out_file(&mut self, p: &str) {
        Mc6850Dev::set_out_file(self, p)
    }
    fn set_tee_stderr(&mut self, on: bool) {
        Mc6850Dev::set_tee_stderr(self, on)
    }
    fn set_flush_every(&mut self, n: usize) {
        Mc6850Dev::set_flush_every(self, n)
    }
    fn set_flush_on_newline(&mut self, on: bool) {
        Mc6850Dev::set_flush_on_newline(self, on)
    }
    fn set_local_echo(&mut self, on: bool) {
        Mc6850Dev::set_local_echo(self, on)
    }
}

pub struct BusOptions {
    pub use_mmu: bool,
    pub mmu_phys: usize,
    pub mmu_regs_base: u16,
    pub mmu_task: u8,
    pub mmu_idmap: bool,
    pub mmu_log_maps: bool,
    pub mmu_maps_sys: Vec<(usize, u16)>,
    pub mmu_maps_user: Vec<(usize, u16)>,
    pub mmu_map_file: Option<String>,
    pub preset_name: Option<String>,
    pub mmu_mode_sys: Option<bool>,
    pub mmu_prot: Option<u8>,
    pub mmu_attrs_sys: Vec<(usize, u8)>,
    pub mmu_attrs_user: Vec<(usize, u8)>,
    pub protect_vectors: bool,
    pub console_base: Option<u16>,
    pub console_mc6850: bool,
    pub console_rx_wm: Option<usize>,
    pub console_irq_hold: Option<u32>,
    pub console_in: Option<String>,
    pub console_out_path: Option<String>,
    pub console_flush_every: Option<usize>,
    pub console_flush_newline: bool,
    pub console_tee_stderr: bool,
    pub console_local_echo: bool,
    pub block_base: Option<u16>,
    pub block_file_path: Option<String>,
    pub block_irq_enable: bool,
    pub block_firq: bool,
    pub block_irq_hold: Option<u32>,
    pub gpio_base: Option<u16>,
    pub gpio_bits: u8,
    pub timer_base: Option<u16>,
    pub timer_reload: Option<u16>,
    pub timer_rate_hz: Option<u32>,
    pub timer_ips_est: u64,
    pub timer_irq: bool,
    pub timer_firq: bool,
    pub timer_start: bool,
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

pub fn build_bus(mut opts: BusOptions) -> Option<(Box<dyn Bus>, Option<u16>)> {
    // Choose memory backend and build a Bus object
    let bus: Box<dyn bus::Bus> = if opts.use_mmu {
        let mut mmu = Mc6829::new(opts.mmu_phys, opts.mmu_regs_base);
        if opts.mmu_log_maps {
            mmu.set_log_maps(true);
        }
        mmu.set_task(opts.mmu_task);
        if opts.mmu_idmap {
            mmu.identity_map_current();
        }
        mmu.write8(mmu.regs_base + 0x11, 0x01);
        for (p, f) in &opts.mmu_maps_sys {
            mmu.set_map_entry(*p, *f);
        }
        for (pg, val) in &opts.mmu_attrs_sys {
            mmu.write8(mmu.regs_base + 0x18 + (*pg as u16), *val);
        }
        mmu.write8(mmu.regs_base + 0x11, 0x00);
        for (p, f) in &opts.mmu_maps_user {
            mmu.set_map_entry(*p, *f);
        }
        for (pg, val) in &opts.mmu_attrs_user {
            mmu.write8(mmu.regs_base + 0x18 + (*pg as u16), *val);
        }
        if let Some(path) = &opts.mmu_map_file {
            match std::fs::read_to_string(path) {
                Ok(s) => {
                    if let Err(e) = apply_mmu_config_from_str(&mut mmu, &s) {
                        eprintln!("MMU map file error: {e}");
                        return None;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read {path}: {e}");
                    return None;
                }
            }
        }
        if let Some(pname) = &opts.preset_name {
            if let Err(e) = apply_preset(&mut mmu, pname) {
                eprintln!("Preset error: {e}");
                return None;
            }
            if pname.to_lowercase().starts_with("os9l2") && opts.console_base.is_none() {
                opts.console_base = Some(0xFF00);
            }
        }
        if let Some(sys) = opts.mmu_mode_sys {
            if sys {
                mmu.write8(mmu.regs_base + 0x11, 0x01);
            } else {
                mmu.write8(mmu.regs_base + 0x11, 0x00);
            }
        }
        if let Some(p) = opts.mmu_prot {
            mmu.write8(mmu.regs_base + 0x12, p);
        }
        if opts.protect_vectors {
            let prev_mode = mmu.read8(mmu.regs_base + 0x11) & 0x01;
            mmu.write8(mmu.regs_base + 0x12, 0x03);
            mmu.write8(mmu.regs_base + 0x11, 0x01);
            mmu.write8(mmu.regs_base + 0x18 + 0x0F, 0x01);
            mmu.write8(mmu.regs_base + 0x11, 0x00);
            mmu.write8(mmu.regs_base + 0x18 + 0x0F, 0x01);
            mmu.write8(mmu.regs_base + 0x11, prev_mode);
        }
        let need_iobus = opts.console_base.is_some()
            || opts.block_base.is_some()
            || opts.gpio_base.is_some()
            || opts.timer_base.is_some();
        if need_iobus {
            let mut iobus = IoBus::new(mmu);
            if let Some(base) = opts.console_base {
                attach_console_device!(iobus, &opts, base);
            }
            if let Some(base) = opts.block_base {
                let mut blk = BlockDev::new(base);
                if let Some(p) = &opts.block_file_path {
                    blk.set_backing_file(p);
                }
                if let Some(n) = opts.block_irq_hold {
                    blk.set_irq_hold_cycles(n);
                }
                if opts.block_irq_enable {
                    blk.set_irq_enable(true);
                }
                if opts.block_firq {
                    blk.set_firq(true);
                }
                iobus.add_device(blk);
            }
            if let Some(base) = opts.gpio_base {
                let gpio = GpioDev::new(base, opts.gpio_bits);
                iobus.add_device(gpio);
            }
            if let Some(base) = opts.timer_base {
                let mut t = TimerDev::new(base);
                if let Some(r) = opts.timer_reload {
                    t.set_reload(r);
                } else if let Some(hz) = opts.timer_rate_hz {
                    let ticks = (opts.timer_ips_est / (hz as u64).max(1)).max(1);
                    let ticks16 = u16::try_from(ticks).unwrap_or(u16::MAX);
                    t.set_reload(ticks16.max(1));
                }
                if opts.timer_irq {
                    t.set_irq_enable(true);
                }
                if opts.timer_firq {
                    t.set_firq(true);
                }
                if opts.timer_start {
                    t.start();
                }
                iobus.add_device(t);
            }
            Box::new(iobus)
        } else {
            Box::new(mmu)
        }
    } else {
        let mem = bus::Memory::new();
        // For the no-MMU path, optionally wrap in IoBus if devices requested
        let need_iobus = opts.console_base.is_some()
            || opts.block_base.is_some()
            || opts.gpio_base.is_some()
            || opts.timer_base.is_some();
        if need_iobus {
            let mut iobus = IoBus::new(mem);
            if let Some(base) = opts.console_base {
                attach_console_device!(iobus, &opts, base);
            }
            if let Some(base) = opts.block_base {
                let mut blk = BlockDev::new(base);
                if let Some(p) = &opts.block_file_path {
                    blk.set_backing_file(p);
                }
                if let Some(n) = opts.block_irq_hold {
                    blk.set_irq_hold_cycles(n);
                }
                if opts.block_irq_enable {
                    blk.set_irq_enable(true);
                }
                if opts.block_firq {
                    blk.set_firq(true);
                }
                iobus.add_device(blk);
            }
            if let Some(base) = opts.gpio_base {
                let gpio = GpioDev::new(base, opts.gpio_bits);
                iobus.add_device(gpio);
            }
            if let Some(base) = opts.timer_base {
                let mut t = TimerDev::new(base);
                if let Some(r) = opts.timer_reload {
                    t.set_reload(r);
                } else if let Some(hz) = opts.timer_rate_hz {
                    let ticks = (opts.timer_ips_est / (hz as u64).max(1)).max(1);
                    let ticks16 = u16::try_from(ticks).unwrap_or(u16::MAX);
                    t.set_reload(ticks16.max(1));
                }
                if opts.timer_irq {
                    t.set_irq_enable(true);
                }
                if opts.timer_firq {
                    t.set_firq(true);
                }
                if opts.timer_start {
                    t.start();
                }
                iobus.add_device(t);
            }
            Box::new(iobus)
        } else {
            Box::new(mem)
        }
    };

    Some((bus, opts.console_base))
}
