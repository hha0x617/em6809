use crate::bus::Bus;
use std::any::Any;
use std::collections::VecDeque;

pub trait Device {
    fn contains(&self, addr: u16) -> bool;
    fn read8(&mut self, addr: u16) -> u8;
    fn write8(&mut self, addr: u16, data: u8);
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn irq_lines(&mut self) -> (bool, bool, bool) { (false, false, false) }
}

pub struct ConsoleDev {
    pub base: u16, // base..base+3 used
    rx: VecDeque<u8>,
    ctrl: u8,
    rx_wm: usize,
    irq_hold_cycles: u32,
    irq_countdown: u32,
}

impl ConsoleDev {
    pub fn new(base: u16) -> Self { Self { base, rx: VecDeque::new(), ctrl: 0, rx_wm: 1, irq_hold_cycles: 0, irq_countdown: 0 } }
    pub fn feed_bytes(&mut self, bytes: &[u8]) { for &b in bytes { self.rx.push_back(b); } }
    pub fn set_rx_watermark(&mut self, wm: usize) { self.rx_wm = wm.max(1); }
    pub fn set_irq_hold_cycles(&mut self, n: u32) { self.irq_hold_cycles = n; }
    pub fn set_ctrl(&mut self, v: u8) { self.ctrl = v; }
    pub fn set_firq(&mut self, on: bool) { if on { self.ctrl |= 0x10; } else { self.ctrl &= !0x10; } }
}

impl Device for ConsoleDev {
    fn contains(&self, addr: u16) -> bool { addr.wrapping_sub(self.base) < 4 }
    fn read8(&mut self, addr: u16) -> u8 {
        let off = addr.wrapping_sub(self.base);
        match off {
            0 => { // RXDATA
                if let Some(b) = self.rx.pop_front() { b } else { 0x00 }
            }
            1 => { // STATUS
                let txrdy = 0x01u8; // always ready
                let rxrdy = if self.rx.len() >= 1 { 0x02 } else { 0x00 };
                txrdy | rxrdy
            }
            2 => self.ctrl,      // CTRL: bit0 RX IRQ enable, bit1 TX IRQ enable
            3 => 0x00,           // BAUD/MISC (ignored)
            _ => 0xFF,
        }
    }
    fn write8(&mut self, addr: u16, data: u8) {
        let off = addr.wrapping_sub(self.base);
        match off {
            0 => { // TXDATA: print ASCII
                // Print without newline; flush is line-buffered by default
                print!("{}", data as char);
            }
            2 => { // CTRL
                self.ctrl = data;
            }
            _ => {}
        }
    }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn irq_lines(&mut self) -> (bool, bool, bool) {
        let rx_irq_now = (self.ctrl & 0x01) != 0 && self.rx.len() >= self.rx_wm;
        let tx_irq_now = (self.ctrl & 0x02) != 0; // always ready
        let mut on = rx_irq_now || tx_irq_now;
        if on {
            self.irq_countdown = self.irq_hold_cycles;
        } else if self.irq_countdown > 0 {
            self.irq_countdown -= 1;
            on = true;
        }
        let use_firq = (self.ctrl & 0x10) != 0; // bit4: use FIRQ
        if use_firq { (false, on, false) } else { (false, false, on) }
    }
}

pub struct IoBus<B: Bus> {
    inner: B,
    devices: Vec<Box<dyn Device>>,
}

impl<B: Bus> IoBus<B> {
    pub fn new(inner: B) -> Self { Self { inner, devices: Vec::new() } }
    pub fn add_device<D: Device + 'static>(&mut self, dev: D) { self.devices.push(Box::new(dev)); }
    pub fn feed_console_input(&mut self, bytes: &[u8]) {
        for d in self.devices.iter_mut() {
            if let Some(c) = d.as_any_mut().downcast_mut::<ConsoleDev>() { c.feed_bytes(bytes); return; }
        }
    }
    pub fn with_console_mut<F: FnOnce(&mut ConsoleDev)>(&mut self, f: F) {
        for d in self.devices.iter_mut() {
            if let Some(c) = d.as_any_mut().downcast_mut::<ConsoleDev>() { f(c); return; }
        }
    }
}

impl<B: Bus + 'static> Bus for IoBus<B> {
    fn read8(&mut self, addr: u16) -> u8 {
        for d in self.devices.iter_mut() { if d.contains(addr) { return d.read8(addr); } }
        self.inner.read8(addr)
    }
    fn write8(&mut self, addr: u16, data: u8) {
        for d in self.devices.iter_mut() { if d.contains(addr) { d.write8(addr, data); return; } }
        self.inner.write8(addr, data)
    }
    fn irq_lines(&mut self) -> (bool, bool, bool) {
        let mut nmi=false; let mut firq=false; let mut irq=false;
        for d in self.devices.iter_mut() {
            let (n, f, i) = d.irq_lines();
            nmi |= n; firq |= f; irq |= i;
        }
        let (n2,f2,i2) = self.inner.irq_lines();
        (nmi|n2, firq|f2, irq|i2)
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
