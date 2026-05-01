        ORG   $E000
CONBASE EQU   $FF00       ; MC6850 ACIA: status @ +0, data @ +1
TDRE    EQU   $02         ; SR bit1: Transmit Data Register Empty

START   LDY   #CONBASE
        LDX   #MSG
NEXT    LDB   ,X+         ; 文字取得
        BEQ   HALT
WAIT    LDA   ,Y          ; STATUS = base+0
        BITA  #TDRE
        BEQ   WAIT
        STB   1,Y         ; TX 送信 (base+1)
        BRA   NEXT

HALT    BRA   HALT

MSG     FCC   "HELLO, WORLD"
        FCB   $0D,$0A,0

; ISR: runs on each timer tick. After 60 ticks, 
; No action at this time.
IRQ_ENT RTI

        ORG   $FFF4
        FDB   IRQ_ENT     ; FIRQ vector
        ORG   $FFF6
        FDB   IRQ_ENT     ; IRQ vector
        ORG   $FFFE
        FDB   START
