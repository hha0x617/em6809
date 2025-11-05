#- Force a visible pending:
#  - Use a very small reload (e.g., --timer-reload 0x0002) so the timer overflows
# immediately; pause the GUI and the pill should flip to “PENDING” (red).
#  - Or add to your boot script:

on_pc 0xE000 timer_reload 0x0002
on_pc 0xE000 timer_irq on
on_pc 0xE000 timer_start
