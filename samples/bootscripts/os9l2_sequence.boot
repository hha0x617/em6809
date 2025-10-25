# OS-9 Level II boot sequence example
# Stages: init -> kernel mapping -> user transfer -> I/O init

# Enter system mode at reset PC (adjust PC as needed)
on_pc 0xE000 mode sys

# Enable WPROT and NX protections, IRQ on fault
on_pc 0xE000 prot 0x0B

# NX null page; protect vectors
on_pc 0xE000 attr 0x00=0x04
on_pc 0xE000 attr 0x0F=0x01

# Map kernel pages 0x0C..0x0E to high frames (example frames 0xD0..0xD2)
on_pc 0xE000 map 0x0C=0xD0
on_pc 0xE000 map 0x0D=0xD1
on_pc 0xE000 map 0x0E=0xD2

# After some init, switch to user mode
on_step 512 mode user

# Initialize console: RX IRQ + FIRQ, watermark and hold
on_step 520 con_ctrl 0x11
on_step 520 con_rx_wm 4
on_step 520 con_irq_hold 8

# Optional: IRQ/FIRQ masking during early init
# on_pc 0xE000 irq_mask on
# on_step 300 irq_mask off
