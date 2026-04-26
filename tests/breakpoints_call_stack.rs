// Integration tests for the Phase A2 / A1 work: execution breakpoints
// (`em6809::debug::Breakpoint*`) and the shadow call stack
// (`em6809::debug::ShadowCallStack` + `Cpu::shadow_stack`).
//
// These tests stay at the `Cpu + Memory` level — no GUI, no plugin —
// so they exercise the call/return hooks and breakpoint matcher in
// isolation from the rest of the system.

use em6809::bus::Memory;
use em6809::cpu::Cpu;
use em6809::debug::CallKind;

/// Boot a fresh CPU at PC=0x1000 with S pointing to the top of a
/// generous stack region, ready to execute whatever the test loaded
/// into `mem`. Tests rely on the shadow stack and breakpoint set
/// starting empty, which `Cpu::new()` guarantees.
fn boot(mem: &Memory) -> Cpu {
    let _ = mem; // silence unused-binding warnings; tests load before booting
    let mut cpu = Cpu::new();
    cpu.r.pc = 0x1000;
    cpu.r.s = 0x2000;
    cpu
}

// ============================================================
// ShadowCallStack
// ============================================================

#[test]
fn bsr_then_rts_pushes_and_pops_one_frame() {
    // $1000  8D 02     BSR  +2    -> $1004
    // $1002  39        RTS         (end of caller — never reached in this test)
    // $1004  39        RTS         (callee body: just return)
    let mut mem = Memory::new();
    mem.load_slice(0x1000, &[0x8D, 0x02, 0x39, 0x00, 0x39]);
    let mut cpu = boot(&mem);

    cpu.step(&mut mem, false); // BSR
    assert_eq!(cpu.shadow_stack.len(), 1, "BSR should push one frame");
    let f = cpu.shadow_stack.peek().unwrap();
    assert_eq!(f.kind, CallKind::Bsr);
    assert_eq!(f.return_addr, 0x1002);
    assert_eq!(f.call_site, 0x1000);
    assert_eq!(f.target, 0x1004);
    assert_eq!(cpu.r.pc, 0x1004);

    cpu.step(&mut mem, false); // RTS
    assert!(cpu.shadow_stack.is_empty(), "RTS should pop the frame");
    assert_eq!(cpu.r.pc, 0x1002);
}

#[test]
fn jsr_extended_then_rts_round_trip() {
    // $1000  BD 12 34   JSR  $1234
    // $1234  39         RTS
    let mut mem = Memory::new();
    mem.load_slice(0x1000, &[0xBD, 0x12, 0x34]);
    mem.load_slice(0x1234, &[0x39]);
    let mut cpu = boot(&mem);

    cpu.step(&mut mem, false);
    assert_eq!(cpu.shadow_stack.len(), 1);
    assert_eq!(cpu.shadow_stack.peek().unwrap().kind, CallKind::Jsr);
    assert_eq!(cpu.shadow_stack.peek().unwrap().target, 0x1234);
    assert_eq!(cpu.r.pc, 0x1234);

    cpu.step(&mut mem, false); // RTS
    assert!(cpu.shadow_stack.is_empty());
    assert_eq!(cpu.r.pc, 0x1003);
}

#[test]
fn lbsr_uses_bsr_kind() {
    // $1000  17 00 04   LBSR +4    -> $1007
    // $1007  39         RTS
    let mut mem = Memory::new();
    mem.load_slice(0x1000, &[0x17, 0x00, 0x04]);
    mem.load_slice(0x1007, &[0x39]);
    let mut cpu = boot(&mem);

    cpu.step(&mut mem, false);
    assert_eq!(cpu.shadow_stack.peek().unwrap().kind, CallKind::Bsr);
    assert_eq!(cpu.r.pc, 0x1007);
}

#[test]
fn nested_calls_keep_frames_in_push_order() {
    // outer:                          inner:
    //   $1000  BD 20 00  JSR $2000     $2000  BD 30 00  JSR $3000
    //   $1003  39        RTS           $2003  39        RTS
    //
    // grandchild:
    //   $3000  39        RTS
    let mut mem = Memory::new();
    mem.load_slice(0x1000, &[0xBD, 0x20, 0x00, 0x39]);
    mem.load_slice(0x2000, &[0xBD, 0x30, 0x00, 0x39]);
    mem.load_slice(0x3000, &[0x39]);
    let mut cpu = boot(&mem);

    cpu.step(&mut mem, false); // JSR $2000
    cpu.step(&mut mem, false); // JSR $3000
    assert_eq!(cpu.shadow_stack.len(), 2);
    let frames = cpu.shadow_stack.frames();
    assert_eq!(frames[0].target, 0x2000);
    assert_eq!(frames[1].target, 0x3000);

    cpu.step(&mut mem, false); // RTS (from grandchild)
    assert_eq!(cpu.shadow_stack.len(), 1);
    assert_eq!(cpu.shadow_stack.peek().unwrap().target, 0x2000);

    cpu.step(&mut mem, false); // RTS (from inner)
    assert!(cpu.shadow_stack.is_empty());
    assert_eq!(cpu.r.pc, 0x1003);
}

