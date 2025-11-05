use crate::bus::Bus;
use crate::io::IoBus;
// ConsoleDev referenced via IoBus helper; no direct use here
use crate::bus::Memory;
use crate::cpu::Cpu;
use crate::mmu::Mc6829;

#[derive(Clone, Debug)]
pub enum Action {
    Mode(bool),      // true=sys, false=user
    Prot(u8),        // write PROT_CTRL
    Map(usize, u16), // page -> frame
    Attr(usize, u8), // page attr
    ConCtrl(u8),     // console ctrl register
    ConRxWm(usize),  // console RX watermark
    ConIrqHold(u32), // console IRQ hold steps
    ConFirq(bool),   // console FIRQ preference
    IrqMask(bool),   // CPU I mask (true=set mask=disable IRQ)
    FirqMask(bool),  // CPU F mask (true=set mask=disable FIRQ)
    BlkIrq(bool),
    BlkFirq(bool),
    BlkIrqHold(u32),
}

#[derive(Clone, Debug)]
pub enum Trigger {
    OnPc(u16, Action),
    OnStep(u64, Action),
}

pub fn parse_boot_script(s: &str) -> Result<Vec<Trigger>, String> {
    let mut out: Vec<Trigger> = Vec::new();
    for (lineno, raw) in s.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(kind) = parts.next() else { continue };
        match kind {
            "on_pc" => {
                let Some(addr_s) = parts.next() else {
                    return Err(format!("Line {}: missing address", lineno + 1));
                };
                let addr = parse_hex_or_dec_u16(addr_s)
                    .ok_or_else(|| format!("Line {}: bad addr", lineno + 1))?;
                let Some(cmd) = parts.next() else {
                    return Err(format!("Line {}: missing command", lineno + 1));
                };
                let act = parse_action(cmd, &mut parts, lineno + 1)?;
                out.push(Trigger::OnPc(addr, act));
            }
            "on_step" => {
                let Some(n_s) = parts.next() else {
                    return Err(format!("Line {}: missing step", lineno + 1));
                };
                let n = parse_hex_or_dec_u64(n_s)
                    .ok_or_else(|| format!("Line {}: bad step", lineno + 1))?;
                let Some(cmd) = parts.next() else {
                    return Err(format!("Line {}: missing command", lineno + 1));
                };
                let act = parse_action(cmd, &mut parts, lineno + 1)?;
                out.push(Trigger::OnStep(n, act));
            }
            _ => return Err(format!("Line {}: unknown directive {}", lineno + 1, kind)),
        }
    }
    Ok(out)
}

fn parse_action<'a, I: Iterator<Item = &'a str>>(
    cmd: &str,
    parts: &mut I,
    lineno: usize,
) -> Result<Action, String> {
    match cmd {
        "mode" => {
            let Some(m) = parts.next() else {
                return Err(format!("Line {}: missing mode", lineno));
            };
            Ok(Action::Mode(matches!(
                m.to_lowercase().as_str(),
                "sys" | "system"
            )))
        }
        "prot" => {
            let Some(v) = parts.next() else {
                return Err(format!("Line {}: missing prot", lineno));
            };
            let pv = parse_hex_or_dec_u8(v).ok_or_else(|| format!("Line {}: bad prot", lineno))?;
            Ok(Action::Prot(pv))
        }
        "map" => {
            let Some(pf) = parts.next() else {
                return Err(format!("Line {}: missing map", lineno));
            };
            let Some(eq) = pf.find('=') else {
                return Err(format!("Line {}: map expects P=F", lineno));
            };
            let (p, f) = pf.split_at(eq);
            let f = &f[1..];
            let page =
                parse_hex_or_dec_usize(p).ok_or_else(|| format!("Line {}: bad page", lineno))?;
            let frame =
                parse_hex_or_dec_u16(f).ok_or_else(|| format!("Line {}: bad frame", lineno))?;
            Ok(Action::Map(page, frame))
        }
        "attr" => {
            let Some(pv) = parts.next() else {
                return Err(format!("Line {}: missing attr", lineno));
            };
            let Some(eq) = pv.find('=') else {
                return Err(format!("Line {}: attr expects P=V", lineno));
            };
            let (p, v) = pv.split_at(eq);
            let v = &v[1..];
            let page =
                parse_hex_or_dec_usize(p).ok_or_else(|| format!("Line {}: bad page", lineno))?;
            let val =
                parse_hex_or_dec_u8(v).ok_or_else(|| format!("Line {}: bad value", lineno))?;
            Ok(Action::Attr(page, val))
        }
        "con_ctrl" => {
            let Some(v) = parts.next() else {
                return Err(format!("Line {}: missing con_ctrl", lineno));
            };
            let pv =
                parse_hex_or_dec_u8(v).ok_or_else(|| format!("Line {}: bad con_ctrl", lineno))?;
            Ok(Action::ConCtrl(pv))
        }
        "con_rx_wm" => {
            let Some(v) = parts.next() else {
                return Err(format!("Line {}: missing con_rx_wm", lineno));
            };
            let pv = parse_hex_or_dec_usize(v)
                .ok_or_else(|| format!("Line {}: bad con_rx_wm", lineno))?;
            Ok(Action::ConRxWm(pv))
        }
        "con_irq_hold" => {
            let Some(v) = parts.next() else {
                return Err(format!("Line {}: missing con_irq_hold", lineno));
            };
            let pv = parse_hex_or_dec_u64(v)
                .ok_or_else(|| format!("Line {}: bad con_irq_hold", lineno))?
                as u32;
            Ok(Action::ConIrqHold(pv))
        }
        "con_firq" => {
            let Some(v) = parts.next() else {
                return Err(format!("Line {}: missing con_firq", lineno));
            };
            let on = matches!(v.to_lowercase().as_str(), "1" | "on" | "true" | "yes");
            Ok(Action::ConFirq(on))
        }
        "blk_irq" => {
            let Some(v) = parts.next() else { return Err(format!("Line {}: missing blk_irq", lineno)); };
            let on = matches!(v.to_lowercase().as_str(), "1" | "on" | "true" | "yes");
            Ok(Action::BlkIrq(on))
        }
        "blk_firq" => {
            let Some(v) = parts.next() else { return Err(format!("Line {}: missing blk_firq", lineno)); };
            let on = matches!(v.to_lowercase().as_str(), "1" | "on" | "true" | "yes");
            Ok(Action::BlkFirq(on))
        }
        "blk_irq_hold" => {
            let Some(v) = parts.next() else { return Err(format!("Line {}: missing blk_irq_hold", lineno)); };
            let pv = parse_hex_or_dec_u64(v).ok_or_else(|| format!("Line {}: bad blk_irq_hold", lineno))? as u32;
            Ok(Action::BlkIrqHold(pv))
        }
        "irq_mask" => {
            let Some(v) = parts.next() else {
                return Err(format!("Line {}: missing irq_mask", lineno));
            };
            let on = matches!(v.to_lowercase().as_str(), "1" | "on" | "true" | "yes");
            Ok(Action::IrqMask(on))
        }
        "firq_mask" => {
            let Some(v) = parts.next() else {
                return Err(format!("Line {}: missing firq_mask", lineno));
            };
            let on = matches!(v.to_lowercase().as_str(), "1" | "on" | "true" | "yes");
            Ok(Action::FirqMask(on))
        }
        other => Err(format!("Line {}: unknown action {}", lineno, other)),
    }
}

