use crate::bus::Bus;

fn r8<B: Bus + ?Sized>(bus: &mut B, addr: u16) -> u8 { bus.read8(addr) }
fn r16<B: Bus + ?Sized>(bus: &mut B, addr: u16) -> u16 { ((r8(bus, addr) as u16) << 8) | (r8(bus, addr.wrapping_add(1)) as u16) }

fn fmt_hex8(v: u8) -> String { format!("${:02X}", v) }
fn fmt_hex16(v: u16) -> String { format!("${:04X}", v) }

fn reg_name(code: u8) -> &'static str {
    match code & 0x03 { 0 => "X", 1 => "Y", 2 => "U", _ => "S" }
}

fn reg16_name(code: u8) -> Option<&'static str> {
    match code & 0x0F { 0x0=>Some("D"), 0x1=>Some("X"), 0x2=>Some("Y"), 0x3=>Some("U"), 0x4=>Some("S"), 0x5=>Some("PC"), _=>None }
}
fn reg8_name(code: u8) -> Option<&'static str> {
    match code & 0x0F { 0x8=>Some("A"), 0x9=>Some("B"), 0xA=>Some("CC"), 0xB=>Some("DP"), _=>None }
}

fn decode_indexed<B: Bus + ?Sized>(bus: &mut B, pc: u16) -> (u16, String) {
    let pb = r8(bus, pc);
    let rsel = (pb >> 5) & 0x03;
    if (pb & 0x80) == 0 {
        // 5-bit signed offset ,R
        let off5 = ((pb & 0x1F) as i8) << 3 >> 3;
        return (1, format!("{}{},{}",
            if off5 >= 0 { "" } else { "-" },
            off5.abs(),
            reg_name(rsel)
        ));
    }
    let form = pb & 0x1F;
    let indirect = (pb & 0x10) != 0;
    let is_pc_rel = rsel == 3 && matches!(form, 0x04 | 0x08 | 0x09 | 0x0B);
    let base = if is_pc_rel { "PC" } else { reg_name(rsel) };
    match form {
        0x04 => { // ,R or [ ,R ]
            let t = if indirect { format!("[{}]", base) } else { format!(",{}", base) };
            (1, t)
        }
        0x05 => { // B,R
            let t = if indirect { format!("[B,{}]", base) } else { format!("B,{}", base) };
            (1, t)
        }
        0x06 => { // A,R
            let t = if indirect { format!("[A,{}]", base) } else { format!("A,{}", base) };
            (1, t)
        }
        0x08 => { // n8,R
            let off = r8(bus, pc.wrapping_add(1)) as i8 as i16;
            let t = if indirect { format!("[{}{}{}]", base, if off>=0{"+"} else {""}, off) }
                    else { format!("{}{}{}", base, if off>=0{"+"} else {""}, off) };
            (2, t)
        }
        0x09 => { // n16,R
            let off = r16(bus, pc.wrapping_add(1)) as i16;
            let t = if indirect { format!("[{}{}{}]", base, if off>=0{"+"} else {""}, off) }
                    else { format!("{}{}{}", base, if off>=0{"+"} else {""}, off) };
            (3, t)
        }
        0x00 => (1, format!(",{}+", base)),
        0x01 => (1, format!(",{}++", base)),
        0x02 => (1, format!(",-{}", base)),
        0x03 => (1, format!(",--{}", base)),
        0x0B => { // D,R
            let t = if indirect { format!("[D,{}]", base) } else { format!("D,{}", base) };
            (1, t)
        }
        0x0F | 0x1F => { // [n16]
            let addr = r16(bus, pc.wrapping_add(1));
            (3, format!("[{}]", fmt_hex16(addr)))
        }
        _ => (1, format!(",{}(?)", base)),
    }
}

