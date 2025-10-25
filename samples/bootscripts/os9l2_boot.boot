# Phase 1: Initialization and kernel mapping (OS-9 Level II)

# Ensure we start in system mode and protect vectors
mode system
# Example: enable write-protect on vector page, and set protection control
attr vectors wprot
prot_ctrl 0x0B

# Map kernel pages (example mapping; adjust for your image)
# Map logical pages 0x0C..0x0E to high frames 0xD0..0xD2
map 0x0C 0xD0
map 0x0D 0xD1
map 0x0E 0xD2

# Optionally clear/disable NX mapping
nx null

# Defer user switch and I/O init to later phases
