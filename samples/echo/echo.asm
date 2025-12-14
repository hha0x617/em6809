CONBASE  EQU   $FF00      ; base address for console
CONDATA  EQU   CONBASE    ; TX write / RX read
CONSTAT  EQU   CONBASE+1  ; status
TXRDY    EQU   $01        ; status bit0
RXRDY    EQU   $02        ; status bit1

        ORG   $E000
START   LDY   #CONBASE
        LDX   #PROMPT
; print prompt
PLOOP   LDB   ,X+
        BEQ   MAIN
        BSR   TXB
        BRA   PLOOP
; main echo loop
MAIN    ; wait for a byte (poll RXRDY)
RWAIT   LDA   CONSTAT
        BITA  #RXRDY
        BEQ   RWAIT

        ; read one byte
        LDB   CONDATA

        ; if CR, emit CR+LF
        CMPB  #$0D
        BEQ   CRLF

        ; else echo the byte
        BSR   TXB
        BRA   MAIN

CRLF    LDB   #$0D
        BSR   TXB
        LDB   #$0A
        BSR   TXB
        BRA   MAIN

; transmit B (poll TXRDY then write)
TXB     PSHS  A
TWAIT   LDA   CONSTAT
        BITA  #TXRDY
        BEQ   TWAIT
        STB   CONDATA
        PULS  A,PC

PROMPT  FCC   "Type something (CR=newline): "
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
