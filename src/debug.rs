#![allow(clippy::uninlined_format_args)]
use std::collections::BTreeSet;

use crate::bus::{Bus, Memory};
use crate::cpu::Cpu;
use crate::disasm::disasm_one;

// =============================================================================
// Instruction boundaries
// =============================================================================
//
// MC6809 instructions are 1..5 bytes long. Decoding `bus[pc]` as if it were
// always an opcode goes well only when `pc` happens to be the start of a real
// instruction; offset by one byte and the entire downstream stream is wrong.
// That's why disassembly views that anchor on `pc - some_constant` look
// stable until the user reaches a region where the chosen offset isn't a
// boundary, and then everything desyncs — including the current-PC line and
// any breakpoint markers, since those are matched by address against the
// (now-misaligned) line list.
//
// `InstructionBoundaries` is a confirmed-boundary set: every address it
// holds is *known* to be an instruction start, either because the CPU has
// actually executed that PC or because a linear sweep of a known code
// region disassembled an instruction starting there. Disassembly views
// then anchor on `boundaries.floor(pc)` — the most recent confirmed
// boundary at or before `pc` — guaranteeing the rendered listing aligns
// with the real instruction stream around `pc`.

/// A set of addresses that are *known* to be instruction starts.
///
/// Used by disassembly views to pick a safe anchor near the current PC
/// instead of guessing with `pc - constant` arithmetic. Populate via
/// [`InstructionBoundaries::insert`] (per executed instruction) and
/// [`linear_sweep`] (after loading code).
#[derive(Debug, Default, Clone)]
pub struct InstructionBoundaries {
    set: BTreeSet<u16>,
}

impl InstructionBoundaries {
    pub fn new() -> Self {
        Self { set: BTreeSet::new() }
    }

    /// Forget every recorded boundary. Call this when the program is
    /// reloaded, the bus is reset, or memory contents change in a way
    /// that invalidates previously-confirmed instruction starts.
    pub fn clear(&mut self) {
        self.set.clear();
    }

    /// Record `addr` as an instruction start.
    pub fn insert(&mut self, addr: u16) {
        self.set.insert(addr);
    }

    pub fn extend<I: IntoIterator<Item = u16>>(&mut self, iter: I) {
        self.set.extend(iter);
    }

    /// Largest known boundary `<= addr`. `None` if no boundary at or
    /// below `addr` has been seen yet.
    pub fn floor(&self, addr: u16) -> Option<u16> {
        self.set.range(..=addr).next_back().copied()
    }

    /// Smallest known boundary `>= addr`. `None` if no boundary at or
    /// above `addr` has been seen yet.
    pub fn ceil(&self, addr: u16) -> Option<u16> {
        self.set.range(addr..).next().copied()
    }

    /// Iterate boundaries in ascending order over the inclusive range
    /// `[from, to]`. Implements `DoubleEndedIterator` so callers can
    /// walk the range from either side via `.rev()`.
    pub fn iter_range(
        &self,
        from: u16,
        to: u16,
    ) -> impl DoubleEndedIterator<Item = u16> + '_ {
        self.set.range(from..=to).copied()
    }

    pub fn contains(&self, addr: u16) -> bool {
        self.set.contains(&addr)
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }
}

/// Disassemble linearly from `start` up to (but not including) `end`,
/// recording each instruction start in `boundaries`.
///
/// This is the cheap pass to run right after a program load — it walks
/// the byte stream as if every encountered byte starts an instruction,
/// which is correct for pure code regions. In code/data mixed regions
/// the sweep may stamp boundaries onto bytes that are actually data;
/// the resulting "phantom" boundaries do no harm as long as the
/// real PC ends up inside a true code region, because
/// [`InstructionBoundaries::floor`] will return whichever true boundary
/// is closest.
///
/// Stops when `pc >= end` or when it would wrap past 0xFFFF.
pub fn linear_sweep<B: Bus + ?Sized>(
    bus: &mut B,
    start: u16,
    end: u16,
    boundaries: &mut InstructionBoundaries,
) {
    if end <= start {
        return;
    }
    let mut pc = start;
    while pc < end {
        boundaries.insert(pc);
        let (consumed, _) = disasm_one(bus, pc);
        // disasm_one always consumes at least one byte for valid 6809;
        // guard against pathological zero-consume to avoid infinite loop.
        if consumed == 0 {
            break;
        }
        let (next, wrapped) = pc.overflowing_add(consumed);
        if wrapped || next <= pc {
            break;
        }
        pc = next;
    }
}

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
        let addr16 = addr as u16;
        print!("{addr16:04X}:");
        for b in chunk {
            print!(" {b:02X}");
        }
        println!();
        addr += chunk.len();
    }
}

#[allow(clippy::needless_range_loop)]
pub fn dump_memory_bus<B: Bus + ?Sized>(bus: &mut B, start: u16, len: usize) {
    let mut addr = start;
    let mut left = len;
    while left > 0 {
        print!("{addr:04X}:");
        let line = left.min(16);
        for i in 0..line {
            let b = bus.read8(addr.wrapping_add(i as u16));
            print!(" {b:02X}");
        }
        println!();
        addr = addr.wrapping_add(line as u16);
        left -= line;
    }
}

#[allow(clippy::needless_range_loop)]
pub fn dump_memory_ascii<B: Bus + ?Sized>(bus: &mut B, start: u16, len: usize) {
    let mut addr = start;
    let mut left = len;
    while left > 0 {
        let line = left.min(16);
        let mut bytes: [u8; 16] = [0; 16];
        for i in 0..line {
            bytes[i] = bus.read8(addr.wrapping_add(i as u16));
        }
        print!("{addr:04X}:");
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
            print!("{ch}");
        }
        println!("|");
        addr = addr.wrapping_add(line as u16);
        left -= line;
    }
}
