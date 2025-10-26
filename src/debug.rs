use crate::bus::{Bus, Memory};
use crate::cpu::Cpu;

pub fn dump_registers(cpu: &Cpu) {
    let r = &cpu.r;
    println!(
        "A:{:02X} B:{:02X} X:{:04X} Y:{:04X} U:{:04X} S:{:04X} PC:{:04X} DP:{:02X} CC:{:02X} (E F H I N Z V C) = {}{}{}{}{}{}{}{}",
        r.a, r.b, r.x, r.y, r.u, r.s, r.pc, r.dp, r.cc,
        if (r.cc & 0x80)!=0 {'E'} else {'-'},
        if (r.cc & 0x40)!=0 {'F'} else {'-'},
        if (r.cc & 0x20)!=0 {'H'} else {'-'},
        if (r.cc & 0x10)!=0 {'I'} else {'-'},
        if (r.cc & 0x08)!=0 {'N'} else {'-'},
        if (r.cc & 0x04)!=0 {'Z'} else {'-'},
        if (r.cc & 0x02)!=0 {'V'} else {'-'},
        if (r.cc & 0x01)!=0 {'C'} else {'-'}
    );
}

pub fn dump_memory(mem: &Memory, start: u16, len: usize) {
    let bytes = mem.read_slice(start, len);
    let mut addr = start as usize;
    for chunk in bytes.chunks(16) {
        print!("{:04X}:", addr as u16);
        for b in chunk {
            print!(" {:02X}", b);
        }
        println!("");
        addr += chunk.len();
    }
}

pub fn dump_memory_bus<B: Bus + ?Sized>(bus: &mut B, start: u16, len: usize) {
    let mut addr = start;
    let mut left = len;
    while left > 0 {
        print!("{:04X}:", addr);
        let line = left.min(16);
        for i in 0..line {
            let b = bus.read8(addr.wrapping_add(i as u16));
            print!(" {:02X}", b);
        }
        println!("");
        addr = addr.wrapping_add(line as u16);
        left -= line;
    }
}

pub fn dump_memory_ascii<B: Bus + ?Sized>(bus: &mut B, start: u16, len: usize) {
    let mut addr = start;
    let mut left = len;
    while left > 0 {
        let line = left.min(16);
        let mut bytes: [u8; 16] = [0; 16];
        for i in 0..line {
            bytes[i] = bus.read8(addr.wrapping_add(i as u16));
        }
        print!("{:04X}:", addr);
        for i in 0..line {
            print!(" {:02X}", bytes[i]);
        }
        for _ in line..16 {
            print!("   ");
        }
        print!("  |");
        for i in 0..line {
            let c = bytes[i];
            let ch = if (0x20..=0x7E).contains(&c) {
                c as char
            } else {
                '.'
            };
            print!("{}", ch);
        }
        println!("|");
        addr = addr.wrapping_add(line as u16);
        left -= line;
    }
}
