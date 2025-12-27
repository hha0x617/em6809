; MMU map/protection demonstration for em6809
; - Splits the logical page $0C differently between system/user maps
; - Shows write-protect fault reporting on the vectors page
;
; Assemble (requires lwasm):
;   lwasm -9 --format=srec -o samples/mmu/mmu_map_demo.s19 samples/mmu/mmu_map_demo.asm
; Run:
;   cargo run -- samples/mmu/mmu_map_demo.s19 \
;     --mmu --console 0xFF00 --console-echo --mmu-log-maps --run

CONBASE     EQU   $FF00
CONDATA     EQU   CONBASE
CONSTAT     EQU   CONBASE+1
TXRDY       EQU   $01

MMU_BASE    EQU   $FFA0
MMU_MAP     EQU   MMU_BASE          ; +page (0..0F) -> frame#
MMU_TASK    EQU   MMU_BASE+$10
MMU_MODE    EQU   MMU_BASE+$11      ; 0=user, 1=system
MMU_PROT    EQU   MMU_BASE+$12      ; bit0=WPROT_EN, bit3=NX_EN, bit1=IRQ on fault
MMU_FAULT   EQU   MMU_BASE+$13      ; latched fault bits, clears on read
MMU_ATTR    EQU   MMU_BASE+$18      ; +page -> attr (bit0=WPROT, bit2=NX)

PAGE_BANK   EQU   $0C               ; logical page to flip
FRAME_SYS   EQU   $0C               ; system map frame for $0C
FRAME_USER  EQU   $0D               ; user map frame for $0C
LOG_BANK    EQU   $C100             ; logical address used to read banked text
PAGE_VECT   EQU   $0F
PROT_FLAGS  EQU   $0B               ; WPROT_EN + IRQ on fault + NX_EN

        ORG   $E000

START   LDS   #$E1FF
        JSR   INIT_MMU
        LDX   #MSG_HELLO
        JSR   PRINT_STR

        ; User map view (page $0C -> frame $0D)
        CLR   MMU_MODE
        JSR   SHOW_VIEW

        ; System map view (page $0C -> frame $0C)
        LDA   #$01
        STA   MMU_MODE
        JSR   SHOW_VIEW

        ; Write-protect demo on vectors page
        CLR   MMU_MODE
        JSR   TEST_WPROT

HALT    BRA   HALT

; Configure MMU maps and protection.
INIT_MMU
        PSHS  A
        ; Program system map entry for page $0C -> frame $0C
        LDA   #$01
        STA   MMU_MODE
        LDA   #FRAME_SYS
        STA   MMU_MAP+PAGE_BANK
        ; Program user map entry for page $0C -> frame $0D
        CLR   MMU_MODE
        LDA   #FRAME_USER
        STA   MMU_MAP+PAGE_BANK
        ; Enable NX/WPROT controls and mark pages
        LDA   #PROT_FLAGS
        STA   MMU_PROT
        LDA   #$04                 ; NX on page 0 to catch null jumps
        STA   MMU_ATTR+0
        LDA   #$01                 ; WPROT vectors page
        STA   MMU_ATTR+PAGE_VECT
        PULS  A,PC

; Print current map entry and banked text.
SHOW_VIEW
        PSHS  A,B,X
        LDA   MMU_MODE
        BEQ   SV_USER
        LDX   #MSG_SYS
        BRA   SV_LABEL
SV_USER LDX   #MSG_USER
SV_LABEL JSR  PRINT_STR
        LDX   #MSG_MAP
        JSR   PRINT_STR
        LDA   MMU_MAP+PAGE_BANK
        JSR   PRINT_HEX
        JSR   PRINT_NL
        LDX   #MSG_TEXT
        JSR   PRINT_STR
        LDX   #LOG_BANK
        JSR   PRINT_STR
        JSR   PRINT_NL
        PULS  A,B,X,PC

; Attempt a write to the protected vectors page and show fault_status.
TEST_WPROT
        PSHS  A,B,X
        LDX   #MSG_WPROT
        JSR   PRINT_STR
        LDA   #$01
        STA   MMU_ATTR+PAGE_VECT
        LDA   #$55
        STA   $FFF0               ; falls in vectors page ($0F), WPROT triggers
        LDA   MMU_FAULT           ; read & clear fault bits
        LDX   #MSG_FAULT
        JSR   PRINT_STR
        JSR   PRINT_HEX
        JSR   PRINT_NL
        PULS  A,B,X,PC

; Print null-terminated string pointed by X.
PRINT_STR
        PSHS  A,B,X
PS_LOOP LDA   ,X+
        BEQ   PS_DONE
        BSR   TXA
        BRA   PS_LOOP
PS_DONE PULS  A,B,X,PC

; Transmit A via console (polling TXRDY).
TXA     PSHS  B
TX_WAIT LDB   CONSTAT
        BITB  #TXRDY
        BEQ   TX_WAIT
        STA   CONDATA
        PULS  B,PC

; Print "$xx" from A.
PRINT_HEX
        PSHS  A,B
        TFR   A,B
        LDA   #'$'
        BSR   TXA
        TFR   B,A
        LSRA
        LSRA
        LSRA
        LSRA
        BSR   HEX_DIGIT
        TFR   B,A
        ANDA  #$0F
        BSR   HEX_DIGIT
        PULS  A,B,PC

HEX_DIGIT
        ADDA  #'0'
        CMPA  #'9'
        BLS   TXA
        ADDA  #7
        BRA   TXA

PRINT_NL
        PSHS  A
        LDA   #$0D
        BSR   TXA
        LDA   #$0A
        BSR   TXA
        PULS  A,PC

; Dummy IRQ/FIRQ handlers.
IRQ_ENT RTI

        ORG   $C100
BANK_SYS
        FCC   "[system map view]"
        FCB   0

        ORG   $D100
BANK_USER
        FCC   "[user map view]"
        FCB   0

        ORG   $E200
MSG_HELLO
        FCC   "MMU map/protect demo"
        FCB   $0D,$0A,0
MSG_USER
        FCC   "User mode (page $0C -> frame $0D)"
        FCB   $0D,$0A,0
MSG_SYS
        FCC   "System mode (page $0C -> frame $0C)"
        FCB   $0D,$0A,0
MSG_MAP
        FCC   "Map entry $0C -> "
        FCB   0
MSG_TEXT
        FCC   "Text @ $C100: "
        FCB   0
MSG_WPROT
        FCC   "Write to protected vectors page... "
        FCB   0
MSG_FAULT
        FCC   "fault_status="
        FCB   0

        ORG   $FFF4
        FDB   IRQ_ENT      ; FIRQ vector
        ORG   $FFF6
        FDB   IRQ_ENT      ; IRQ vector
        ORG   $FFFE
        FDB   START