pub fn disasm_one<B: Bus + ?Sized>(bus: &mut B, pc: u16) -> (u16, String) {
    let op = r8(bus, pc);
    match op {
        0x12 => (1, "NOP".to_string()),
        0x39 => (1, "RTS".to_string()),
        0x3B => (1, "RTI".to_string()),
        0x48 => (1, "ASLA".to_string()),
        0x44 => (1, "LSRA".to_string()),
        0x49 => (1, "ROLA".to_string()),
        0x46 => (1, "RORA".to_string()),
        0x20 => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BRA {}", off)) }
        0x21 => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BRN {}", off)) }
        0x22 => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BHI {}", off)) }
        0x23 => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BLS {}", off)) }
        0x24 => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BCC {}", off)) }
        0x25 => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BCS {}", off)) }
        0x26 => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BNE {}", off)) }
        0x27 => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BEQ {}", off)) }
        0x28 => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BVC {}", off)) }
        0x29 => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BVS {}", off)) }
        0x2A => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BPL {}", off)) }
        0x2B => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BMI {}", off)) }
        0x2C => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BGE {}", off)) }
        0x2D => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BLT {}", off)) }
        0x2E => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BGT {}", off)) }
        0x2F => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BLE {}", off)) }
        0x8D => { let off = r8(bus, pc.wrapping_add(1)) as i8; (2, format!("BSR {}", off)) }
        0x16 => { let off = r16(bus, pc.wrapping_add(1)) as i16; (3, format!("LBRA {}", off)) }
        0x17 => { let off = r16(bus, pc.wrapping_add(1)) as i16; (3, format!("LBSR {}", off)) }
        0x7E => { let addr = r16(bus, pc.wrapping_add(1)); (3, format!("JMP {}", fmt_hex16(addr))) }
        0x9D => { let zp=r8(bus, pc.wrapping_add(1)); (2, format!("JSR <{}", fmt_hex8(zp))) }
        0xAD => { let (add, idx)=decode_indexed(bus, pc.wrapping_add(1)); (1+add, format!("JSR {}", idx)) }
        0xBD => { let addr = r16(bus, pc.wrapping_add(1)); (3, format!("JSR {}", fmt_hex16(addr))) }
        0x86 => { let imm = r8(bus, pc.wrapping_add(1)); (2, format!("LDA #{}", fmt_hex8(imm))) }
        0x96 => { let zp = r8(bus, pc.wrapping_add(1)); (2, format!("LDA <{}", fmt_hex8(zp))) }
        0xB6 => { let addr = r16(bus, pc.wrapping_add(1)); (3, format!("LDA {}", fmt_hex16(addr))) }
        0xA6 => {
            let (add, idx) = decode_indexed(bus, pc.wrapping_add(1));
            (1 + add, format!("LDA {}", idx))
        }
        0x97 => { let zp = r8(bus, pc.wrapping_add(1)); (2, format!("STA <{}", fmt_hex8(zp))) }
        0xB7 => { let addr = r16(bus, pc.wrapping_add(1)); (3, format!("STA {}", fmt_hex16(addr))) }
        0xA7 => {
            let (add, idx) = decode_indexed(bus, pc.wrapping_add(1));
            (1 + add, format!("STA {}", idx))
        }
        0x8E => { let imm = r16(bus, pc.wrapping_add(1)); (3, format!("LDX #{}", fmt_hex16(imm))) }
        0xCE => { let imm = r16(bus, pc.wrapping_add(1)); (3, format!("LDU #{}", fmt_hex16(imm))) }
        0xC3 => { let imm = r16(bus, pc.wrapping_add(1)); (3, format!("ADDD #{}", fmt_hex16(imm))) }
        0x83 => { let imm = r16(bus, pc.wrapping_add(1)); (3, format!("SUBD #{}", fmt_hex16(imm))) }
        0x34 => { let m = r8(bus, pc.wrapping_add(1)); (2, format!("PSHS #{}", fmt_hex8(m))) }
        0x35 => { let m = r8(bus, pc.wrapping_add(1)); (2, format!("PULS #{}", fmt_hex8(m))) }
        0x30 => { let (add, idx) = decode_indexed(bus, pc.wrapping_add(1)); (1+add, format!("LEAX {}", idx)) }
        0x31 => { let (add, idx) = decode_indexed(bus, pc.wrapping_add(1)); (1+add, format!("LEAY {}", idx)) }
        0x32 => { let (add, idx) = decode_indexed(bus, pc.wrapping_add(1)); (1+add, format!("LEAU {}", idx)) }
        0x33 => { let (add, idx) = decode_indexed(bus, pc.wrapping_add(1)); (1+add, format!("LEAS {}", idx)) }
        0x10 => {
            let op2 = r8(bus, pc.wrapping_add(1));
            match op2 {
                0x3F => (2, "SWI2".to_string()),
                0x8E => { let imm = r16(bus, pc.wrapping_add(2)); (4, format!("LDY #{}", fmt_hex16(imm))) }
                0x8C => { let imm = r16(bus, pc.wrapping_add(2)); (4, format!("CMPY #{}", fmt_hex16(imm))) }
                0x9E => { let zp=r8(bus, pc.wrapping_add(2)); (3, format!("LDY <{}", fmt_hex8(zp))) }
                0x9F => { let zp=r8(bus, pc.wrapping_add(2)); (3, format!("STY <{}", fmt_hex8(zp))) }
                0xAE => { let (add, idx) = decode_indexed(bus, pc.wrapping_add(2)); (2+add, format!("LDY {}", idx)) }
                0xAF => { let (add, idx) = decode_indexed(bus, pc.wrapping_add(2)); (2+add, format!("STY {}", idx)) }
                0xBE => { let addr=r16(bus, pc.wrapping_add(2)); (4, format!("LDY {}", fmt_hex16(addr))) }
                0xBF => { let addr=r16(bus, pc.wrapping_add(2)); (4, format!("STY {}", fmt_hex16(addr))) }
                // Long conditional branches
                0x21 => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBRN {}", off)) }
                0x22 => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBHI {}", off)) }
                0x23 => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBLS {}", off)) }
                0x24 => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBCC {}", off)) }
                0x25 => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBCS {}", off)) }
                0x26 => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBNE {}", off)) }
                0x27 => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBEQ {}", off)) }
                0x28 => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBVC {}", off)) }
                0x29 => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBVS {}", off)) }
                0x2A => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBPL {}", off)) }
                0x2B => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBMI {}", off)) }
                0x2C => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBGE {}", off)) }
                0x2D => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBLT {}", off)) }
                0x2E => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBGT {}", off)) }
                0x2F => { let off=r16(bus, pc.wrapping_add(2)) as i16; (4, format!("LBLE {}", off)) }
                _ => (2, format!("FCB ${:02X}, ${:02X}", op, op2)),
            }
        }
        0x1E => { // EXG
            let pb = r8(bus, pc.wrapping_add(1));
            let src = pb >> 4; let dst = pb & 0x0F;
            let text = if (src & 0x08)==0 && (dst & 0x08)==0 {
                if let (Some(s), Some(d)) = (reg16_name(src), reg16_name(dst)) { format!("EXG {},{}", s, d) } else { format!("EXG ${:02X}", pb) }
            } else if (src & 0x08)!=0 && (dst & 0x08)!=0 {
                if let (Some(s), Some(d)) = (reg8_name(src), reg8_name(dst)) { format!("EXG {},{}", s, d) } else { format!("EXG ${:02X}", pb) }
            } else { format!("EXG ${:02X}", pb) };
            (2, text)
        }
        0x1F => { // TFR
            let pb = r8(bus, pc.wrapping_add(1));
            let src = pb >> 4; let dst = pb & 0x0F;
            let text = if (src & 0x08)==0 && (dst & 0x08)==0 {
                if let Some(s) = reg16_name(src) { if let Some(d)=reg16_name(dst) { format!("TFR {},{}", s, d) } else { format!("TFR ${:02X}", pb) } } else { format!("TFR ${:02X}", pb) }
            } else if (src & 0x08)!=0 && (dst & 0x08)!=0 {
                if let Some(s) = reg8_name(src) { if let Some(d)=reg8_name(dst) { format!("TFR {},{}", s, d) } else { format!("TFR ${:02X}", pb) } } else { format!("TFR ${:02X}", pb) }
            } else { format!("TFR ${:02X}", pb) };
            (2, text)
        }
        0x11 => { let op2 = r8(bus, pc.wrapping_add(1)); if op2==0x3F { (2, "SWI3".to_string()) } else { (2, format!("FCB ${:02X}, ${:02X}", op, op2)) } }
        _ => (1, format!("FCB ${:02X}", op)),
    }
}

pub fn disasm_one_hex<B: Bus + ?Sized>(bus: &mut B, pc: u16) -> (u16, String) {
    let (len, text) = disasm_one(bus, pc);
    // Gather bytes
    let mut bytes = String::new();
    for i in 0..len {
        let b = r8(bus, pc.wrapping_add(i));
        if !bytes.is_empty() { bytes.push(' '); }
        bytes.push_str(&format!("{:02X}", b));
    }
    (len, format!("{: <9} {}", bytes, text))
}
