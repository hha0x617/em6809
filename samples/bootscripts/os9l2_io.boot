# Phase 3: I/O initialization (console)

# Configure console control: bit0=RX IRQ, bit1=TX IRQ, bit4=FIRQ
con_ctrl 0x11    # enable RX IRQ + FIRQ

# Set receive IRQ watermark and optional hold (debounce/coalescing)
con_rx_wm 4
con_irq_hold 8

# Unmask IRQ if previously masked
irq_mask off
