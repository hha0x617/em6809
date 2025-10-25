use mc6809emu::bus::{Bus, Memory};
use mc6809emu::io::{IoBus, ConsoleDev};

#[test]
fn console_rx_tx_status() {
    let mem = Memory::new();
    let mut bus = IoBus::new(mem);
    let mut con = ConsoleDev::new(0xFF00);
    con.feed_bytes(b"Hi");
    bus.add_device(con);

    // STATUS should show RXRDY=1 (bit1)
    let st = bus.read8(0xFF01);
    assert_eq!(st & 0x01, 0x01); // TXRDY
    assert_ne!(st & 0x02, 0x00); // RXRDY

    // Read two bytes
    let h = bus.read8(0xFF00);
    let i = bus.read8(0xFF00);
    assert_eq!(h, b'H');
    assert_eq!(i, b'i');
    // RX should now be empty
    let st2 = bus.read8(0xFF01);
    assert_eq!(st2 & 0x02, 0x00);

    // TX: write a byte (prints). Ensure it doesn't panic
    bus.write8(0xFF00, b'!');
}

#[test]
fn console_rx_irq_watermark() {
    let mem = Memory::new();
    let mut bus = IoBus::new(mem);
    let mut con = ConsoleDev::new(0xFF00);
    con.set_rx_watermark(2);
    con.feed_bytes(b"A");
    bus.add_device(con);
    let (_n,_f,irq1) = bus.irq_lines();
    assert!(!irq1, "Below watermark should not assert IRQ");
    // feed second byte
    // Find device and feed again via write directly on IoBus
    // Simpler: add another device
    // Instead re-create with both bytes for test clarity
    let mem2 = Memory::new();
    let mut bus2 = IoBus::new(mem2);
    let mut con2 = ConsoleDev::new(0xFF00);
    con2.set_rx_watermark(2);
    con2.feed_bytes(b"AB");
    // Enable RX IRQ
    mc6809emu::io::Device::write8(&mut con2, 0xFF02, 0x01);
    bus2.add_device(con2);
    let (_n2,_f2,irq2) = bus2.irq_lines();
    assert!(irq2, "At/above watermark should assert IRQ");
}
