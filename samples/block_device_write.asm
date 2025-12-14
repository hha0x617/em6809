; Block device write demonstration for em6809
; Writes a length-prefixed payload to LBA 1, then reads it back and prints
; the contents on the console.

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
CMD_READ  EQU   $01
CMD_RESET EQU   $FF
CMD_WRITE EQU   $02

WRITE_LBA EQU   $0001

        ORG   $E000

START   LDX   #MSG_HELLO
        JSR   PRINT_STR

        JSR   LOAD_BUFFER
        JSR   ISSUE_WRITE

        LDX   #MSG_WRITE_OK
        JSR   PRINT_STR

        JSR   ISSUE_READ
        JSR   STREAM_BLOCK

        LDX   #MSG_DONE
        JSR   PRINT_STR

HALT    BRA   HALT

; Load payload into the block device's sector buffer (length byte + data + zero padding)
LOAD_BUFFER
        PSHS  A,B,X,Y
        LDA   #CMD_RESET
        STA   BLKCMD
        LDB   #PAYLOAD_LEN
        STB   BLKDATA
        LDX   #PAYLOAD_TEXT
        LDB   #PAYLOAD_LEN
WRMSG   LDA   ,X+
        STA   BLKDATA
        DECB
        BNE   WRMSG
        LDY   #PAD_COUNT
        BEQ   ZPAD_DONE
ZPAD    CLR   BLKDATA
        LEAY  -1,Y
        BNE   ZPAD
ZPAD_DONE
        PULS  A,B,X,Y,PC

; Program a single-sector WRITE to WRITE_LBA and wait for BUSY to clear
ISSUE_WRITE
        PSHS  A
        LDA   #$01
        STA   BLKSECC
        CLRA
        STA   BLKLBA3
        STA   BLKLBA2
        LDA   #WRITE_LBA/256
        STA   BLKLBA1
        LDA   #WRITE_LBA&$FF
        STA   BLKLBA0
        LDA   #CMD_WRITE
        STA   BLKCMD
WRWAIT  LDA   BLKSTAT
        BITA  #STAT_BUSY
        BNE   WRWAIT
        PULS  A,PC

; Program and wait for a single-sector READ of WRITE_LBA
ISSUE_READ
        PSHS  A
        LDA   #$01
        STA   BLKSECC
        CLRA
        STA   BLKLBA3
        STA   BLKLBA2
        LDA   #WRITE_LBA/256
        STA   BLKLBA1
        LDA   #WRITE_LBA&$FF
        STA   BLKLBA0
        LDA   #CMD_READ
        STA   BLKCMD
RDWAIT  LDA   BLKSTAT
        BITA  #STAT_DRDY
        BEQ   RDWAIT
        PULS  A,PC

; Read length byte then stream that many bytes from DATA port
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

; Print null-terminated string pointed to by X
PRINT_STR
        PSHS  B,X
PLOOP   LDB   ,X+
        BEQ   PDONE
        BSR   TXB
        BRA   PLOOP
PDONE   PULS  B,X,PC

; Transmit byte in B via polling TXRDY
TXB     PSHS  A
TWAIT   LDA   CONSTAT
        BITA  #TXRDY
        BEQ   TWAIT
        STB   CONDATA
        PULS  A,PC

PAYLOAD_TEXT
        FCC   "Block write demo payload!"
PAYLOAD_END
PAYLOAD_LEN EQU   PAYLOAD_END-PAYLOAD_TEXT
PAD_COUNT  EQU   511-PAYLOAD_LEN

MSG_HELLO
        FCB   $0D,$0A
        FCC   "Writing payload to block device (LBA 1)..."
        FCB   $0D,$0A,0

MSG_WRITE_OK
        FCB   $0D,$0A
        FCC   "Write complete, reading back:"
        FCB   $0D,$0A,0

MSG_DONE
        FCB   $0D,$0A
        FCC   "[demo complete]"
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
