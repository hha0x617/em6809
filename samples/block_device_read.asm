; Block device read demonstration for em6809
; Reads LBA 0 from the block storage controller at $FF20
; and streams the payload (length-prefixed text) to the console.

CONBASE   EQU   $FF00
CONDATA   EQU   CONBASE
CONSTAT   EQU   CONBASE+1
TXRDY     EQU   $01

BLKBASE   EQU   $FF20
BLKDATA   EQU   BLKBASE
BLKSTAT   EQU   BLKBASE+1
BLKCMD    EQU   BLKBASE+2
BLKSECC   EQU   BLKBASE+3
BLKLBA0   EQU   BLKBASE+4
BLKLBA1   EQU   BLKBASE+5
BLKLBA2   EQU   BLKBASE+6
BLKLBA3   EQU   BLKBASE+7

STAT_BUSY EQU   $01
STAT_DRDY EQU   $02
STAT_ERR  EQU   $04

CMD_READ  EQU   $01

        ORG   $E000

START   LDX   #MSG_HELLO
        JSR   PRINT_STR

        JSR   ISSUE_READ
        JSR   STREAM_BLOCK

        LDX   #MSG_DONE
        JSR   PRINT_STR

HALT    BRA   HALT

; Print null-terminated string pointed to by X.
PRINT_STR
        PSHS  B,X
PLOOP   LDB   ,X+
        BEQ   PDONE
        BSR   TXB
        BRA   PLOOP
PDONE   PULS  B,X,PC

; Transmit byte in B via polling TXRDY.
TXB     PSHS  A
TWAIT   LDA   CONSTAT
        BITA  #TXRDY
        BEQ   TWAIT
        STB   CONDATA
        PULS  A,PC

; Program and wait for a single-sector READ of LBA 0.
ISSUE_READ
        PSHS  A
        LDA   #$01
        STA   BLKSECC
        CLRA
        STA   BLKLBA0
        STA   BLKLBA1
        STA   BLKLBA2
        STA   BLKLBA3
        LDA   #CMD_READ
        STA   BLKCMD
RDWAIT  LDA   BLKSTAT
        BITA  #STAT_DRDY
        BEQ   RDWAIT
        PULS  A,PC

; Read length byte then stream that many bytes from DATA.
STREAM_BLOCK
        PSHS  X,B
        LDB   BLKDATA
        TSTB
        BEQ   RDDONE
        CLRA
        TFR   D,X
READLP  LDB   BLKDATA
        BSR   TXB
        LEAX  -1,X
        BNE   READLP
RDDONE  PULS  B,X,PC

MSG_HELLO
        FCB   $0D,$0A
        FCC   "Reading sector 0 via block device..."
        FCB   $0D,$0A,0

MSG_DONE
        FCB   $0D,$0A
        FCC   "[done]"
        FCB   $0D,$0A,0

        ORG   $FFFE
        FDB   START
