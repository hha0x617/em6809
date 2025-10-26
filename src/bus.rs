use std::any::Any;

pub trait Bus {
    fn read8(&mut self, addr: u16) -> u8;
    // Instruction fetch path (for execute permission checks). Default: read8
    fn read8_fetch(&mut self, addr: u16) -> u8 {
        self.read8(addr)
    }
    fn write8(&mut self, addr: u16, data: u8);
    fn irq_lines(&mut self) -> (bool, bool, bool) {
        (false, false, false)
    }
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub struct Memory {
    data: [u8; 0x10000],
}

impl Memory {
    pub fn new() -> Self {
        Self { data: [0; 0x10000] }
    }
    pub fn load_slice(&mut self, base: u16, bytes: &[u8]) {
        let start = base as usize;
        let end = (base as usize).saturating_add(bytes.len()).min(0x10000);
        let len = end - start;
        self.data[start..end].copy_from_slice(&bytes[..len]);
    }
    pub fn read_slice(&self, start: u16, len: usize) -> &[u8] {
        let s = start as usize;
        let e = (s + len).min(0x10000);
        &self.data[s..e]
    }
}

impl Bus for Memory {
    fn read8(&mut self, addr: u16) -> u8 {
        self.data[addr as usize]
    }
    fn read8_fetch(&mut self, addr: u16) -> u8 {
        self.read8(addr)
    }
    fn write8(&mut self, addr: u16, data: u8) {
        self.data[addr as usize] = data;
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
