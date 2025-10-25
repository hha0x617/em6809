# OS-9 Level II: Full boot sequence (init -> kernel map -> user -> I/O)

# Phase 1: init + kernel mapping
mode system
attr vectors wprot
prot_ctrl 0x0B
map 0x0C 0xD0
map 0x0D 0xD1
map 0x0E 0xD2
nx null

# Phase 2: switch to user mode at step N
on_step 512 mode user

# Phase 3: I/O initialization a few steps later
on_step 520 con_ctrl 0x11
on_step 520 con_rx_wm 4
on_step 520 con_irq_hold 8

# Optional: adjust IRQ mask near I/O bring-up
on_step 520 irq_mask off
