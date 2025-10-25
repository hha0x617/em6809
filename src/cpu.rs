use crate::bus::Bus;

#[derive(Clone, Copy, Default)]
pub struct Registers {
    pub a: u8,
    pub b: u8,
    pub x: u16,
    pub y: u16,
    pub u: u16,
    pub s: u16,
    pub pc: u16,
    pub dp: u8,
    pub cc: u8, // E F H I N Z V C
}

pub struct Cpu {
    pub r: Registers,
    pub cycles: u64,
    // pending interrupt latches
    pub nmi_pending: bool,
    pub firq_pending: bool,
    pub irq_pending: bool,
}

// CC flag bits
const E: u8 = 0x80;
const F: u8 = 0x40;
const H: u8 = 0x20;
const I: u8 = 0x10;
const N: u8 = 0x08;
const Z: u8 = 0x04;
const V: u8 = 0x02;
const C: u8 = 0x01;

impl Cpu {
    pub fn new() -> Self { Self { r: Registers::default(), cycles: 0, nmi_pending: false, firq_pending: false, irq_pending: false } }

    pub fn reset<B: Bus + ?Sized>(&mut self, bus: &mut B) {
        // PC from reset vector at FFFE/FFFF
        let pch = bus.read8(0xFFFE);
        let pcl = bus.read8(0xFFFF);
        self.r.pc = ((pch as u16) << 8) | (pcl as u16);
        self.r.cc = I | F; // mask interrupts by default
        self.r.dp = 0;
    }

    pub fn set_pc(&mut self, pc: u16) { self.r.pc = pc; }
    pub fn request_nmi(&mut self) { self.nmi_pending = true; }
    pub fn request_firq(&mut self) { self.firq_pending = true; }
    pub fn request_irq(&mut self) { self.irq_pending = true; }

    fn fetch8<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u8 { let v = bus.read8_fetch(self.r.pc); self.r.pc = self.r.pc.wrapping_add(1); v }
    fn fetch16<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u16 { let hi = self.fetch8(bus) as u16; let lo = self.fetch8(bus) as u16; (hi<<8)|lo }

    fn ea_direct(&self, zp: u8) -> u16 { ((self.r.dp as u16) << 8) | (zp as u16) }
    fn ea_extended<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u16 { self.fetch16(bus) }

    fn get_idx_reg(&self, code: u8) -> u16 {
        match code & 0x03 { 0 => self.r.x, 1 => self.r.y, 2 => self.r.u, _ => self.r.s }
    }
    fn set_idx_reg(&mut self, code: u8, val: u16) {
        match code & 0x03 { 0 => self.r.x = val, 1 => self.r.y = val, 2 => self.r.u = val, _ => self.r.s = val }
    }

    // Decode subset of 6809 indexed addressing postbyte; returns (EA, extra_cycles, desc)
    fn ea_indexed<B: Bus + ?Sized>(&mut self, bus: &mut B) -> (u16, u32, String) {
        let pb = self.fetch8(bus);
        let rsel = (pb >> 5) & 0x03; // 0:X 1:Y 2:U 3:S
        if (pb & 0x80) == 0 { // 5-bit signed offset
            let base = self.get_idx_reg(rsel);
            let off5 = ((pb & 0x1F) as i8) << 3 >> 3; // sign-extend 5-bit
            let ea = base.wrapping_add(off5 as u16);
            let reg = match rsel {0=>"X",1=>"Y",2=>"U",_=>"S"};
            let desc = format!("{}+{}", reg, off5);
            return (ea, 0, desc); // base indexed cost will be applied by caller
        }
        // bit7=1: extended forms (partial)
        let form = pb & 0x1F;
        let indirect = (pb & 0x10) != 0; // indirect (bracketed) forms
        // For extended forms, PC-relative when rsel==3 (S code) with certain forms
        let mut base = self.get_idx_reg(rsel);
        let is_pc_rel = rsel == 3 && matches!(form, 0x04 | 0x08 | 0x09 | 0x0B);
        if is_pc_rel { base = self.r.pc; }
        match form {
            0x04 => { // ,R
                let mut ea = base;
                if indirect { let hi = bus.read8(ea) as u16; let lo = bus.read8(ea.wrapping_add(1)) as u16; ea = (hi<<8)|lo; }
                let reg = if is_pc_rel { "PC" } else { match rsel {0=>"X",1=>"Y",2=>"U",_=>"S"} };
                let desc = if indirect { format!("[{}]", reg) } else { format!(",{}", reg) };
                let add = if indirect { 3 } else { 0 };
                (ea, add, desc)
            }
            0x05 => { // B,R
                let off = self.r.b as i8 as i16;
                let mut ea = (base as i16).wrapping_add(off) as u16;
                if indirect { let hi = bus.read8(ea) as u16; let lo = bus.read8(ea.wrapping_add(1)) as u16; ea = (hi<<8)|lo; }
                let reg = if is_pc_rel { "PC" } else { match rsel {0=>"X",1=>"Y",2=>"U",_=>"S"} };
                let desc = if indirect { format!("[B,{}]", reg) } else { format!("B,{}", reg) };
                let add = if indirect { 3 } else { 0 };
                (ea, add, desc)
            }
            0x06 => { // A,R
                let off = self.r.a as i8 as i16;
                let mut ea = (base as i16).wrapping_add(off) as u16;
                if indirect { let hi = bus.read8(ea) as u16; let lo = bus.read8(ea.wrapping_add(1)) as u16; ea = (hi<<8)|lo; }
                let reg = if is_pc_rel { "PC" } else { match rsel {0=>"X",1=>"Y",2=>"U",_=>"S"} };
                let desc = if indirect { format!("[A,{}]", reg) } else { format!("A,{}", reg) };
                let add = if indirect { 3 } else { 0 };
                (ea, add, desc)
            }
            0x08 => { // 8-bit offset
                let off8 = self.fetch8(bus) as i8 as i16;
                let mut ea = (base as i16).wrapping_add(off8) as u16;
                if indirect { let hi = bus.read8(ea) as u16; let lo = bus.read8(ea.wrapping_add(1)) as u16; ea = (hi<<8)|lo; }
                let reg = if is_pc_rel { "PC" } else { match rsel {0=>"X",1=>"Y",2=>"U",_=>"S"} };
                let desc = if indirect { format!("[{}+{}]", reg, off8) } else { format!("{}+{}", reg, off8) };
                let add = 1 + if indirect { 3 } else { 0 };
                (ea, add, desc)
            }
            0x09 => { // 16-bit offset
                let off16 = self.fetch16(bus) as i16;
                let mut ea = (base as i32 + off16 as i32) as u16;
                if indirect { let hi = bus.read8(ea) as u16; let lo = bus.read8(ea.wrapping_add(1)) as u16; ea = (hi<<8)|lo; }
                let reg = if is_pc_rel { "PC" } else { match rsel {0=>"X",1=>"Y",2=>"U",_=>"S"} };
                let desc = if indirect { format!("[{}+{}]", reg, off16) } else { format!("{}+{}", reg, off16) };
                let add = 2 + if indirect { 3 } else { 0 };
                (ea, add, desc)
            }
            0x00 => { // ,R+
                let ea = base;
                self.set_idx_reg(rsel, base.wrapping_add(1));
                let desc = format!(",{}+", match rsel {0=>"X",1=>"Y",2=>"U",_=>"S"});
                (ea, 0, desc)
            }
            0x01 => { // ,R++
                let ea = base;
                self.set_idx_reg(rsel, base.wrapping_add(2));
                let desc = format!(",{}++", match rsel {0=>"X",1=>"Y",2=>"U",_=>"S"});
                (ea, 1, desc)
            }
            0x02 => { // ,-R
                let ea = base.wrapping_sub(1);
                self.set_idx_reg(rsel, ea);
                let desc = format!(",-{}", match rsel {0=>"X",1=>"Y",2=>"U",_=>"S"});
                (ea, 0, desc)
            }
            0x03 => { // ,--R
                let ea = base.wrapping_sub(2);
                self.set_idx_reg(rsel, ea);
                let desc = format!(",--{}", match rsel {0=>"X",1=>"Y",2=>"U",_=>"S"});
                (ea, 1, desc)
            }
            0x0B => { // D,R (already handled; keep explicit for clarity)
                let off = (((self.r.a as u16) << 8) | self.r.b as u16) as i16;
                let mut ea = (base as i32 + off as i32) as u16;
                if indirect { let hi = bus.read8(ea) as u16; let lo = bus.read8(ea.wrapping_add(1)) as u16; ea = (hi<<8)|lo; }
                let reg = if is_pc_rel { "PC" } else { match rsel {0=>"X",1=>"Y",2=>"U",_=>"S"} };
                let desc = if indirect { format!("[D,{}]", reg) } else { format!("D,{}", reg) };
                let add = 1 + if indirect { 3 } else { 0 };
                (ea, add, desc)
            }
            0x0F | 0x1F => { // [n16] extended (absolute) indirect pointer
                let addr = self.fetch16(bus);
                // Per 6809, this is always indirect; treat both variants as indirect
                let hi = bus.read8(addr) as u16; let lo = bus.read8(addr.wrapping_add(1)) as u16;
                let ea = (hi<<8)|lo;
                let desc = format!("[{:04X}]", addr);
                (ea, 5, desc) // 16-bit offset (2) + indirect (3)
            }
            _ => {
                // Fallback: treat as ,R
                let ea = base;
                let desc = format!(",{}?", match rsel {0=>"X",1=>"Y",2=>"U",_=>"S"});
                (ea, 0, desc)
            }
        }
    }

    fn set_nz8(&mut self, v: u8) {
        self.r.cc = (self.r.cc & !(N|Z)) | (if v==0 {Z} else {0}) | (if (v & 0x80)!=0 {N} else {0});
    }
    fn set_nz16(&mut self, v: u16) {
        self.r.cc = (self.r.cc & !(N|Z)) | (if v==0 {Z} else {0}) | (if (v & 0x8000)!=0 {N} else {0});
    }

