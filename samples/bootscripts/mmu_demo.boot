# MMU map change demo
# - Enters system mode at reset
# - Sets basic protection
# - Flips page->frame mappings over time so you can see the MMU grid update

# 1) Initialization at RESET
on_pc 0xE000 mode sys
on_pc 0xE000 prot 0x0B         # WPROT_EN | NX_EN | IRQ on fault
on_pc 0xE000 attr 0x00=0x04    # NX null page
on_pc 0xE000 attr 0x0F=0x01    # WPROT vectors page

# 2) Flip I/O window and code pages over time
on_step 200  map 0x0E=0xFE     # Map logical page 0x0E to frame 0xFE
on_step 400  map 0x0E=0xD2     # Move page 0x0E to frame 0xD2
on_step 600  map 0x0C=0xD0
on_step 800  map 0x0D=0xD1

# 3) Optionally drop to user mode later
on_step 1200 mode user

# Tips:
# - Run with: --preset os9l2-io --boot-script samples/bootscripts/mmu_demo.boot --mmu-log-maps
# - The GUI opens paused; resume/step to watch the grid highlight changes.
