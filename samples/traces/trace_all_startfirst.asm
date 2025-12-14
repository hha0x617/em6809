        ; Variant with START at top (subroutines after), functionally same as trace_all.asm

        ORG   $E000

; CC bits
EQU_DI  EQU   $50         ; disable FIRQ($40) and IRQ($10) masks
EQU_Z   EQU   $04
EQU_N   EQU   $08
EQU_V   EQU   $02
EQU_C   EQU   $01

SETCC   MACRO
        PSHS  A
        PULS  CC
        ENDM

CONBASE EQU   $FF00
CONDATA EQU   CONBASE
CONSTAT EQU   CONBASE+1
TXRDY   EQU   $01
CONON   EQU   $00FF

START   LDS   #$E800
        LDU   #$E900
        LDX   #$0100
        LDY   #$0200
        ; probe console
        LDX   #$0010
PPRB    LDA   CONSTAT
        BITA  #TXRDY
        BEQ   PPRB2
        LDA   #$01
        STA   <$FF
        BRA   PPRB3
PPRB2   LEAX  -1,X
        BNE   PPRB
PPRB3   NOP

; Tests identical to trace_all.asm (abbreviated for brevity):
        LDX   #S_BEQ
        LBSR  PRNT
        LDA   #EQU_Z
        ORA   #EQU_DI
        SETCC
        BEQ   BEQ_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BEQ_NEXT
BEQ_T   LDX   #S_T
        LBSR  PRNT
BEQ_NEXT NOP

        ; ... replicate the rest from trace_all.asm as needed ...

DONE    BRA   DONE

; --- Subroutines after START ---
TXB     PSHS  A,X
        LDA   CONON
        BEQ   TXRET
        LDX   #$00FF
TXW     LDA   CONSTAT
        BITA  #TXRDY
        BNE   TXGO
        LEAX  -1,X
        BNE   TXW
TXGO    STB   CONDATA
TXRET   PULS  A,X,PC

PRNT    PSHS  A,B,X
        LDA   CONON
        BEQ   PDONE
PLOOP   LDB   ,X+
        BEQ   PDONE
        LBSR  TXB
        BRA   PLOOP
PDONE   PULS  A,B,X,PC

; Strings
S_T     FCC   " T\r\n"
        FCB   0
S_N     FCC   " N\r\n"
        FCB   0
S_BEQ   FCC   "BEQ:"
        FCB   0

; ISR: runs on each timer tick. After 60 ticks, 
; No action at this time.
IRQ_ENT RTI

        ORG   $FFF4
        FDB   IRQ_ENT     ; FIRQ vector
        ORG   $FFF6
        FDB   IRQ_ENT     ; IRQ vector
        ORG   $FFFE
        FDB   START

