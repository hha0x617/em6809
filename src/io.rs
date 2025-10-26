use crate::bus::Bus;
use std::any::Any;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;

pub trait Device {
    fn contains(&self, addr: u16) -> bool;
    fn read8(&mut self, addr: u16) -> u8;
    fn write8(&mut self, addr: u16, data: u8);
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn irq_lines(&mut self) -> (bool, bool, bool) {
        (false, false, false)
    }
}

pub struct ConsoleDev {
    pub base: u16, // base..base+3 used
    rx: VecDeque<u8>,
    ctrl: u8,
    rx_wm: usize,
    irq_hold_cycles: u32,
    irq_countdown: u32,
    out_file: Option<File>,
    tee_stderr: bool,
    flush_every: Option<usize>,
    flush_count: usize,
    local_echo: bool,
    utf8_buf: Vec<u8>,
    utf8_need: usize,
}

impl ConsoleDev {
    pub fn new(base: u16) -> Self {
        Self {
            base,
            rx: VecDeque::new(),
            ctrl: 0,
            rx_wm: 1,
            irq_hold_cycles: 0,
            irq_countdown: 0,
            out_file: None,
            tee_stderr: false,
            flush_every: None,
            flush_count: 0,
            local_echo: false,
            utf8_buf: Vec::with_capacity(8),
            utf8_need: 0,
        }
    }
    pub fn feed_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.rx.push_back(b);
        }
    }
    pub fn set_rx_watermark(&mut self, wm: usize) {
        self.rx_wm = wm.max(1);
    }
    pub fn set_irq_hold_cycles(&mut self, n: u32) {
        self.irq_hold_cycles = n;
    }
    pub fn set_ctrl(&mut self, v: u8) {
        self.ctrl = v;
    }
    pub fn set_firq(&mut self, on: bool) {
        if on {
            self.ctrl |= 0x10;
        } else {
            self.ctrl &= !0x10;
        }
    }
    pub fn set_out_file(&mut self, path: &str) {
        if let Ok(f) = File::create(path) {
            self.out_file = Some(f);
        }
    }
    pub fn set_tee_stderr(&mut self, on: bool) {
        self.tee_stderr = on;
    }
    pub fn set_flush_every(&mut self, n: usize) {
        self.flush_every = if n > 0 { Some(n) } else { None };
        self.flush_count = 0;
    }
    pub fn set_local_echo(&mut self, on: bool) {
        self.local_echo = on;
    }

    fn push_stdout_byte(&mut self, b: u8) {
        // Mirror raw byte to file (unmodified)
        if let Some(f) = self.out_file.as_mut() {
            let _ = f.write_all(&[b]);
        }
        // ASCII fast path
        if b < 0x80 && self.utf8_need == 0 {
            print!("{}", b as char);
        } else {
            // Determine expected length if starting sequence
            if self.utf8_need == 0 {
                self.utf8_need = match b {
                    0xC0..=0xDF => 2,
                    0xE0..=0xEF => 3,
                    0xF0..=0xF7 => 4,
                    _ => 1,
                };
            }
            self.utf8_buf.push(b);
            if self.utf8_buf.len() >= self.utf8_need.max(1) {
                let s = String::from_utf8_lossy(&self.utf8_buf);
                print!("{}", s);
                self.utf8_buf.clear();
                self.utf8_need = 0;
            }
        }
        // Flush policy
        let mut need_flush = b == b'\n';
        if let Some(n) = self.flush_every {
            self.flush_count = self.flush_count.wrapping_add(1);
            if self.flush_count % n == 0 {
                need_flush = true;
            }
        }
        if need_flush {
            let _ = std::io::stdout().flush();
            if let Some(f) = self.out_file.as_mut() {
                let _ = f.flush();
            }
        }
    }
}

impl Device for ConsoleDev {
    fn contains(&self, addr: u16) -> bool {
        addr.wrapping_sub(self.base) < 4
    }
    fn read8(&mut self, addr: u16) -> u8 {
        let off = addr.wrapping_sub(self.base);
        match off {
            0 => {
                // RXDATA
                if let Some(b) = self.rx.pop_front() {
                    // Optional local echo to TX path
                    if self.local_echo {
                        self.push_stdout_byte(b);
                    }
                    b
                } else {
                    0x00
                }
            }
            1 => {
                // STATUS
                let txrdy = 0x01u8; // always ready
                let rxrdy = if self.rx.len() >= 1 { 0x02 } else { 0x00 };
                txrdy | rxrdy
            }
            2 => self.ctrl, // CTRL: bit0 RX IRQ enable, bit1 TX IRQ enable
            3 => 0x00,      // BAUD/MISC (ignored)
            _ => 0xFF,
        }
    }
    fn write8(&mut self, addr: u16, data: u8) {
        let off = addr.wrapping_sub(self.base);
        match off {
            0 => {
                // TXDATA: print ASCII
                // Print and flush to ensure visibility in buffered consoles
                if self.tee_stderr {
                    let _ = writeln!(
                        std::io::stderr(),
                        "[console-tx] addr={:04X} data={:02X}",
                        addr,
                        data
                    );
                }
                self.push_stdout_byte(data);
            }
            2 => {
                // CTRL
                self.ctrl = data;
            }
            _ => {}
        }
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
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
        if use_firq {
            (false, on, false)
        } else {
            (false, false, on)
        }
    }
}

pub struct IoBus<B: Bus> {
    inner: B,
    devices: Vec<Box<dyn Device>>,
}

impl<B: Bus> IoBus<B> {
    pub fn new(inner: B) -> Self {
        Self {
            inner,
            devices: Vec::new(),
        }
    }
    pub fn add_device<D: Device + 'static>(&mut self, dev: D) {
        self.devices.push(Box::new(dev));
    }
    pub fn feed_console_input(&mut self, bytes: &[u8]) {
        for d in self.devices.iter_mut() {
            if let Some(c) = d.as_any_mut().downcast_mut::<ConsoleDev>() {
                c.feed_bytes(bytes);
                return;
            }
        }
    }
    pub fn with_console_mut<F: FnOnce(&mut ConsoleDev)>(&mut self, f: F) {
        for d in self.devices.iter_mut() {
            if let Some(c) = d.as_any_mut().downcast_mut::<ConsoleDev>() {
                f(c);
                return;
            }
        }
    }
}

impl<B: Bus + 'static> Bus for IoBus<B> {
    fn read8(&mut self, addr: u16) -> u8 {
        for d in self.devices.iter_mut() {
            if d.contains(addr) {
                return d.read8(addr);
            }
        }
        self.inner.read8(addr)
    }
    fn write8(&mut self, addr: u16, data: u8) {
        for d in self.devices.iter_mut() {
            if d.contains(addr) {
                d.write8(addr, data);
                return;
            }
        }
        self.inner.write8(addr, data)
    }
    fn irq_lines(&mut self) -> (bool, bool, bool) {
        let mut nmi = false;
        let mut firq = false;
        let mut irq = false;
        for d in self.devices.iter_mut() {
            let (n, f, i) = d.irq_lines();
            nmi |= n;
            firq |= f;
            irq |= i;
        }
        let (n2, f2, i2) = self.inner.irq_lines();
        (nmi | n2, firq | f2, irq | i2)
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