#[test]
fn swi_pushes_with_swi_kind_and_rti_pops() {
    // $1000  3F        SWI    -> jumps to $4000 (vector at $FFFA/$FFFB)
    // $4000  3B        RTI    (immediately return)
    //
    // We assert that SWI pushes a Swi(1) frame and RTI drops it.
    // We deliberately do NOT assert the post-RTI PC value here:
    // em6809's SWI/RTI byte-order in the saved frame is a separate
    // concern (and a known quirk in this codebase) that's outside
    // the scope of the shadow-stack hooks.
    let mut mem = Memory::new();
    mem.load_slice(0x1000, &[0x3F]);
    mem.load_slice(0x4000, &[0x3B]);
    mem.load_slice(0xFFFA, &[0x40, 0x00]);
    let mut cpu = boot(&mem);

    cpu.step(&mut mem, false); // SWI
    assert_eq!(cpu.shadow_stack.len(), 1);
    let f = cpu.shadow_stack.peek().unwrap();
    assert_eq!(f.kind, CallKind::Swi(1));
    assert_eq!(f.target, 0x4000);
    assert_eq!(cpu.r.pc, 0x4000);

    cpu.step(&mut mem, false); // RTI
    assert!(cpu.shadow_stack.is_empty());
}

#[test]
fn nmi_during_bsr_keeps_both_frames_then_rti_drops_only_the_nmi() {
    // em6809's `step` is "instruction then interrupt-service" — a
    // pending NMI is acted on *after* the next instruction completes.
    // Use a NOP (0x12) as the callee body so we can observe a
    // BSR+NMI nested state without the callee already having
    // returned via RTS.
    //
    //   $1000  8D 02   BSR  $1004
    //   $1004  12      NOP    (callee body)
    //   $1005  39      RTS    (callee return, never reached in this test)
    //   $5000  3B      RTI    (NMI handler)
    //
    // NMI vector at $FFFC/$FFFD -> $5000.
    let mut mem = Memory::new();
    mem.load_slice(0x1000, &[0x8D, 0x02, 0x39, 0x00, 0x12, 0x39]);
    mem.load_slice(0x5000, &[0x3B]);
    mem.load_slice(0xFFFC, &[0x50, 0x00]);
    let mut cpu = boot(&mem);

    cpu.step(&mut mem, false); // BSR
    assert_eq!(cpu.shadow_stack.len(), 1);

    // Fire NMI: the next step runs the NOP, then services NMI.
    cpu.request_nmi();
    cpu.step(&mut mem, false);
    assert_eq!(cpu.shadow_stack.len(), 2);
    let frames = cpu.shadow_stack.frames();
    assert_eq!(frames[0].kind, CallKind::Bsr);
    assert_eq!(frames[1].kind, CallKind::Nmi);
    assert_eq!(cpu.r.pc, 0x5000);

    cpu.step(&mut mem, false); // RTI from NMI handler
                               // Only the NMI frame should be gone; the BSR frame remains.
    assert_eq!(cpu.shadow_stack.len(), 1);
    assert_eq!(cpu.shadow_stack.peek().unwrap().kind, CallKind::Bsr);
}

// ============================================================
// BreakpointSet
// ============================================================

#[test]
fn breakpoint_disabled_does_not_trigger() {
    let mut cpu = Cpu::new();
    let id = cpu.breakpoints.add(0x1234);
    cpu.breakpoints.set_enabled(id, false);
    assert!(cpu.breakpoints.should_break(0x1234).is_none());
}

#[test]
fn breakpoint_enabled_triggers_and_counts_hits() {
    let mut cpu = Cpu::new();
    let id = cpu.breakpoints.add(0x1234);
    assert_eq!(cpu.breakpoints.should_break(0x1234), Some(id));
    assert_eq!(cpu.breakpoints.should_break(0x1234), Some(id));
    let bp = cpu.breakpoints.get(id).unwrap();
    assert_eq!(bp.hit_count, 2);
}

#[test]
fn breakpoint_ignore_count_skips_first_n_hits() {
    let mut cpu = Cpu::new();
    let id = cpu.breakpoints.add(0x1234);
    cpu.breakpoints.set_ignore_count(id, 3);
    // First 3 hits silent, 4th hit triggers.
    assert!(cpu.breakpoints.should_break(0x1234).is_none());
    assert!(cpu.breakpoints.should_break(0x1234).is_none());
    assert!(cpu.breakpoints.should_break(0x1234).is_none());
    assert_eq!(cpu.breakpoints.should_break(0x1234), Some(id));
    let bp = cpu.breakpoints.get(id).unwrap();
    assert_eq!(bp.hit_count, 1, "hit_count only counts actual stops");
}