    // Stack helpers (S)
    fn push8<B: Bus + ?Sized>(&mut self, bus: &mut B, v: u8) { self.r.s = self.r.s.wrapping_sub(1); bus.write8(self.r.s, v); }
    fn push16<B: Bus + ?Sized>(&mut self, bus: &mut B, v: u16) { self.push8(bus, (v & 0x00FF) as u8); self.push8(bus, (v >> 8) as u8); }
    fn pull8<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u8 { let v = bus.read8(self.r.s); self.r.s = self.r.s.wrapping_add(1); v }
    fn pull16<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u16 { let hi = self.pull8(bus) as u16; let lo = self.pull8(bus) as u16; (hi<<8)|lo }

    // U-stack helpers
    fn upush8<B: Bus + ?Sized>(&mut self, bus: &mut B, v: u8) { self.r.u = self.r.u.wrapping_sub(1); bus.write8(self.r.u, v); }
    fn upush16<B: Bus + ?Sized>(&mut self, bus: &mut B, v: u16) { self.upush8(bus, (v & 0x00FF) as u8); self.upush8(bus, (v >> 8) as u8); }
    fn upull8<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u8 { let v = bus.read8(self.r.u); self.r.u = self.r.u.wrapping_add(1); v }
    fn upull16<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u16 { let hi = self.upull8(bus) as u16; let lo = self.upull8(bus) as u16; (hi<<8)|lo }

    fn cond_eval(&self, opcode: u8) -> Option<bool> {
        let c = (self.r.cc & C) != 0;
        let z = (self.r.cc & Z) != 0;
        let v = (self.r.cc & V) != 0;
        let n = (self.r.cc & N) != 0;
        let nv = n ^ v;
        match opcode {
            0x21 => Some(false),             // BRN (never)
            0x22 => Some(!(c || z)),         // BHI
            0x23 => Some(c || z),            // BLS
            0x24 => Some(!c),                // BCC/BHS
            0x25 => Some(c),                 // BCS/BLO
            0x26 => Some(!z),                // BNE
            0x27 => Some(z),                 // BEQ
            0x28 => Some(!v),                // BVC
            0x29 => Some(v),                 // BVS
            0x2A => Some(!n),                // BPL
            0x2B => Some(n),                 // BMI
            0x2C => Some(!nv),               // BGE
            0x2D => Some(nv),                // BLT
            0x2E => Some(!z && !nv),         // BGT
            0x2F => Some(z || nv),           // BLE
            _ => None,
        }
    }

    fn service_nmi<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u32 {
        // NMI: push entire state, set I (mask IRQ), jump to vector FFFA/FFFB
        self.r.cc |= E;
        self.push8(bus, self.r.cc);
        self.push8(bus, self.r.a);
        self.push8(bus, self.r.b);
        self.push8(bus, self.r.dp);
        self.push16(bus, self.r.x);
        self.push16(bus, self.r.y);
        self.push16(bus, self.r.u);
        self.push16(bus, self.r.pc);
        self.r.cc |= I | F;
        let hi = bus.read8(0xFFFA) as u16; let lo = bus.read8(0xFFFB) as u16;
        self.r.pc = (hi<<8)|lo;
        self.nmi_pending = false;
        19
    }
    fn service_firq<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u32 {
        // FIRQ: push CC and PC only (fast), clear E, mask FIRQ/IRQ
        self.push8(bus, self.r.cc);
        self.push16(bus, self.r.pc);
        self.r.cc &= !E;
        self.r.cc |= I | F;
        let hi = bus.read8(0xFFF4) as u16; let lo = bus.read8(0xFFF5) as u16;
        self.r.pc = (hi<<8)|lo;
        self.firq_pending = false;
        10
    }
    fn service_irq<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u32 {
        // IRQ: push entire state, set I, go to IRQ vector FFF6/FFF7
        self.r.cc |= E;
        self.push8(bus, self.r.cc);
        self.push8(bus, self.r.a);
        self.push8(bus, self.r.b);
        self.push8(bus, self.r.dp);
        self.push16(bus, self.r.x);
        self.push16(bus, self.r.y);
        self.push16(bus, self.r.u);
        self.push16(bus, self.r.pc);
        self.r.cc |= I | F;
        let hi = bus.read8(0xFFF6) as u16; let lo = bus.read8(0xFFF7) as u16;
        self.r.pc = (hi<<8)|lo;
        self.irq_pending = false;
        19
    }

    // TFR/EXG helpers
    fn get_reg16_by_code(&self, code: u8) -> Option<u16> {
        match code & 0x0F {
            0x0 => Some(((self.r.a as u16) << 8) | self.r.b as u16), // D
            0x1 => Some(self.r.x),
            0x2 => Some(self.r.y),
            0x3 => Some(self.r.u),
            0x4 => Some(self.r.s),
            0x5 => Some(self.r.pc),
            _ => None,
        }
    }
    fn set_reg16_by_code(&mut self, code: u8, val: u16) -> bool {
        match code & 0x0F {
            0x0 => { self.r.a = (val >> 8) as u8; self.r.b = (val & 0xFF) as u8; true }
            0x1 => { self.r.x = val; true }
            0x2 => { self.r.y = val; true }
            0x3 => { self.r.u = val; true }
            0x4 => { self.r.s = val; true }
            0x5 => { self.r.pc = val; true }
            _ => false,
        }
    }
    fn get_reg8_by_code(&self, code: u8) -> Option<u8> {
        match code & 0x0F {
            0x8 => Some(self.r.a),
            0x9 => Some(self.r.b),
            0xA => Some(self.r.cc),
            0xB => Some(self.r.dp),
            _ => None,
        }
    }
    fn set_reg8_by_code(&mut self, code: u8, val: u8) -> bool {
        match code & 0x0F {
            0x8 => { self.r.a = val; true }
            0x9 => { self.r.b = val; true }
            0xA => { self.r.cc = val; true }
            0xB => { self.r.dp = val; true }
            _ => false,
        }
    }

    // ALU helpers
    fn add8_flags(&mut self, a: u8, b: u8) -> u8 {
        let (sum1, c1) = a.overflowing_add(b);
        let half = ((a & 0x0F) + (b & 0x0F)) > 0x0F;
        let ovf = ((a ^ sum1) & (b ^ sum1) & 0x80) != 0;
        self.set_nz8(sum1);
        self.r.cc = (self.r.cc & !(C|V|H)) | (if c1 {C} else {0}) | (if ovf {V} else {0}) | (if half {H} else {0});
        sum1
    }
    fn adc8_flags(&mut self, a: u8, b: u8) -> u8 {
        let cin = if (self.r.cc & C) != 0 {1} else {0};
        let (t, c1) = a.overflowing_add(b);
        let (sum, c2) = t.overflowing_add(cin);
        let carry = c1 || c2;
        let half = ((a & 0x0F) + (b & 0x0F) + cin) > 0x0F;
        let ovf = (((a ^ sum) & (b ^ sum)) & 0x80) != 0;
        self.set_nz8(sum);
        self.r.cc = (self.r.cc & !(C|V|H)) | (if carry {C} else {0}) | (if ovf {V} else {0}) | (if half {H} else {0});
        sum
    }
    fn sub8_flags(&mut self, a: u8, b: u8) -> u8 {
        let (diff, borrow) = a.overflowing_sub(b);
        // C=1 means no borrow
        let cflag = !borrow;
        let ovf = (((a ^ b) & (a ^ diff)) & 0x80) != 0;
        let half = ((a & 0x0F) as i16 - (b & 0x0F) as i16) < 0;
        self.set_nz8(diff);
        self.r.cc = (self.r.cc & !(C|V|H)) | (if cflag {C} else {0}) | (if ovf {V} else {0}) | (if half {H} else {0});
        diff
    }
    fn sbc8_flags(&mut self, a: u8, b: u8) -> u8 {
        let cin = if (self.r.cc & C)!=0 {0} else {1}; // borrow-in = !C
        let (t, b1) = a.overflowing_sub(b);
        let (diff, b2) = t.overflowing_sub(cin);
        let borrow = b1 || b2;
        let cflag = !borrow;
        let ovf = (((a ^ b) & (a ^ diff)) & 0x80) != 0;
        let half = ((a & 0x0F) as i16 - (b & 0x0F) as i16 - cin as i16) < 0;
        self.set_nz8(diff);
        self.r.cc = (self.r.cc & !(C|V|H)) | (if cflag {C} else {0}) | (if ovf {V} else {0}) | (if half {H} else {0});
        diff
    }
    fn logic8_nz_clearv(&mut self, v: u8) {
        self.set_nz8(v);
        self.r.cc &= !V;
    }

    fn add16_flags(&mut self, a: u16, b: u16) -> u16 {
        let (sum, carry) = a.overflowing_add(b);
        let ovf = ((a ^ sum) & (b ^ sum) & 0x8000) != 0;
        self.set_nz16(sum);
        self.r.cc = (self.r.cc & !(C|V)) | (if carry {C} else {0}) | (if ovf {V} else {0});
        sum
    }
    fn sub16_flags(&mut self, a: u16, b: u16) -> u16 {
        let (diff, borrow) = a.overflowing_sub(b);
        let cflag = !borrow; // 6809: C=1 means no borrow
        let ovf = (((a ^ b) & (a ^ diff)) & 0x8000) != 0;
        self.set_nz16(diff);
        self.r.cc = (self.r.cc & !(C|V)) | (if cflag {C} else {0}) | (if ovf {V} else {0});
        diff
    }

