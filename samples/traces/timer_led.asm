; Timer-driven LED toggle sample
; - Uses GPIO at $FF30 (bit0 as LED)
; - Timer device at $FF10 generates periodic interrupts
; - ISR counts 60 ticks, then toggles LED (approx 1 Hz if timer-rate=60)
;
; Assemble for MC6809, place at $E000, set vectors at top of memory.

                ORG     $E000

; I/O base addresses
GPIO_BASE       EQU     $FF30      ; OUT=$FF30, IN=$FF31, DIR=$FF32
TIMER_BASE      EQU     $FF10      ; CTRL/STATUS=$FF10

; Variables
LED_STATE       EQU     $E100      ; current LED state (mirrors OUT bit0)
TICK_COUNT      EQU     $E101      ; tick accumulator (0..59)

start           CLR     LED_STATE
                CLR     TICK_COUNT
                ; Configure GPIO bit0 as output, initialize OFF
                LDA     #$00
                STA     GPIO_BASE      ; OUT=0 (LED OFF)
                LDA     #$01
                STA     GPIO_BASE+2    ; DIR=1 for bit0 (output)
                ANDCC   #$AF           ; enable IRQ and FIRQ (clear I and F masks)

main_loop       BRA     main_loop       ; idle - timer ISR does the work

; ISR: runs on each timer tick. After 60 ticks, toggle LED.
irq_handler     PSHS    A               ; save A (simple ISR)
                LDA     TICK_COUNT
                INCA
                CMPA    #60
                BLO     .store_ret      ; <60: store and return
                CLR     TICK_COUNT      ; reached 60 -> reset tick counter
                ; Toggle LED bit0
                LDA     GPIO_BASE       ; read OUT
                EORA    #$01            ; flip bit0
                STA     GPIO_BASE       ; write OUT
                STA     LED_STATE       ; mirror in RAM for inspection
                BRA     .done
.store_ret      STA     TICK_COUNT
.done           PULS    A
                RTI

                ORG     $FFF4
                FDB     irq_handler     ; FIRQ vector
                ORG     $FFF6
                FDB     irq_handler     ; IRQ vector
                ORG     $FFFE
                FDB     start           ; RESET vector -> start

; Suggested run commands:
;  60 Hz via FIRQ (recommended: fits 16-bit reload, ~1Hz blink):
;    cargo run -- samples/traces/timer_led.s19 \
;      --gpio 0xFF30 --gpio-bits 8 \
;      --timer 0xFF10 --timer-rate 60 --timer-ips 1000000 --timer-irq --timer-firq --timer-start --run
;
;  Using IRQ instead of FIRQ (still fine since both vectors map to same handler):
;    cargo run -- samples/traces/timer_led.s19 \
;      --gpio 0xFF30 --gpio-bits 8 \
;      --timer 0xFF10 --timer-rate 60 --timer-ips 1000000 --timer-irq --timer-start --run
;
; Notes:
; - The timer "rate" is based on instruction ticks using the estimate from --timer-ips. With --timer-ips=1,000,000
;   and --timer-rate=60, reload is ~16666 (fits in 16 bits). Counting 60 ticks produces ~1 second.
; - In the viewer, watch $E100 (LED_STATE) and GPIO OUT at $FF30 to see the toggling.
