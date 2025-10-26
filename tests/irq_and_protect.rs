use mc6809emu::bus::{Bus, Memory};
use mc6809emu::io::{ConsoleDev, Device, IoBus};
use mc6809emu::mmu::Mc6829;

#[test]
fn mmu_write_protect_raises_irq() {
    let mut mmu = Mc6829::new(1 << 20, 0xFFA0);
    mmu.identity_map_current();
    // Enable write-protect on page 0 and IRQ on fault
    // attr page0 at regs+0x18
    mmu.write8(0xFFA0 + 0x18, 0x01);
    mmu.write8(0xFFA0 + 0x12, 0x03); // prot_ctrl: WPROT_EN + IRQ on fault
                                     // Attempt to write logical 0x0001
    mmu.write8(0x0001, 0xAA);
    let (_n, _f, irq) = mmu.irq_lines();
    assert!(irq, "IRQ should be raised on write-protect fault");
}

#[test]
fn console_rx_irq_line() {
    let mem = Memory::new();
    let mut bus = IoBus::new(mem);
    let mut con = ConsoleDev::new(0xFF00);
    con.feed_bytes(b"A");
    // Enable RX IRQ via CTRL
    Device::write8(&mut con, 0xFF02, 0x01);
    bus.add_device(con);
    let (_n, _f, irq) = bus.irq_lines();
    assert!(irq, "Console RX should assert IRQ when enabled");
}

#[test]
fn mmu_noexec_fetch_fault_sets_irq() {
    let mut mmu = Mc6829::new(1 << 20, 0xFFA0);
    mmu.identity_map_current();
    // Page 0 NX, enable NX_EN and IRQ on fault
    mmu.write8(0xFFA0 + 0x18, 0x04); // attr: NX
    mmu.write8(0xFFA0 + 0x12, 0x0A); // prot_ctrl: NX_EN + IRQ on fault
                                     // Instruction fetch from 0x0000 should trip fault
    let _ = mmu.read8_fetch(0x0000);
    let (_n, _f, irq) = mmu.irq_lines();
    assert!(irq, "NX fetch should assert IRQ when enabled");
}

#[test]
fn console_firq_enable() {
    let mem = Memory::new();
    let mut bus = IoBus::new(mem);
    let mut con = ConsoleDev::new(0xFF00);
    con.feed_bytes(b"Z");
    // Enable RX IRQ and FIRQ mode (bit4)
    Device::write8(&mut con, 0xFF02, 0x11);
    bus.add_device(con);
    let (_n, f, irq) = bus.irq_lines();
    assert!(f && !irq, "FIRQ should be asserted, not IRQ");
}