    pub fn step<B: Bus + ?Sized>(&mut self, bus: &mut B, trace: bool) -> u32 {
        let pc0 = self.r.pc;
        let op = self.fetch8(bus);
        let mut consumed: u32 = 0;
        match op {
            0x10 => { // prefix for long conditional branches / SWI2 / CMPD / Y-reg ops
                let sub = self.fetch8(bus);
                if sub == 0x3F { // SWI2
                    self.r.cc |= E;
                    self.push8(bus, self.r.cc);
                    self.push8(bus, self.r.a);
                    self.push8(bus, self.r.b);
                    self.push8(bus, self.r.dp);
                    self.push16(bus, self.r.x);
                    self.push16(bus, self.r.y);
                    self.push16(bus, self.r.u);
                    self.push16(bus, self.r.pc);
                    self.r.cc |= I | F;
                    let hi = bus.read8(0xFFF2) as u16; let lo = bus.read8(0xFFF3) as u16;
                    self.r.pc = (hi<<8)|lo;
                    consumed = 20; if trace { println!("{:04X}: 10 3F   SWI2", pc0); }
                } else if sub == 0x83 { // CMPD #imm16
                    let imm = self.fetch16(bus);
                    let d = ((self.r.a as u16) << 8) | (self.r.b as u16);
                    let _ = self.sub16_flags(d, imm);
                    consumed = 5; if trace { println!("{:04X}: 10 83 {:04X} CMPD #${:04X}", pc0, imm, imm); }
                } else if sub == 0x8E { // LDY #imm16
                    let imm = self.fetch16(bus);
                    self.r.y = imm;
                    self.set_nz16(self.r.y);
                    self.r.cc &= !V;
                    consumed = 4; if trace { println!("{:04X}: 10 8E {:04X} LDY #${:04X}", pc0, imm, imm); }
                } else if sub == 0x9F { // STY <dir
                    let zp = self.fetch8(bus);
                    let addr = self.ea_direct(zp);
                    bus.write8(addr, (self.r.y >> 8) as u8);
                    bus.write8(addr.wrapping_add(1), (self.r.y & 0xFF) as u8);
                    self.set_nz16(self.r.y);
                    self.r.cc &= !V;
                    consumed = 5; if trace { println!("{:04X}: 10 9F {:02X} STY <${:02X}", pc0, zp, zp); }
                } else if sub == 0xAF { // STY indexed
                    let (ea, addcyc, desc) = self.ea_indexed(bus);
                    bus.write8(ea, (self.r.y >> 8) as u8);
                    bus.write8(ea.wrapping_add(1), (self.r.y & 0xFF) as u8);
                    self.set_nz16(self.r.y);
                    self.r.cc &= !V;
                    consumed = 5 + addcyc; if trace { println!("{:04X}: 10 AF ..    STY {}", pc0, desc); }
                } else if sub == 0x8C { // CMPY #imm16
                    let imm = self.fetch16(bus);
                    let y = self.r.y;
                    let _ = self.sub16_flags(y, imm);
                    consumed = 5; if trace { println!("{:04X}: 10 8C {:04X} CMPY #${:04X}", pc0, imm, imm); }
                } else if sub == 0x9E { // LDY <dir
                    let zp = self.fetch8(bus);
                    let addr = self.ea_direct(zp);
                    let hi = bus.read8(addr) as u16; let lo = bus.read8(addr.wrapping_add(1)) as u16;
                    self.r.y = (hi<<8)|lo;
                    self.set_nz16(self.r.y); self.r.cc &= !V;
                    consumed = 5; if trace { println!("{:04X}: 10 9E {:02X} LDY <${:02X}", pc0, zp, zp); }
                } else if sub == 0xAE { // LDY indexed
                    let (ea, addcyc, desc) = self.ea_indexed(bus);
                    let hi = bus.read8(ea) as u16; let lo = bus.read8(ea.wrapping_add(1)) as u16;
                    self.r.y = (hi<<8)|lo;
                    self.set_nz16(self.r.y); self.r.cc &= !V;
                    consumed = 5 + addcyc; if trace { println!("{:04X}: 10 AE ..    LDY {}", pc0, desc); }
                } else if sub == 0xBE { // LDY extended
                    let addr = self.ea_extended(bus);
                    let hi = bus.read8(addr) as u16; let lo = bus.read8(addr.wrapping_add(1)) as u16;
                    self.r.y = (hi<<8)|lo; self.set_nz16(self.r.y); self.r.cc &= !V;
                    consumed = 6; if trace { println!("{:04X}: 10 BE {:04X} LDY ${:04X}", pc0, addr, addr); }
                } else if sub == 0xBF { // STY extended
                    let addr = self.ea_extended(bus);
                    bus.write8(addr, (self.r.y >> 8) as u8); bus.write8(addr.wrapping_add(1), (self.r.y & 0xFF) as u8);
                    self.set_nz16(self.r.y); self.r.cc &= !V;
                    consumed = 6; if trace { println!("{:04X}: 10 BF {:04X} STY ${:04X}", pc0, addr, addr); }
                } else if sub == 0x9C { // CMPY <dir
                    let zp = self.fetch8(bus);
                    let addr = self.ea_direct(zp);
                    let hi = bus.read8(addr) as u16; let lo = bus.read8(addr.wrapping_add(1)) as u16;
                    let v = (hi<<8)|lo; let _ = self.sub16_flags(self.r.y, v);
                    consumed = 5; if trace { println!("{:04X}: 10 9C {:02X} CMPY <${:02X}", pc0, zp, zp); }
                } else if sub == 0xAC { // CMPY indexed
                    let (ea, addcyc, desc) = self.ea_indexed(bus);
                    let hi = bus.read8(ea) as u16; let lo = bus.read8(ea.wrapping_add(1)) as u16; let v=(hi<<8)|lo;
                    let _ = self.sub16_flags(self.r.y, v);
                    consumed = 5 + addcyc; if trace { println!("{:04X}: 10 AC ..    CMPY {}", pc0, desc); }
                } else if sub == 0xBC { // CMPY extended
                    let addr = self.ea_extended(bus);
                    let hi = bus.read8(addr) as u16; let lo = bus.read8(addr.wrapping_add(1)) as u16; let v=(hi<<8)|lo;
                    let _ = self.sub16_flags(self.r.y, v);
                    consumed = 6; if trace { println!("{:04X}: 10 BC {:04X} CMPY ${:04X}", pc0, addr, addr); }
                } else if let Some(taken) = self.cond_eval(sub) {
                    let off = self.fetch16(bus) as i16;
                    if taken {
                        let newpc = (self.r.pc as i32 + off as i32) as u16;
                        self.r.pc = newpc;
                        consumed = 5; // taken
                        if trace { println!("{:04X}: 10 {:02X} {:04X} LBR taken ${:04X}", pc0, sub, off as u16, newpc); }
                    } else {
                        consumed = 4; // not taken
                        if trace { println!("{:04X}: 10 {:02X} {:04X} LBR not", pc0, sub, off as u16); }
                    }
                } else {
                    if trace { println!("{:04X}: 10 {:02X}   UNIMPL-PFX", pc0, sub); }
                    consumed = 1;
                }
            }
            0x32 => { // LEAU indexed
                let (ea, addcyc, _desc) = self.ea_indexed(bus);
                self.r.u = ea;
                self.r.cc = (self.r.cc & !Z) | (if self.r.u == 0 { Z } else { 0 });
                consumed = 4 + addcyc; if trace { println!("{:04X}: 32 ..    LEAU", pc0); }
            }
            0x33 => { // LEAS indexed
                let (ea, addcyc, _desc) = self.ea_indexed(bus);
                self.r.s = ea;
                self.r.cc = (self.r.cc & !Z) | (if self.r.s == 0 { Z } else { 0 });
                consumed = 4 + addcyc; if trace { println!("{:04X}: 33 ..    LEAS", pc0); }
            }
            0xCE => { // LDU #imm16
                let imm = self.fetch16(bus); self.r.u = imm; self.set_nz16(self.r.u); self.r.cc &= !V; consumed = 3; if trace { println!("{:04X}: CE {:04X} LDU #${:04X}", pc0, imm, imm); }
            }
            0xDE => { // LDU <dir
                let zp = self.fetch8(bus); let addr = self.ea_direct(zp);
                let hi = bus.read8(addr) as u16; let lo = bus.read8(addr.wrapping_add(1)) as u16; self.r.u=(hi<<8)|lo; self.set_nz16(self.r.u); self.r.cc &= !V; consumed=4; if trace { println!("{:04X}: DE {:02X} LDU <${:02X}", pc0, zp, zp); }
            }
            0xEE => { // LDU indexed
                let (ea, addcyc, desc) = self.ea_indexed(bus); let hi=bus.read8(ea) as u16; let lo=bus.read8(ea.wrapping_add(1)) as u16; self.r.u=(hi<<8)|lo; self.set_nz16(self.r.u); self.r.cc &= !V; consumed=5+addcyc; if trace { println!("{:04X}: EE ..    LDU {}", pc0, desc); }
            }
            0xFE => { // LDU extended
                let addr = self.ea_extended(bus); let hi=bus.read8(addr) as u16; let lo=bus.read8(addr.wrapping_add(1)) as u16; self.r.u=(hi<<8)|lo; self.set_nz16(self.r.u); self.r.cc &= !V; consumed=6; if trace { println!("{:04X}: FE {:04X} LDU ${:04X}", pc0, addr, addr); }
            }
            0xDF => { // STU <dir
                let zp = self.fetch8(bus); let addr=self.ea_direct(zp); bus.write8(addr, (self.r.u>>8) as u8); bus.write8(addr.wrapping_add(1), (self.r.u&0xFF) as u8); self.set_nz16(self.r.u); self.r.cc &= !V; consumed=4; if trace { println!("{:04X}: DF {:02X} STU <${:02X}", pc0, zp, zp); }
            }
            0xEF => { // STU indexed
                let (ea, addcyc, desc) = self.ea_indexed(bus); bus.write8(ea,(self.r.u>>8) as u8); bus.write8(ea.wrapping_add(1),(self.r.u&0xFF) as u8); self.set_nz16(self.r.u); self.r.cc &= !V; consumed=5+addcyc; if trace { println!("{:04X}: EF ..    STU {}", pc0, desc); }
            }
            0xFF => { // STU extended
                let addr = self.ea_extended(bus); bus.write8(addr,(self.r.u>>8) as u8); bus.write8(addr.wrapping_add(1),(self.r.u&0xFF) as u8); self.set_nz16(self.r.u); self.r.cc &= !V; consumed=6; if trace { println!("{:04X}: FF {:04X} STU ${:04X}", pc0, addr, addr); }
            }
            0x11 => { // SWI3 or extended prefix
                let sub = self.fetch8(bus);
                if sub == 0x3F { // SWI3
                    self.r.cc |= E;
                    self.push8(bus, self.r.cc);
                    self.push16(bus, self.r.pc);
                    self.r.cc &= !E;
                    self.r.cc |= I | F;
                    let hi = bus.read8(0xFFF0) as u16; let lo = bus.read8(0xFFF1) as u16;
                    self.r.pc = (hi<<8)|lo;
                    consumed = 20; if trace { println!("{:04X}: 11 3F   SWI3", pc0); }
                } else if sub == 0x83 { // CMPU #imm16
                    let imm = self.fetch16(bus); let _ = self.sub16_flags(self.r.u, imm); consumed = 5; if trace { println!("{:04X}: 11 83 {:04X} CMPU #${:04X}", pc0, imm, imm); }
                } else if sub == 0x93 { // CMPU <dir
                    let zp = self.fetch8(bus); let addr=self.ea_direct(zp); let hi=bus.read8(addr) as u16; let lo=bus.read8(addr.wrapping_add(1)) as u16; let v=(hi<<8)|lo; let _=self.sub16_flags(self.r.u, v); consumed=5; if trace { println!("{:04X}: 11 93 {:02X} CMPU <${:02X}", pc0, zp, zp); }
                } else if sub == 0xA3 { // CMPU indexed
                    let (ea, addcyc, desc)=self.ea_indexed(bus); let hi=bus.read8(ea) as u16; let lo=bus.read8(ea.wrapping_add(1)) as u16; let v=(hi<<8)|lo; let _=self.sub16_flags(self.r.u, v); consumed=5+addcyc; if trace { println!("{:04X}: 11 A3 ..    CMPU {}", pc0, desc); }
                } else if sub == 0xB3 { // CMPU extended
                    let addr=self.ea_extended(bus); let hi=bus.read8(addr) as u16; let lo=bus.read8(addr.wrapping_add(1)) as u16; let v=(hi<<8)|lo; let _=self.sub16_flags(self.r.u, v); consumed=6; if trace { println!("{:04X}: 11 B3 {:04X} CMPU ${:04X}", pc0, addr, addr); }
                } else {
                    consumed = 1; if trace { println!("{:04X}: 11 {:02X}   UNIMPL-PFX", pc0, sub); }
                }
            }
            0x30 => { // LEAX indexed
                let (ea, addcyc, _desc) = self.ea_indexed(bus);
                self.r.x = ea;
                // LEA family only affects Z flag
                self.r.cc = (self.r.cc & !Z) | (if self.r.x == 0 { Z } else { 0 });
                consumed = 4 + addcyc;
                if trace { println!("{:04X}: 30 ..    LEAX", pc0); }
            }
            0x31 => { // LEAY indexed
                let (ea, addcyc, _desc) = self.ea_indexed(bus);
                self.r.y = ea;
                self.r.cc = (self.r.cc & !Z) | (if self.r.y == 0 { Z } else { 0 });
                consumed = 4 + addcyc;
                if trace { println!("{:04X}: 31 ..    LEAY", pc0); }
            }
            0x8C => { // CMPX #imm16
                let imm = self.fetch16(bus);
                let x = self.r.x;
                let _ = self.sub16_flags(x, imm);
                consumed = 4; if trace { println!("{:04X}: 8C {:04X} CMPX #${:04X}", pc0, imm, imm); }
            }
            0x9C => { // CMPX <dir
                let zp = self.fetch8(bus);
                let addr = self.ea_direct(zp);
                let hi = bus.read8(addr) as u16; let lo = bus.read8(addr.wrapping_add(1)) as u16;
                let v = (hi<<8)|lo; let _ = self.sub16_flags(self.r.x, v);
                consumed = 4; if trace { println!("{:04X}: 9C {:02X} CMPX <${:02X}", pc0, zp, zp); }
            }
            0xAC => { // CMPX indexed
                let (ea, addcyc, desc) = self.ea_indexed(bus); let hi=bus.read8(ea) as u16; let lo=bus.read8(ea.wrapping_add(1)) as u16; let v=(hi<<8)|lo;
                let _ = self.sub16_flags(self.r.x, v);
                consumed = 5 + addcyc; if trace { println!("{:04X}: AC ..    CMPX {}", pc0, desc); }
            }
            0xBC => { // CMPX extended
                let addr = self.ea_extended(bus); let hi=bus.read8(addr) as u16; let lo=bus.read8(addr.wrapping_add(1)) as u16; let v=(hi<<8)|lo; let _=self.sub16_flags(self.r.x, v);
                consumed = 5; if trace { println!("{:04X}: BC {:04X} CMPX ${:04X}", pc0, addr, addr); }
            }
            0x11 => { // SWI3 or extended prefix
                let sub = self.fetch8(bus);
                if sub == 0x3F { // SWI3
                    self.r.cc |= E;
                    self.push8(bus, self.r.cc);
                    self.push8(bus, self.r.a);
                    self.push8(bus, self.r.b);
                    self.push8(bus, self.r.dp);
                    self.push16(bus, self.r.x);
                    self.push16(bus, self.r.y);
                    self.push16(bus, self.r.u);
                    self.push16(bus, self.r.pc);
                    self.r.cc |= I | F;
                    let hi = bus.read8(0xFFF0) as u16; let lo = bus.read8(0xFFF1) as u16;
                    self.r.pc = (hi<<8)|lo;
                    consumed = 20; if trace { println!("{:04X}: 11 3F   SWI3", pc0); }
                } else {
                    consumed = 1; if trace { println!("{:04X}: 11 {:02X}   UNIMPL-PFX", pc0, sub); }
                }
            }
            0x1E => { // EXG
                let pb = self.fetch8(bus);
                let src = pb >> 4; let dst = pb & 0x0F;
                let is16 = (src & 0x08)==0 && (dst & 0x08)==0;
                let is8  = (src & 0x08)!=0 && (dst & 0x08)!=0;
                if is16 { if let (Some(vs), Some(vd)) = (self.get_reg16_by_code(src), self.get_reg16_by_code(dst)) { self.set_reg16_by_code(src, vd); self.set_reg16_by_code(dst, vs); } }
                else if is8 { if let (Some(vs), Some(vd)) = (self.get_reg8_by_code(src), self.get_reg8_by_code(dst)) { self.set_reg8_by_code(src, vd); self.set_reg8_by_code(dst, vs); } }
                consumed = 6; if trace { println!("{:04X}: 1E {:02X}   EXG", pc0, pb); }
            }
            0x1F => { // TFR
                let pb = self.fetch8(bus);
                let src = pb >> 4; let dst = pb & 0x0F;
                let is16 = (src & 0x08)==0 && (dst & 0x08)==0;
                let is8  = (src & 0x08)!=0 && (dst & 0x08)!=0;
                if is16 { if let Some(vs) = self.get_reg16_by_code(src) { self.set_reg16_by_code(dst, vs); } }
                else if is8 { if let Some(vs) = self.get_reg8_by_code(src) { self.set_reg8_by_code(dst, vs); } }
                consumed = 6; if trace { println!("{:04X}: 1F {:02X}   TFR", pc0, pb); }
            }
            0x12 => { // NOP
                consumed = 2;
                if trace { println!("{:04X}: 12        NOP", pc0); }
            }
            0x86 => { // LDA #imm
                let imm = self.fetch8(bus);
                self.r.a = imm;
                self.set_nz8(self.r.a);
                self.r.cc &= !V; // V cleared, C unaffected
                consumed = 2;
                if trace { println!("{:04X}: 86 {:02X}   LDA #${:02X}", pc0, imm, imm); }
            }
            0x96 => { // LDA direct
                let zp = self.fetch8(bus);
                let addr = self.ea_direct(zp);
                self.r.a = bus.read8(addr);
                self.set_nz8(self.r.a);
                self.r.cc &= !V;
                consumed = 4;
                if trace { println!("{:04X}: 96 {:02X}   LDA ${:02X}", pc0, zp, zp); }
            }
            0xB6 => { // LDA extended
                let addr = self.ea_extended(bus);
                self.r.a = bus.read8(addr);
                self.set_nz8(self.r.a);
                self.r.cc &= !V;
                consumed = 5;
                if trace { println!("{:04X}: B6 {:04X} LDA ${:04X}", pc0, addr, addr); }
            }
            0x97 => { // STA direct
                let zp = self.fetch8(bus);
                let addr = ((self.r.dp as u16) << 8) | (zp as u16);
                bus.write8(addr, self.r.a);
                self.set_nz8(self.r.a);
                self.r.cc &= !(V);
                consumed = 4;
                if trace { println!("{:04X}: 97 {:02X}   STA ${:02X}", pc0, zp, zp); }
            }
            0xB7 => { // STA extended
                let addr = self.ea_extended(bus);
                bus.write8(addr, self.r.a);
                self.set_nz8(self.r.a);
                self.r.cc &= !V;
                consumed = 5;
                if trace { println!("{:04X}: B7 {:04X} STA ${:04X}", pc0, addr, addr); }
            }
            0xA6 => { // LDA indexed
                let (ea, addcyc, desc) = self.ea_indexed(bus);
                let v = bus.read8(ea);
                self.r.a = v;
                self.set_nz8(v);
                self.r.cc &= !V;
                consumed = 4 + addcyc;
                if trace { println!("{:04X}: A6 ..    LDA {} -> {:04X}", pc0, desc, ea); }
            }
            0xA7 => { // STA indexed
                let (ea, addcyc, desc) = self.ea_indexed(bus);
                bus.write8(ea, self.r.a);
                self.set_nz8(self.r.a);
                self.r.cc &= !V;
                consumed = 4 + addcyc;
                if trace { println!("{:04X}: A7 ..    STA {} -> {:04X}", pc0, desc, ea); }
            }
            0x7E => { // JMP extended
                let addr = self.fetch16(bus);
                self.r.pc = addr;
                consumed = 3;
                if trace { println!("{:04X}: 7E {:04X} JMP ${:04X}", pc0, addr, addr); }
            }
            0x20 => { // BRA relative
                let off = self.fetch8(bus) as i8 as i16;
                let newpc = (self.r.pc as i16).wrapping_add(off) as u16;
                self.r.pc = newpc;
                consumed = 3;
                if trace { println!("{:04X}: 20 {:02X}   BRA ${:04X}", pc0, (off as i8) as u8, newpc); }
            }
            0x16 => { // LBRA relative 16
                let off = self.fetch16(bus) as i16;
                let newpc = (self.r.pc as i32 + off as i32) as u16;
                self.r.pc = newpc;
                consumed = 5;
                if trace { println!("{:04X}: 16 {:04X} LBRA ${:04X}", pc0, off as u16, newpc); }
            }
            0x26 => { // BNE relative
                let offb = self.fetch8(bus);
                let zero = (self.r.cc & Z) != 0;
                if !zero {
                    let off = (offb as i8) as i16;
                    self.r.pc = (self.r.pc as i16).wrapping_add(off) as u16;
                    consumed = 3;
                } else {
                    consumed = 2;
                }
                if trace { println!("{:04X}: 26 {:02X}   BNE {}", pc0, offb, if !zero {"taken"} else {"not"}); }
            }
            0x17 => { // LBSR relative 16
                let off = self.fetch16(bus) as i16;
                let ret = self.r.pc;
                self.r.s = self.r.s.wrapping_sub(1); bus.write8(self.r.s, (ret & 0x00FF) as u8);
                self.r.s = self.r.s.wrapping_sub(1); bus.write8(self.r.s, (ret >> 8) as u8);
                self.r.pc = (self.r.pc as i32 + off as i32) as u16;
                consumed = 9;
                if trace { println!("{:04X}: 17 {:04X} LBSR", pc0, off as u16); }
            }
            0x39 => { // RTS
                let hi = bus.read8(self.r.s); self.r.s = self.r.s.wrapping_add(1);
                let lo = bus.read8(self.r.s); self.r.s = self.r.s.wrapping_add(1);
                self.r.pc = ((hi as u16) << 8) | (lo as u16);
                consumed = 5;
                if trace { println!("{:04X}: 39        RTS", pc0); }
            }
            0x3B => { // RTI
                let new_cc = self.pull8(bus);
                let e_set = (new_cc & E) != 0;
                self.r.cc = new_cc;
                if e_set {
                    self.r.a = self.pull8(bus);
                    self.r.b = self.pull8(bus);
                    self.r.dp = self.pull8(bus);
                    self.r.x = self.pull16(bus);
                    self.r.y = self.pull16(bus);
                    self.r.u = self.pull16(bus);
                }
                self.r.pc = self.pull16(bus);
                // 6809: full frame ~15, minimal ~12
                consumed = if e_set { 15 } else { 12 };
                if trace { println!("{:04X}: 3B        RTI", pc0); }
            }
            0x3F => { // SWI
                self.r.cc |= E;
                self.push8(bus, self.r.cc);
                self.push8(bus, self.r.a);
                self.push8(bus, self.r.b);
                self.push8(bus, self.r.dp);
                self.push16(bus, self.r.x);
                self.push16(bus, self.r.y);
                self.push16(bus, self.r.u);
                self.push16(bus, self.r.pc);
                self.r.cc |= I | F;
                let hi = bus.read8(0xFFF8) as u16; let lo = bus.read8(0xFFF9) as u16;
                self.r.pc = (hi<<8)|lo;
                consumed = 19;
                if trace { println!("{:04X}: 3F        SWI", pc0); }
            }
            0x8D => { // BSR rel
                let off = self.fetch8(bus) as i8 as i16;
                let ret = self.r.pc;
                // Push return address on S
                self.r.s = self.r.s.wrapping_sub(1); bus.write8(self.r.s, (ret & 0x00FF) as u8);
                self.r.s = self.r.s.wrapping_sub(1); bus.write8(self.r.s, (ret >> 8) as u8);
                self.r.pc = (self.r.pc as i16).wrapping_add(off) as u16;
                consumed = 7;
                if trace { println!("{:04X}: 8D {:02X}   BSR", pc0, (off as i8) as u8); }
            }
            0x34 => { // PSHS
                let pb = self.fetch8(bus);
                let mut bytes = 0u32;
                if (pb & 0x80)!=0 { self.push16(bus, self.r.pc); bytes+=2; }
                if (pb & 0x40)!=0 { self.push16(bus, self.r.u); bytes+=2; }
                if (pb & 0x20)!=0 { self.push16(bus, self.r.y); bytes+=2; }
                if (pb & 0x10)!=0 { self.push16(bus, self.r.x); bytes+=2; }
                if (pb & 0x08)!=0 { self.push8(bus, self.r.dp); bytes+=1; }
                if (pb & 0x04)!=0 { self.push8(bus, self.r.b); bytes+=1; }
                if (pb & 0x02)!=0 { self.push8(bus, self.r.a); bytes+=1; }
                if (pb & 0x01)!=0 { self.push8(bus, self.r.cc); bytes+=1; }
                consumed = 5 + bytes; if trace { println!("{:04X}: 34 {:02X}   PSHS", pc0, pb); }
            }
            0x36 => { // PSHU
                let pb = self.fetch8(bus);
                let mut bytes = 0u32;
                if (pb & 0x80)!=0 { self.upush16(bus, self.r.pc); bytes+=2; }
                if (pb & 0x40)!=0 { self.upush16(bus, self.r.s); bytes+=2; }
                if (pb & 0x20)!=0 { self.upush16(bus, self.r.y); bytes+=2; }
                if (pb & 0x10)!=0 { self.upush16(bus, self.r.x); bytes+=2; }
                if (pb & 0x08)!=0 { self.upush8(bus, self.r.dp); bytes+=1; }
                if (pb & 0x04)!=0 { self.upush8(bus, self.r.b); bytes+=1; }
                if (pb & 0x02)!=0 { self.upush8(bus, self.r.a); bytes+=1; }
                if (pb & 0x01)!=0 { self.upush8(bus, self.r.cc); bytes+=1; }
                consumed = 5 + bytes; if trace { println!("{:04X}: 36 {:02X}   PSHU", pc0, pb); }
            }
            0x35 => { // PULS
                let pb = self.fetch8(bus);
                let mut bytes = 0u32;
                if (pb & 0x01)!=0 { self.r.cc = self.pull8(bus); bytes+=1; }
                if (pb & 0x02)!=0 { self.r.a  = self.pull8(bus); bytes+=1; }
                if (pb & 0x04)!=0 { self.r.b  = self.pull8(bus); bytes+=1; }
                if (pb & 0x08)!=0 { self.r.dp = self.pull8(bus); bytes+=1; }
                if (pb & 0x10)!=0 { self.r.x  = self.pull16(bus); bytes+=2; }
                if (pb & 0x20)!=0 { self.r.y  = self.pull16(bus); bytes+=2; }
                if (pb & 0x40)!=0 { self.r.u  = self.pull16(bus); bytes+=2; }
                if (pb & 0x80)!=0 { self.r.pc = self.pull16(bus); bytes+=2; }
                consumed = 5 + bytes; if trace { println!("{:04X}: 35 {:02X}   PULS", pc0, pb); }
            }
            0x37 => { // PULU
                let pb = self.fetch8(bus);
                let mut bytes = 0u32;
                if (pb & 0x01)!=0 { self.r.cc = self.upull8(bus); bytes+=1; }
                if (pb & 0x02)!=0 { self.r.a  = self.upull8(bus); bytes+=1; }
                if (pb & 0x04)!=0 { self.r.b  = self.upull8(bus); bytes+=1; }
                if (pb & 0x08)!=0 { self.r.dp = self.upull8(bus); bytes+=1; }
                if (pb & 0x10)!=0 { self.r.x  = self.upull16(bus); bytes+=2; }
                if (pb & 0x20)!=0 { self.r.y  = self.upull16(bus); bytes+=2; }
                if (pb & 0x40)!=0 { self.r.s  = self.upull16(bus); bytes+=2; }
                if (pb & 0x80)!=0 { self.r.pc = self.upull16(bus); bytes+=2; }
                consumed = 5 + bytes; if trace { println!("{:04X}: 37 {:02X}   PULU", pc0, pb); }
            }
            0xBD => { // JSR extended
                let addr = self.fetch16(bus);
                let ret = self.r.pc;
                self.r.s = self.r.s.wrapping_sub(1); bus.write8(self.r.s, (ret & 0x00FF) as u8);
                self.r.s = self.r.s.wrapping_sub(1); bus.write8(self.r.s, (ret >> 8) as u8);
                self.r.pc = addr;
                consumed = 7;
                if trace { println!("{:04X}: BD {:04X} JSR ${:04X}", pc0, addr, addr); }
            }
            0x8E => { // LDX #imm16
                let imm = self.fetch16(bus);
                self.r.x = imm;
                self.set_nz16(self.r.x);
                self.r.cc &= !V;
                consumed = 3;
                if trace { println!("{:04X}: 8E {:04X} LDX #${:04X}", pc0, imm, imm); }
            }
            // Accumulator shifts/rotates/bit ops (A)
            0x48 => { // ASLA
                let c_out = (self.r.a & 0x80) != 0;
                let res = self.r.a.wrapping_shl(1);
                self.r.a = res;
                self.set_nz8(res);
                let n = (res & 0x80) != 0; let c = c_out;
                self.r.cc = (self.r.cc & !(C|V)) | (if c {C} else {0}) | (if (n as u8) ^ (c as u8) != 0 {V} else {0});
                consumed = 2; if trace { println!("{:04X}: 48        ASLA", pc0); }
            }
            0x44 => { // LSRA
                let c_out = (self.r.a & 0x01) != 0;
                let res = self.r.a >> 1;
                self.r.a = res;
                self.set_nz8(res);
                self.r.cc = (self.r.cc & !(C|V|N)) | (if c_out {C} else {0}) | 0; // N cleared for LSR
                consumed = 2; if trace { println!("{:04X}: 44        LSRA", pc0); }
            }
            0x49 => { // ROLA
                let c_in = (self.r.cc & C) != 0;
                let c_out = (self.r.a & 0x80) != 0;
                let res = (self.r.a << 1) | (if c_in {1} else {0});
                self.r.a = res;
                self.set_nz8(res);
                let n = (res & 0x80) != 0; let c = c_out;
                self.r.cc = (self.r.cc & !(C|V)) | (if c {C} else {0}) | (if (n as u8) ^ (c as u8) != 0 {V} else {0});
                consumed = 2; if trace { println!("{:04X}: 49        ROLA", pc0); }
            }
            0x46 => { // RORA
                let c_in = (self.r.cc & C) != 0;
                let c_out = (self.r.a & 0x01) != 0;
                let res = (self.r.a >> 1) | (if c_in {0x80} else {0});
                self.r.a = res;
                self.set_nz8(res);
                self.r.cc = (self.r.cc & !(C|V)) | (if c_out {C} else {0});
                consumed = 2; if trace { println!("{:04X}: 46        RORA", pc0); }
            }
            0x43 => { // COMA
                let res = !self.r.a;
                self.r.a = res;
                self.set_nz8(res);
                self.r.cc = (self.r.cc & !V) | C; // V=0, C=1
                consumed = 2; if trace { println!("{:04X}: 43        COMA", pc0); }
            }
            0x4F => { // CLRA
                self.r.a = 0; self.set_nz8(0); self.r.cc &= !N; self.r.cc &= !V; self.r.cc &= !C; self.r.cc |= Z;
                consumed = 2; if trace { println!("{:04X}: 4F        CLRA", pc0); }
            }
            0x4D => { // TSTA
                let v = self.r.a; self.set_nz8(v); self.r.cc &= !V; consumed = 2; if trace { println!("{:04X}: 4D        TSTA", pc0); }
            }
            0x4A => { // DECA
                let prev = self.r.a; let res = prev.wrapping_sub(1); self.r.a = res; self.set_nz8(res); self.r.cc &= !C; self.r.cc = (self.r.cc & !V) | (if prev==0x80 {V} else {0}); consumed=2; if trace { println!("{:04X}: 4A        DECA", pc0); }
            }
            0x4C => { // INCA
                let prev = self.r.a; let res = prev.wrapping_add(1); self.r.a = res; self.set_nz8(res); self.r.cc &= !C; self.r.cc = (self.r.cc & !V) | (if prev==0x7F {V} else {0}); consumed=2; if trace { println!("{:04X}: 4C        INCA", pc0); }
            }
            // Accumulator shifts/rotates/bit ops (B)
            0x58 => { let c_out=(self.r.b & 0x80)!=0; let res=self.r.b.wrapping_shl(1); self.r.b=res; self.set_nz8(res); let n=(res&0x80)!=0; let c=c_out; self.r.cc=(self.r.cc & !(C|V)) | (if c{C}else{0}) | (if (n as u8)^(c as u8)!=0 {V}else{0}); consumed=2; if trace { println!("{:04X}: 58        ASLB", pc0); } }
            0x54 => { let c_out=(self.r.b & 0x01)!=0; let res=self.r.b>>1; self.r.b=res; self.set_nz8(res); self.r.cc=(self.r.cc & !(C|V|N)) | (if c_out{C}else{0}); consumed=2; if trace { println!("{:04X}: 54        LSRB", pc0); } }
            0x59 => { let c_in=(self.r.cc&C)!=0; let c_out=(self.r.b & 0x80)!=0; let res=(self.r.b<<1) | (if c_in{1}else{0}); self.r.b=res; self.set_nz8(res); let n=(res&0x80)!=0; let c=c_out; self.r.cc=(self.r.cc & !(C|V)) | (if c{C}else{0}) | (if (n as u8)^(c as u8)!=0 {V}else{0}); consumed=2; if trace { println!("{:04X}: 59        ROLB", pc0); } }
            0x56 => { let c_in=(self.r.cc&C)!=0; let c_out=(self.r.b & 0x01)!=0; let res=(self.r.b>>1) | (if c_in{0x80}else{0}); self.r.b=res; self.set_nz8(res); self.r.cc=(self.r.cc & !(C|V)) | (if c_out{C}else{0}); consumed=2; if trace { println!("{:04X}: 56        RORB", pc0); } }
            0x53 => { let res=!self.r.b; self.r.b=res; self.set_nz8(res); self.r.cc=(self.r.cc & !V) | C; consumed=2; if trace { println!("{:04X}: 53        COMB", pc0); } }
            0x5F => { self.r.b=0; self.set_nz8(0); self.r.cc &= !N; self.r.cc &= !V; self.r.cc &= !C; self.r.cc |= Z; consumed=2; if trace { println!("{:04X}: 5F        CLRB", pc0); } }
            0x5D => { let v=self.r.b; self.set_nz8(v); self.r.cc &= !V; consumed=2; if trace { println!("{:04X}: 5D        TSTB", pc0); } }
            0x5A => { let prev=self.r.b; let res=prev.wrapping_sub(1); self.r.b=res; self.set_nz8(res); self.r.cc &= !C; self.r.cc=(self.r.cc & !V) | (if prev==0x80 {V} else {0}); consumed=2; if trace { println!("{:04X}: 5A        DECB", pc0); } }
            0x5C => { let prev=self.r.b; let res=prev.wrapping_add(1); self.r.b=res; self.set_nz8(res); self.r.cc &= !C; self.r.cc=(self.r.cc & !V) | (if prev==0x7F {V} else {0}); consumed=2; if trace { println!("{:04X}: 5C        INCB", pc0); } }

            // 16-bit arithmetic on D (A:B)
            0x83 => { // SUBD #imm16
                let imm = self.fetch16(bus);
                let d = ((self.r.a as u16) << 8) | (self.r.b as u16);
                let res = self.sub16_flags(d, imm);
                self.r.a = (res >> 8) as u8; self.r.b = (res & 0xFF) as u8;
                consumed = 4; if trace { println!("{:04X}: 83 {:04X} SUBD #${:04X}", pc0, imm, imm); }
            }
            0x93 => { // SUBD direct
                let zp = self.fetch8(bus); let addr=self.ea_direct(zp);
                let hi = bus.read8(addr) as u16; let lo = bus.read8(addr.wrapping_add(1)) as u16; let imm=(hi<<8)|lo;
                let d = ((self.r.a as u16) << 8) | (self.r.b as u16);
                let res = self.sub16_flags(d, imm);
                self.r.a = (res >> 8) as u8; self.r.b = (res & 0xFF) as u8;
                consumed = 6; if trace { println!("{:04X}: 93 {:02X}   SUBD ${:02X}", pc0, zp, zp); }
            }
            0xA3 => { // SUBD indexed
                let (ea, add, desc) = self.ea_indexed(bus);
                let hi=bus.read8(ea) as u16; let lo=bus.read8(ea.wrapping_add(1)) as u16; let imm=(hi<<8)|lo;
                let d=((self.r.a as u16)<<8)|(self.r.b as u16);
                let res=self.sub16_flags(d, imm);
                self.r.a=(res>>8) as u8; self.r.b=(res&0xFF) as u8;
                consumed = 6 + add; if trace { println!("{:04X}: A3 ..    SUBD {}", pc0, desc); }
            }
            0xB3 => { // SUBD extended
                let addr=self.ea_extended(bus);
                let hi=bus.read8(addr) as u16; let lo=bus.read8(addr.wrapping_add(1)) as u16; let imm=(hi<<8)|lo;
                let d=((self.r.a as u16)<<8)|(self.r.b as u16);
                let res=self.sub16_flags(d, imm);
                self.r.a=(res>>8) as u8; self.r.b=(res&0xFF) as u8;
                consumed = 7; if trace { println!("{:04X}: B3 {:04X} SUBD ${:04X}", pc0, addr, addr); }
            }
            0xC3 => { // ADDD #imm16
                let imm = self.fetch16(bus);
                let d = ((self.r.a as u16) << 8) | (self.r.b as u16);
                let res = self.add16_flags(d, imm);
                self.r.a = (res >> 8) as u8; self.r.b = (res & 0xFF) as u8;
                consumed = 4; if trace { println!("{:04X}: C3 {:04X} ADDD #${:04X}", pc0, imm, imm); }
            }
            0xD3 => { // ADDD direct
                let zp = self.fetch8(bus); let addr=self.ea_direct(zp);
                let hi = bus.read8(addr) as u16; let lo = bus.read8(addr.wrapping_add(1)) as u16; let imm=(hi<<8)|lo;
                let d = ((self.r.a as u16) << 8) | (self.r.b as u16);
                let res = self.add16_flags(d, imm);
                self.r.a = (res >> 8) as u8; self.r.b = (res & 0xFF) as u8;
                consumed = 6; if trace { println!("{:04X}: D3 {:02X}   ADDD ${:02X}", pc0, zp, zp); }
            }
            0xE3 => { // ADDD indexed
                let (ea, add, desc) = self.ea_indexed(bus);
                let hi=bus.read8(ea) as u16; let lo=bus.read8(ea.wrapping_add(1)) as u16; let imm=(hi<<8)|lo;
                let d=((self.r.a as u16)<<8)|(self.r.b as u16);
                let res=self.add16_flags(d, imm);
                self.r.a=(res>>8) as u8; self.r.b=(res&0xFF) as u8;
                consumed = 6 + add; if trace { println!("{:04X}: E3 ..    ADDD {}", pc0, desc); }
            }
            0xF3 => { // ADDD extended
                let addr=self.ea_extended(bus);
                let hi=bus.read8(addr) as u16; let lo=bus.read8(addr.wrapping_add(1)) as u16; let imm=(hi<<8)|lo;
                let d=((self.r.a as u16)<<8)|(self.r.b as u16);
                let res=self.add16_flags(d, imm);
                self.r.a=(res>>8) as u8; self.r.b=(res&0xFF) as u8;
                consumed = 7; if trace { println!("{:04X}: F3 {:04X} ADDD ${:04X}", pc0, addr, addr); }
            }
            // 8-bit ALU A immediate group
            0x80 => { // SUBA #
                let imm = self.fetch8(bus);
                let a = self.r.a;
                self.r.a = self.sub8_flags(a, imm);
                consumed = 2;
                if trace { println!("{:04X}: 80 {:02X}   SUBA #${:02X}", pc0, imm, imm); }
            }
            0x81 => { // CMPA #
                let imm = self.fetch8(bus);
                let a = self.r.a;
                let _ = self.sub8_flags(a, imm);
                self.r.a = a; // no store
                consumed = 2;
                if trace { println!("{:04X}: 81 {:02X}   CMPA #${:02X}", pc0, imm, imm); }
            }
            0x82 => { // SBCA #
                let imm = self.fetch8(bus);
                let a = self.r.a;
                self.r.a = self.sbc8_flags(a, imm);
                consumed = 2;
                if trace { println!("{:04X}: 82 {:02X}   SBCA #${:02X}", pc0, imm, imm); }
            }
            0x84 => { // ANDA #
                let imm = self.fetch8(bus);
                let v = self.r.a & imm;
                self.logic8_nz_clearv(v);
                self.r.a = v;
                consumed = 2;
                if trace { println!("{:04X}: 84 {:02X}   ANDA #${:02X}", pc0, imm, imm); }
            }
            0x85 => { // BITA # (test)
                let imm = self.fetch8(bus);
                let v = self.r.a & imm;
                self.logic8_nz_clearv(v);
                consumed = 2;
                if trace { println!("{:04X}: 85 {:02X}   BITA #${:02X}", pc0, imm, imm); }
            }
            0x88 => { // EORA #
                let imm = self.fetch8(bus);
                let v = self.r.a ^ imm;
                self.logic8_nz_clearv(v);
                self.r.a = v;
                consumed = 2;
                if trace { println!("{:04X}: 88 {:02X}   EORA #${:02X}", pc0, imm, imm); }
            }
            0x89 => { // ADCA #
                let imm = self.fetch8(bus);
                let a = self.r.a;
                self.r.a = self.adc8_flags(a, imm);
                consumed = 2;
                if trace { println!("{:04X}: 89 {:02X}   ADCA #${:02X}", pc0, imm, imm); }
            }
            0x8A => { // ORA #
                let imm = self.fetch8(bus);
                let v = self.r.a | imm;
                self.logic8_nz_clearv(v);
                self.r.a = v;
                consumed = 2;
                if trace { println!("{:04X}: 8A {:02X}   ORA #${:02X}", pc0, imm, imm); }
            }
            // Direct variants A (0x90..)
            0x90 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); self.r.a = self.sub8_flags(self.r.a, v); consumed=4; if trace { println!("{:04X}: 90 {:02X}   SUBA ${:02X}", pc0, zp, zp); } }
            0x91 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let a=self.r.a; let _=self.sub8_flags(a, v); self.r.a=a; consumed=4; if trace { println!("{:04X}: 91 {:02X}   CMPA ${:02X}", pc0, zp, zp); } }
            0x92 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let a=self.r.a; self.r.a=self.sbc8_flags(a, v); consumed=4; if trace { println!("{:04X}: 92 {:02X}   SBCA ${:02X}", pc0, zp, zp); } }
            0x94 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let res=self.r.a & v; self.logic8_nz_clearv(res); self.r.a=res; consumed=4; if trace { println!("{:04X}: 94 {:02X}   ANDA ${:02X}", pc0, zp, zp); } }
            0x95 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let res=self.r.a & v; self.logic8_nz_clearv(res); consumed=4; if trace { println!("{:04X}: 95 {:02X}   BITA ${:02X}", pc0, zp, zp); } }
            0x98 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let res=self.r.a ^ v; self.logic8_nz_clearv(res); self.r.a=res; consumed=4; if trace { println!("{:04X}: 98 {:02X}   EORA ${:02X}", pc0, zp, zp); } }
            0x99 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let a=self.r.a; self.r.a=self.adc8_flags(a, v); consumed=4; if trace { println!("{:04X}: 99 {:02X}   ADCA ${:02X}", pc0, zp, zp); } }
            0x9A => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let res=self.r.a | v; self.logic8_nz_clearv(res); self.r.a=res; consumed=4; if trace { println!("{:04X}: 9A {:02X}   ORA ${:02X}", pc0, zp, zp); } }
            0x9B => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let a=self.r.a; self.r.a=self.add8_flags(a, v); consumed=4; if trace { println!("{:04X}: 9B {:02X}   ADDA ${:02X}", pc0, zp, zp); } }
            // Indexed variants A (0xA0..)
            0xA0 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); self.r.a=self.sub8_flags(self.r.a, v); consumed=4+ac; if trace { println!("{:04X}: A0 ..    SUBA {}", pc0, desc); } }
            0xA1 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let a=self.r.a; let _=self.sub8_flags(a, v); self.r.a=a; consumed=4+ac; if trace { println!("{:04X}: A1 ..    CMPA {}", pc0, desc); } }
            0xA2 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let a=self.r.a; self.r.a=self.sbc8_flags(a, v); consumed=4+ac; if trace { println!("{:04X}: A2 ..    SBCA {}", pc0, desc); } }
            0xA4 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let res=self.r.a & v; self.logic8_nz_clearv(res); self.r.a=res; consumed=4+ac; if trace { println!("{:04X}: A4 ..    ANDA {}", pc0, desc); } }
            0xA5 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let res=self.r.a & v; self.logic8_nz_clearv(res); consumed=4+ac; if trace { println!("{:04X}: A5 ..    BITA {}", pc0, desc); } }
            0xA8 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let res=self.r.a ^ v; self.logic8_nz_clearv(res); self.r.a=res; consumed=4+ac; if trace { println!("{:04X}: A8 ..    EORA {}", pc0, desc); } }
            0xA9 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let a=self.r.a; self.r.a=self.adc8_flags(a, v); consumed=4+ac; if trace { println!("{:04X}: A9 ..    ADCA {}", pc0, desc); } }
            0xAA => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let res=self.r.a | v; self.logic8_nz_clearv(res); self.r.a=res; consumed=4+ac; if trace { println!("{:04X}: AA ..    ORA {}", pc0, desc); } }
            0xAB => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let a=self.r.a; self.r.a=self.add8_flags(a, v); consumed=4+ac; if trace { println!("{:04X}: AB ..    ADDA {}", pc0, desc); } }
            // Extended variants A (0xB0..)
            0xB0 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); self.r.a=self.sub8_flags(self.r.a, v); consumed=5; if trace { println!("{:04X}: B0 {:04X} SUBA ${:04X}", pc0, addr, addr); } }
            0xB1 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let a=self.r.a; let _=self.sub8_flags(a, v); self.r.a=a; consumed=5; if trace { println!("{:04X}: B1 {:04X} CMPA ${:04X}", pc0, addr, addr); } }
            0xB2 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let a=self.r.a; self.r.a=self.sbc8_flags(a, v); consumed=5; if trace { println!("{:04X}: B2 {:04X} SBCA ${:04X}", pc0, addr, addr); } }
            0xB4 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let res=self.r.a & v; self.logic8_nz_clearv(res); self.r.a=res; consumed=5; if trace { println!("{:04X}: B4 {:04X} ANDA ${:04X}", pc0, addr, addr); } }
            0xB5 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let res=self.r.a & v; self.logic8_nz_clearv(res); consumed=5; if trace { println!("{:04X}: B5 {:04X} BITA ${:04X}", pc0, addr, addr); } }
            0xB8 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let res=self.r.a ^ v; self.logic8_nz_clearv(res); self.r.a=res; consumed=5; if trace { println!("{:04X}: B8 {:04X} EORA ${:04X}", pc0, addr, addr); } }
            0xB9 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let a=self.r.a; self.r.a=self.adc8_flags(a, v); consumed=5; if trace { println!("{:04X}: B9 {:04X} ADCA ${:04X}", pc0, addr, addr); } }
            0xBA => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let res=self.r.a | v; self.logic8_nz_clearv(res); self.r.a=res; consumed=5; if trace { println!("{:04X}: BA {:04X} ORA ${:04X}", pc0, addr, addr); } }
            0xBB => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let a=self.r.a; self.r.a=self.add8_flags(a, v); consumed=5; if trace { println!("{:04X}: BB {:04X} ADDA ${:04X}", pc0, addr, addr); } }

            // 8-bit ALU B immediate group (0xC0..)
            0xC0 => { let imm=self.fetch8(bus); let b=self.r.b; self.r.b=self.sub8_flags(b, imm); consumed=2; if trace { println!("{:04X}: C0 {:02X}   SUBB #${:02X}", pc0, imm, imm); } }
            0xC1 => { let imm=self.fetch8(bus); let b=self.r.b; let _=self.sub8_flags(b, imm); self.r.b=b; consumed=2; if trace { println!("{:04X}: C1 {:02X}   CMPB #${:02X}", pc0, imm, imm); } }
            0xC2 => { let imm=self.fetch8(bus); let b=self.r.b; self.r.b=self.sbc8_flags(b, imm); consumed=2; if trace { println!("{:04X}: C2 {:02X}   SBCB #${:02X}", pc0, imm, imm); } }
            0xC4 => { let imm=self.fetch8(bus); let v=self.r.b & imm; self.logic8_nz_clearv(v); self.r.b=v; consumed=2; if trace { println!("{:04X}: C4 {:02X}   ANDB #${:02X}", pc0, imm, imm); } }
            0xC5 => { let imm=self.fetch8(bus); let v=self.r.b & imm; self.logic8_nz_clearv(v); consumed=2; if trace { println!("{:04X}: C5 {:02X}   BITB #${:02X}", pc0, imm, imm); } }
            0xC6 => { let imm=self.fetch8(bus); self.r.b=imm; self.set_nz8(imm); self.r.cc &= !V; consumed=2; if trace { println!("{:04X}: C6 {:02X}   LDB #${:02X}", pc0, imm, imm); } }
            0xC8 => { let imm=self.fetch8(bus); let v=self.r.b ^ imm; self.logic8_nz_clearv(v); self.r.b=v; consumed=2; if trace { println!("{:04X}: C8 {:02X}   EORB #${:02X}", pc0, imm, imm); } }
            0xC9 => { let imm=self.fetch8(bus); let b=self.r.b; self.r.b=self.adc8_flags(b, imm); consumed=2; if trace { println!("{:04X}: C9 {:02X}   ADCB #${:02X}", pc0, imm, imm); } }
            0xCA => { let imm=self.fetch8(bus); let v=self.r.b | imm; self.logic8_nz_clearv(v); self.r.b=v; consumed=2; if trace { println!("{:04X}: CA {:02X}   ORB #${:02X}", pc0, imm, imm); } }
            0xCB => { let imm=self.fetch8(bus); let b=self.r.b; self.r.b=self.add8_flags(b, imm); consumed=2; if trace { println!("{:04X}: CB {:02X}   ADDB #${:02X}", pc0, imm, imm); } }
            // Direct B (0xD0..)
            0xD0 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); self.r.b=self.sub8_flags(self.r.b, v); consumed=4; if trace { println!("{:04X}: D0 {:02X}   SUBB ${:02X}", pc0, zp, zp); } }
            0xD1 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let b=self.r.b; let _=self.sub8_flags(b, v); self.r.b=b; consumed=4; if trace { println!("{:04X}: D1 {:02X}   CMPB ${:02X}", pc0, zp, zp); } }
            0xD2 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let b=self.r.b; self.r.b=self.sbc8_flags(b, v); consumed=4; if trace { println!("{:04X}: D2 {:02X}   SBCB ${:02X}", pc0, zp, zp); } }
            0xD4 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let res=self.r.b & v; self.logic8_nz_clearv(res); self.r.b=res; consumed=4; if trace { println!("{:04X}: D4 {:02X}   ANDB ${:02X}", pc0, zp, zp); } }
            0xD5 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let res=self.r.b & v; self.logic8_nz_clearv(res); consumed=4; if trace { println!("{:04X}: D5 {:02X}   BITB ${:02X}", pc0, zp, zp); } }
            0xD6 => { let zp=self.fetch8(bus); let addr=self.ea_direct(zp); let v=bus.read8(addr); self.r.b=v; self.set_nz8(v); self.r.cc &= !V; consumed=4; if trace { println!("{:04X}: D6 {:02X}   LDB ${:02X}", pc0, zp, zp); } }
            0xD7 => { let zp=self.fetch8(bus); let addr=self.ea_direct(zp); bus.write8(addr, self.r.b); self.set_nz8(self.r.b); self.r.cc &= !V; consumed=4; if trace { println!("{:04X}: D7 {:02X}   STB ${:02X}", pc0, zp, zp); } }
            0xD8 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let res=self.r.b ^ v; self.logic8_nz_clearv(res); self.r.b=res; consumed=4; if trace { println!("{:04X}: D8 {:02X}   EORB ${:02X}", pc0, zp, zp); } }
            0xD9 => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let b=self.r.b; self.r.b=self.adc8_flags(b, v); consumed=4; if trace { println!("{:04X}: D9 {:02X}   ADCB ${:02X}", pc0, zp, zp); } }
            0xDA => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let res=self.r.b | v; self.logic8_nz_clearv(res); self.r.b=res; consumed=4; if trace { println!("{:04X}: DA {:02X}   ORB ${:02X}", pc0, zp, zp); } }
            0xDB => { let zp=self.fetch8(bus); let v=bus.read8(self.ea_direct(zp)); let b=self.r.b; self.r.b=self.add8_flags(b, v); consumed=4; if trace { println!("{:04X}: DB {:02X}   ADDB ${:02X}", pc0, zp, zp); } }
            // Indexed B (0xE0..)
            0xE0 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); self.r.b=self.sub8_flags(self.r.b, v); consumed=4+ac; if trace { println!("{:04X}: E0 ..    SUBB {}", pc0, desc); } }
            0xE1 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let b=self.r.b; let _=self.sub8_flags(b, v); self.r.b=b; consumed=4+ac; if trace { println!("{:04X}: E1 ..    CMPB {}", pc0, desc); } }
            0xE2 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let b=self.r.b; self.r.b=self.sbc8_flags(b, v); consumed=4+ac; if trace { println!("{:04X}: E2 ..    SBCB {}", pc0, desc); } }
            0xE4 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let res=self.r.b & v; self.logic8_nz_clearv(res); self.r.b=res; consumed=4+ac; if trace { println!("{:04X}: E4 ..    ANDB {}", pc0, desc); } }
            0xE5 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let res=self.r.b & v; self.logic8_nz_clearv(res); consumed=4+ac; if trace { println!("{:04X}: E5 ..    BITB {}", pc0, desc); } }
            0xE6 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); self.r.b=v; self.set_nz8(v); self.r.cc &= !V; consumed=4+ac; if trace { println!("{:04X}: E6 ..    LDB {}", pc0, desc); } }
            0xE7 => { let (ea,ac,desc)=self.ea_indexed(bus); bus.write8(ea, self.r.b); self.set_nz8(self.r.b); self.r.cc &= !V; consumed=4+ac; if trace { println!("{:04X}: E7 ..    STB {}", pc0, desc); } }
            0xE8 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let res=self.r.b ^ v; self.logic8_nz_clearv(res); self.r.b=res; consumed=4+ac; if trace { println!("{:04X}: E8 ..    EORB {}", pc0, desc); } }
            0xE9 => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let b=self.r.b; self.r.b=self.adc8_flags(b, v); consumed=4+ac; if trace { println!("{:04X}: E9 ..    ADCB {}", pc0, desc); } }
            0xEA => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let res=self.r.b | v; self.logic8_nz_clearv(res); self.r.b=res; consumed=4+ac; if trace { println!("{:04X}: EA ..    ORB {}", pc0, desc); } }
            0xEB => { let (ea,ac,desc)=self.ea_indexed(bus); let v=bus.read8(ea); let b=self.r.b; self.r.b=self.add8_flags(b, v); consumed=4+ac; if trace { println!("{:04X}: EB ..    ADDB {}", pc0, desc); } }
            // Extended B (0xF0..)
            0xF0 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); self.r.b=self.sub8_flags(self.r.b, v); consumed=5; if trace { println!("{:04X}: F0 {:04X} SUBB ${:04X}", pc0, addr, addr); } }
            0xF1 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let b=self.r.b; let _=self.sub8_flags(b, v); self.r.b=b; consumed=5; if trace { println!("{:04X}: F1 {:04X} CMPB ${:04X}", pc0, addr, addr); } }
            0xF2 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let b=self.r.b; self.r.b=self.sbc8_flags(b, v); consumed=5; if trace { println!("{:04X}: F2 {:04X} SBCB ${:04X}", pc0, addr, addr); } }
            0xF4 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let res=self.r.b & v; self.logic8_nz_clearv(res); self.r.b=res; consumed=5; if trace { println!("{:04X}: F4 {:04X} ANDB ${:04X}", pc0, addr, addr); } }
            0xF5 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let res=self.r.b & v; self.logic8_nz_clearv(res); consumed=5; if trace { println!("{:04X}: F5 {:04X} BITB ${:04X}", pc0, addr, addr); } }
            0xF6 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); self.r.b=v; self.set_nz8(v); self.r.cc &= !V; consumed=5; if trace { println!("{:04X}: F6 {:04X} LDB ${:04X}", pc0, addr, addr); } }
            0xF7 => { let addr=self.ea_extended(bus); bus.write8(addr, self.r.b); self.set_nz8(self.r.b); self.r.cc &= !V; consumed=5; if trace { println!("{:04X}: F7 {:04X} STB ${:04X}", pc0, addr, addr); } }
            0xF8 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let res=self.r.b ^ v; self.logic8_nz_clearv(res); self.r.b=res; consumed=5; if trace { println!("{:04X}: F8 {:04X} EORB ${:04X}", pc0, addr, addr); } }
            0xF9 => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let b=self.r.b; self.r.b=self.adc8_flags(b, v); consumed=5; if trace { println!("{:04X}: F9 {:04X} ADCB ${:04X}", pc0, addr, addr); } }
            0xFA => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let res=self.r.b | v; self.logic8_nz_clearv(res); self.r.b=res; consumed=5; if trace { println!("{:04X}: FA {:04X} ORB ${:04X}", pc0, addr, addr); } }
            0xFB => { let addr=self.ea_extended(bus); let v=bus.read8(addr); let b=self.r.b; self.r.b=self.add8_flags(b, v); consumed=5; if trace { println!("{:04X}: FB {:04X} ADDB ${:04X}", pc0, addr, addr); } }
            0x8B => { // ADDA #imm
                let imm = self.fetch8(bus);
                let a = self.r.a;
                let (res8, carry1) = a.overflowing_add(imm);
                let half = ((a & 0x0F) + (imm & 0x0F)) > 0x0F;
                let ovf = ((a ^ res8) & (imm ^ res8) & 0x80) != 0;
                self.r.a = res8;
                self.set_nz8(res8);
                self.r.cc = (self.r.cc & !(C|V|H)) | (if carry1 {C} else {0}) | (if ovf {V} else {0}) | (if half {H} else {0});
                consumed = 2;
                if trace { println!("{:04X}: 8B {:02X}   ADDA #${:02X}", pc0, imm, imm); }
            }
            0xAE => { // LDX indexed
                let (ea, addcyc, desc) = self.ea_indexed(bus);
                let hi = bus.read8(ea) as u16; let lo = bus.read8(ea.wrapping_add(1)) as u16;
                let v = (hi<<8)|lo;
                self.r.x = v;
                self.set_nz16(v);
                self.r.cc &= !V;
                consumed = 5 + addcyc;
                if trace { println!("{:04X}: AE ..    LDX {} -> {:04X}", pc0, desc, ea); }
            }
            0xAF => { // STX indexed
                let (ea, addcyc, desc) = self.ea_indexed(bus);
                bus.write8(ea, (self.r.x >> 8) as u8); bus.write8(ea.wrapping_add(1), (self.r.x & 0xFF) as u8);
                self.set_nz16(self.r.x);
                self.r.cc &= !V;
                consumed = 5 + addcyc;
                if trace { println!("{:04X}: AF ..    STX {} -> {:04X}", pc0, desc, ea); }
            }
            _ => {
                // For now, treat unimplemented opcode as NOP-like to avoid hang
                if trace { println!("{:04X}: {:02X}      UNIMPL", pc0, op); }
                consumed = 1; // placeholder
            }
        }
        self.cycles += consumed as u64;
        // Poll bus interrupt lines
        let (nmi_line, firq_line, irq_line) = bus.irq_lines();
        if nmi_line { self.nmi_pending = true; }
        if firq_line { self.firq_pending = true; }
        if irq_line { self.irq_pending = true; }
        // After instruction, handle interrupts
        let mut ic = 0u32;
        if self.nmi_pending {
            ic += self.service_nmi(bus);
        } else if self.firq_pending && (self.r.cc & F)==0 {
            ic += self.service_firq(bus);
        } else if self.irq_pending && (self.r.cc & I)==0 {
            ic += self.service_irq(bus);
        }
        self.cycles += ic as u64;
        consumed + ic
    }
}

pub fn regs_snapshot(cpu: &Cpu) -> Registers { cpu.r }