#[test]
fn breakpoint_remove_clears_match() {
    let mut cpu = Cpu::new();
    let id = cpu.breakpoints.add(0x1234);
    assert!(cpu.breakpoints.remove(id));
    assert!(cpu.breakpoints.should_break(0x1234).is_none());
    assert!(!cpu.breakpoints.remove(id), "second remove returns false");
}

#[test]
fn breakpoint_set_condition_stores_string() {
    let mut cpu = Cpu::new();
    let id = cpu.breakpoints.add(0x1234);
    assert!(cpu
        .breakpoints
        .set_condition(id, Some("a == 5".to_string())));
    let bp = cpu.breakpoints.get(id).unwrap();
    assert_eq!(bp.condition.as_deref(), Some("a == 5"));
    // The pre-condition path (`should_break`) still triggers
    // unconditionally — it doesn't see the condition. The
    // condition-aware path is exercised in the dedicated
    // condition_* tests below.
    assert_eq!(cpu.breakpoints.should_break(0x1234), Some(id));
}

// ============================================================
// Condition evaluator (BreakpointSet::check / Cpu::check_breakpoint)
// ============================================================

#[test]
fn condition_no_condition_always_triggers() {
    let mut cpu = Cpu::new();
    cpu.r.pc = 0x1234;
    let id = cpu.breakpoints.add(0x1234);
    assert_eq!(cpu.check_breakpoint(0x1234), Some(id));
}

#[test]
fn condition_empty_string_treated_as_no_condition() {
    let mut cpu = Cpu::new();
    cpu.r.pc = 0x1234;
    let id = cpu.breakpoints.add(0x1234);
    assert!(cpu.breakpoints.set_condition(id, Some("   ".to_string())));
    assert_eq!(cpu.check_breakpoint(0x1234), Some(id));
}

#[test]
fn condition_register_equality_true() {
    let mut cpu = Cpu::new();
    cpu.r.a = 0x42;
    let id = cpu.breakpoints.add(0x1234);
    cpu.breakpoints
        .set_condition(id, Some("a == 0x42".to_string()));
    assert_eq!(cpu.check_breakpoint(0x1234), Some(id));
    let bp = cpu.breakpoints.get(id).unwrap();
    assert_eq!(bp.hit_count, 1, "a true condition counts as a hit");
}

#[test]
fn condition_register_equality_false_skips_silently() {
    let mut cpu = Cpu::new();
    cpu.r.a = 0x10;
    let id = cpu.breakpoints.add(0x1234);
    cpu.breakpoints
        .set_condition(id, Some("a == 0x42".to_string()));
    assert_eq!(cpu.check_breakpoint(0x1234), None);
    let bp = cpu.breakpoints.get(id).unwrap();
    assert_eq!(
        bp.hit_count, 0,
        "a false condition does not advance hit_count"
    );
}

#[test]
fn condition_dollar_hex_literal() {
    let mut cpu = Cpu::new();
    cpu.r.pc = 0x1234;
    cpu.r.x = 0x0FFE;
    let id = cpu.breakpoints.add(0x1234);
    cpu.breakpoints
        .set_condition(id, Some("x < $1000".to_string()));
    assert_eq!(cpu.check_breakpoint(0x1234), Some(id));
}

#[test]
fn condition_d_register_is_a_concat_b() {
    // D should read as ((a << 8) | b)
    let mut cpu = Cpu::new();
    cpu.r.a = 0x12;
    cpu.r.b = 0x34;
    let id = cpu.breakpoints.add(0x1234);
    cpu.breakpoints
        .set_condition(id, Some("d == 0x1234".to_string()));
    assert_eq!(cpu.check_breakpoint(0x1234), Some(id));
}

#[test]
fn condition_logical_and_short_circuit() {
    let mut cpu = Cpu::new();
    cpu.r.x = 0x100;
    cpu.r.y = 0x300;
    let id = cpu.breakpoints.add(0x1234);
    cpu.breakpoints
        .set_condition(id, Some("x > 0 && y < 0x400".to_string()));
    assert_eq!(cpu.check_breakpoint(0x1234), Some(id));

    cpu.breakpoints
        .set_condition(id, Some("x > 0 && y > 0x400".to_string()));
    assert_eq!(cpu.check_breakpoint(0x1234), None);
}