fn parse_hex_or_dec_u16(s: &str) -> Option<u16> {
    if let Some(h) = s.strip_prefix("0x").or(s.strip_prefix("0X")) {
        u16::from_str_radix(h, 16).ok()
    } else {
        s.parse::<u16>().ok()
    }
}
fn parse_hex_or_dec_u8(s: &str) -> Option<u8> {
    if let Some(h) = s.strip_prefix("0x").or(s.strip_prefix("0X")) {
        u8::from_str_radix(h, 16).ok()
    } else {
        s.parse::<u8>().ok()
    }
}
fn parse_hex_or_dec_usize(s: &str) -> Option<usize> {
    if let Some(h) = s.strip_prefix("0x").or(s.strip_prefix("0X")) {
        usize::from_str_radix(h, 16).ok()
    } else {
        s.parse::<usize>().ok()
    }
}
fn parse_hex_or_dec_u64(s: &str) -> Option<u64> {
    if let Some(h) = s.strip_prefix("0x").or(s.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()
    } else {
        s.parse::<u64>().ok()
    }
}

pub struct BootSequencer {
    triggers: Vec<Trigger>,
    step: u64,
}

impl BootSequencer {
    pub fn new(triggers: Vec<Trigger>) -> Self {
        Self { triggers, step: 0 }
    }
    pub fn on_pre_step<B: Bus + ?Sized>(
        &mut self,
        bus: &mut B,
        regs_base: u16,
        pc: u16,
        cpu: &mut Cpu,
    ) {
        let ts: Vec<Trigger> = self.triggers.clone();
        for t in ts.iter() {
            match t {
                Trigger::OnPc(match_pc, act) if *match_pc == pc => {
                    apply_action(bus, regs_base, act, Some(cpu));
                }
                _ => {}
            }
        }
    }
    pub fn on_post_step<B: Bus + ?Sized>(&mut self, bus: &mut B, regs_base: u16) {
        self.step += 1;
        let ts: Vec<Trigger> = self.triggers.clone();
        for t in ts.iter() {
            match t {
                Trigger::OnStep(n, act) if *n == self.step => {
                    apply_action(bus, regs_base, act, None);
                }
                _ => {}
            }
        }
    }
}

