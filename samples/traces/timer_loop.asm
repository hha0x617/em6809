; Minimal loop program to exercise the Timer at $FF10
; Assembles for MC6809; start at $E000 and set reset vector.

                ORG     $E000

start           ANDCC   #$AF        ; enable IRQ+FIRQ (clear I and F)
                CLR     $E100       ; zero a counter we'll bump in IRQ
                BRA     start       ; tight loop for timer ticks

; Simple IRQ/FIRQ handler: bump $E100 then return
irq_handler     INC     $E100
                RTI

                ORG     $FFFE
                FDB     start       ; RESET vector -> start
                ORG     $FFF4
                FDB     irq_handler ; FIRQ vector
                ORG     $FFF6
                FDB     irq_handler ; IRQ vector

; Run with:
;   cargo run -- --base 0xE000 --pc 0xE000 --timer 0xFF10 --timer-reload 0x0010 --timer-irq --timer-start -- samples/traces/timer_loop.bin
; or assemble to S19 and simply run:
;   cargo run -- samples/traces/timer_loop.s19 --timer 0xFF10 --timer-reload 0x0010 --timer-irq --timer-start