#[test]
fn condition_logical_or_and_negation() {
    let mut cpu = Cpu::new();
    cpu.r.a = 0;
    cpu.r.b = 1;
    let id = cpu.breakpoints.add(0x1234);
    cpu.breakpoints
        .set_condition(id, Some("a == 0 || b == 0".to_string()));
    assert_eq!(cpu.check_breakpoint(0x1234), Some(id));

    cpu.breakpoints
        .set_condition(id, Some("!(a == 0)".to_string()));
    assert_eq!(cpu.check_breakpoint(0x1234), None);
}

#[test]
fn condition_arithmetic() {
    let mut cpu = Cpu::new();
    cpu.r.a = 0x10;
    cpu.r.b = 0x20;
    let id = cpu.breakpoints.add(0x1234);
    cpu.breakpoints
        .set_condition(id, Some("a + b == 0x30".to_string()));
    assert_eq!(cpu.check_breakpoint(0x1234), Some(id));
}

#[test]
fn condition_parens_override_precedence() {
    let mut cpu = Cpu::new();
    cpu.r.a = 0;
    cpu.r.b = 1;
    let id = cpu.breakpoints.add(0x1234);
    // Without parens: a == 0 || b == 0 && false → first disjunct
    // wins. With parens around the OR, the AND clamps it.
    cpu.breakpoints
        .set_condition(id, Some("(a == 0 || b == 0) && a + b > 100".to_string()));
    assert_eq!(cpu.check_breakpoint(0x1234), None);
}

#[test]
fn condition_parse_error_falls_through_to_trigger() {
    // Fail-safe: a malformed condition makes the BP trigger anyway,
    // so the user notices it instead of silently skipping forever.
    let mut cpu = Cpu::new();
    let id = cpu.breakpoints.add(0x1234);
    cpu.breakpoints
        .set_condition(id, Some("a == == 5".to_string()));
    assert_eq!(cpu.check_breakpoint(0x1234), Some(id));
}

#[test]
fn condition_ignore_count_only_consumed_when_condition_true() {
    let mut cpu = Cpu::new();
    cpu.r.a = 0x10;
    let id = cpu.breakpoints.add(0x1234);
    cpu.breakpoints
        .set_condition(id, Some("a == 0x42".to_string()));
    cpu.breakpoints.set_ignore_count(id, 3);

    // Condition is false; ignore_count must NOT be consumed.
    for _ in 0..5 {
        assert_eq!(cpu.check_breakpoint(0x1234), None);
    }
    let bp = cpu.breakpoints.get(id).unwrap();
    assert_eq!(
        bp.ignore_count, 3,
        "false-condition skips do not burn ignore ticks"
    );

    // Now flip A so the condition is true; first 3 hits go to the
    // ignore counter, the 4th finally triggers.
    cpu.r.a = 0x42;
    assert_eq!(cpu.check_breakpoint(0x1234), None);
    assert_eq!(cpu.check_breakpoint(0x1234), None);
    assert_eq!(cpu.check_breakpoint(0x1234), None);
    assert_eq!(cpu.check_breakpoint(0x1234), Some(id));
}

#[test]
fn breakpoint_clear_drops_everything() {
    let mut cpu = Cpu::new();
    cpu.breakpoints.add(0x1000);
    cpu.breakpoints.add(0x2000);
    assert_eq!(cpu.breakpoints.len(), 2);
    cpu.breakpoints.clear();
    assert!(cpu.breakpoints.is_empty());
}

#[test]
fn breakpoint_ids_are_unique_and_not_reused() {
    let mut cpu = Cpu::new();
    let a = cpu.breakpoints.add(0x1000);
    let b = cpu.breakpoints.add(0x1000);
    assert_ne!(a, b, "two BPs at the same address must have distinct IDs");
    cpu.breakpoints.remove(a);
    let c = cpu.breakpoints.add(0x1000);
    assert_ne!(
        a, c,
        "removing then re-adding must not recycle the freed ID"
    );
}

#[test]
fn reset_clears_shadow_stack_but_not_breakpoints() {
    // Reset vector at $FFFE/$FFFF -> $1000
    let mut mem = Memory::new();
    mem.load_slice(0xFFFE, &[0x10, 0x00]);
    mem.load_slice(0x1000, &[0x39]); // RTS (just to give cpu somewhere to be)
    let mut cpu = Cpu::new();
    cpu.breakpoints.add(0x1234);
    cpu.shadow_stack.push(em6809::debug::CallFrame {
        return_addr: 0x9999,
        call_site: 0x9998,
        target: 0x8000,
        sp_at_call: 0x2000,
        kind: CallKind::Jsr,
    });
    cpu.reset(&mut mem);
    assert!(
        cpu.shadow_stack.is_empty(),
        "reset must drop frames from the previous program"
    );
    assert_eq!(
        cpu.breakpoints.len(),
        1,
        "reset must keep user-set breakpoints"
    );
}
