        ORG   $E000
CONBASE EQU   $FF00       ; console base (TX/RX)
TDRE    EQU   $01         ; TX ready bit
ESC     EQU   $1B

START   LDS   #$DFFF
        CLRB
        LDX   #CLRSEQ
        BSR   PUTSTR
        LDX   #TITLE
        BSR   PUTSTR
        LDX   #COLOR1
        BSR   PUTSTR
        LDX   #COLOR2
        BSR   PUTSTR
        LDX   #COLOR3
        BSR   PUTSTR
        LDX   #STYLE1
        BSR   PUTSTR
        LDX   #MOVE1
        BSR   PUTSTR

LINELOOP
        TFR   B,A
        BSR   SET_COLOR
        LDX   #LINEPFX
        BSR   PUTSTR
        TFR   B,A
        BSR   PRINT_HEX
        LDX   #CRLF
        BSR   PUTSTR
        INCB
        CMPB  #$20
        BLO   LINELOOP

HALT    BRA   HALT

; ---------------------------
; Console output helpers
; ---------------------------
PUTCH   PSHS  A
WAIT    LDA   CONBASE+1
        BITA  #TDRE
        BEQ   WAIT
        PULS  A
        STA   CONBASE
        RTS

PUTSTR  LDA   ,X+
        BEQ   PUTSTR_DONE
        BSR   PUTCH
        BRA   PUTSTR
PUTSTR_DONE
        RTS

PRINT_HEX
        PSHS  A
        LSRA
        LSRA
        LSRA
        LSRA
        BSR   HEX_DIGIT
        PULS  A
        ANDA  #$0F
        BSR   HEX_DIGIT
        RTS

HEX_DIGIT
        ADDA  #'0'
        CMPA  #'9'
        BLS   HEX_OUT
        ADDA  #7
HEX_OUT
        BSR   PUTCH
        RTS

SET_COLOR
        ANDA  #$06
        ADDA  #1
        ADDA  #'0'
        PSHS  A
        LDA   #ESC
        BSR   PUTCH
        LDA   #'['
        BSR   PUTCH
        LDA   #'3'
        BSR   PUTCH
        PULS  A
        BSR   PUTCH
        LDA   #'m'
        BSR   PUTCH
        RTS

; ---------------------------
; Data
; ---------------------------
CLRSEQ  FCB   ESC
        FCC   "[2J"
        FCB   ESC
        FCC   "[H"
        FCB   0

TITLE   FCC   "VT100 DEMO"
        FCB   $0D,$0A,0

COLOR1  FCB   ESC
        FCC   "[31mRed text"
        FCB   ESC
        FCC   "[0m"
        FCB   $0D,$0A,0

COLOR2  FCB   ESC
        FCC   "[32mGreen text"
        FCB   ESC
        FCC   "[0m"
        FCB   $0D,$0A,0

COLOR3  FCB   ESC
        FCC   "[34mBlue text"
        FCB   ESC
        FCC   "[0m"
        FCB   $0D,$0A,0

STYLE1  FCB   ESC
        FCC   "[4mUnderline"
        FCB   ESC
        FCC   "[0m"
        FCB   "  "
        FCB   ESC
        FCC   "[7mInverse"
        FCB   ESC
        FCC   "[0m"
        FCB   $0D,$0A,0

MOVE1   FCB   ESC
        FCC   "[5;10HCursor moved to row 5, col 10"
        FCB   ESC
        FCC   "[0m"
        FCB   $0D,$0A,0

LINEPFX FCC   "Line "
        FCB   0

CRLF    FCB   $0D,$0A,0

; IRQ vector
IRQ_ENT RTI

        ORG   $FFF4
        FDB   IRQ_ENT
        ORG   $FFF6
        FDB   IRQ_ENT
        ORG   $FFFE
        FDB   START