fn apply_action<B: Bus + ?Sized>(
    bus: &mut B,
    regs_base: u16,
    act: &Action,
    mut cpu_opt: Option<&mut Cpu>,
) {
    match act {
        Action::Mode(sys) => {
            bus.write8(regs_base + 0x11, if *sys { 1 } else { 0 });
        }
        Action::Prot(v) => {
            bus.write8(regs_base + 0x12, *v);
        }
        Action::Map(page, frame) => {
            bus.write8(regs_base + ((*page as u16) & 0x0F), (*frame & 0xFF) as u8);
        }
        Action::Attr(page, val) => {
            bus.write8(regs_base + 0x18 + ((*page as u16) & 0x0F), *val);
        }
        Action::ConCtrl(v) => {
            if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() {
                iom.with_console_mut(|c| c.set_ctrl(*v));
            } else if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
                iom.with_console_mut(|c| c.set_ctrl(*v));
            }
        }
        Action::ConRxWm(n) => {
            if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() {
                iom.with_console_mut(|c| c.set_rx_watermark(*n));
            } else if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
                iom.with_console_mut(|c| c.set_rx_watermark(*n));
            }
        }
        Action::ConIrqHold(n) => {
            if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() {
                iom.with_console_mut(|c| c.set_irq_hold_cycles(*n));
            } else if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
                iom.with_console_mut(|c| c.set_irq_hold_cycles(*n));
            }
        }
        Action::ConFirq(on) => {
            if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() {
                iom.with_console_mut(|c| c.set_firq(*on));
            } else if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
                iom.with_console_mut(|c| c.set_firq(*on));
            }
        }
        Action::IrqMask(on) => {
            if let Some(cpu) = cpu_opt.as_deref_mut() {
                if *on {
                    cpu.r.cc |= 0x10;
                } else {
                    cpu.r.cc &= !0x10;
                }
            }
        }
        Action::FirqMask(on) => {
            if let Some(cpu) = cpu_opt.as_deref_mut() {
                if *on {
                    cpu.r.cc |= 0x40;
                } else {
                    cpu.r.cc &= !0x40;
                }
            }
        }
        Action::BlkIrq(on) => {
            if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() { iom.with_block_mut(|b| b.set_irq_enable(*on)); }
            else if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() { iom.with_block_mut(|b| b.set_irq_enable(*on)); }
        }
        Action::BlkFirq(on) => {
            if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() { iom.with_block_mut(|b| b.set_firq(*on)); }
            else if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() { iom.with_block_mut(|b| b.set_firq(*on)); }
        }
        Action::BlkIrqHold(n) => {
            if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Memory>>() { iom.with_block_mut(|b| b.set_irq_hold_cycles(*n)); }
            else if let Some(iom) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() { iom.with_block_mut(|b| b.set_irq_hold_cycles(*n)); }
        }
    }
}

pub fn emit_boot_template(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "os9l2-boot" => {
            "# OS-9 L2 boot sequence\n\
             # Switch to system mode at reset PC\n\
             on_pc 0xE000 mode sys\n\
             # After 256 steps, switch to user mode\n\
             on_step 256 mode user\n"
                .to_string()
        }
        "os9l2-io" => {
            "# OS-9 L2 IO boot sequence\n\
             # Enter system mode at RESET, enable RX IRQ with FIRQ on console, then later switch user\n\
             on_pc 0xE000 mode sys\n\
             on_pc 0xE000 con_ctrl 0x11\n\
             on_pc 0xE000 con_rx_wm 4\n\
             on_pc 0xE000 con_irq_hold 8\n\
             on_step 512 mode user\n"
                .to_string()
        }
        "os9l2-sequence" | "os9l2-full" => {
            "# OS-9 L2 boot sequence (init -> kernel map -> user transfer -> I/O init)\n\
             # 1) Initialization at RESET: enter system mode, protect vectors, NX null\n\
             on_pc 0xE000 mode sys\n\
             on_pc 0xE000 prot 0x0B    # WPROT_EN | NX_EN | IRQ on fault\n\
             on_pc 0xE000 attr 0x00=0x04  # NX null page\n\
             on_pc 0xE000 attr 0x0F=0x01  # WPROT vectors\n\
             # 2) Kernel placement: remap pages 0x0C..0x0E to high frames where kernel resides\n\
             on_pc 0xE000 map 0x0C=0xD0\n\
             on_pc 0xE000 map 0x0D=0xD1\n\
             on_pc 0xE000 map 0x0E=0xD2\n\
             # 3) Transfer to user mode after some initialization steps\n\
             on_step 512 mode user\n\
             # 4) I/O init: enable console RX IRQ with FIRQ, set watermark/hold\n\
             on_step 520 con_ctrl 0x11   # RX IRQ + FIRQ\n\
             on_step 520 con_rx_wm 4\n\
             on_step 520 con_irq_hold 8\n\
             # Optional: mask/unmask IRQ/FIRQ during stages\n\
             # on_pc 0xE000 irq_mask on\n\
             # on_step 300 irq_mask off\n"
                .to_string()
        }
        _ => {
            "# Boot script template\n\
             # on_pc 0xADDR mode user|sys\n\
             # on_pc 0xADDR prot 0xVAL\n\
             # on_pc 0xADDR map P=F\n\
             # on_pc 0xADDR attr P=VAL\n\
             # on_step N mode user|sys\n"
                .to_string()
        }
    }
}
