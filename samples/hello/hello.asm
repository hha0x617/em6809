        ORG   $E000
CONBASE EQU   $FF00       ; 送受信データ
TDRE    EQU   $01         ; TX ready bit (bit0)

START   LDY   #CONBASE
        LDX   #MSG
NEXT    LDB   ,X+         ; 文字取得
        BEQ   HALT
WAIT    LDA   1,Y         ; STATUS = base+1
        BITA  #TDRE
        BEQ   WAIT
        STB   ,Y          ; TX 送信 (base)
        BRA   NEXT

HALT    BRA   HALT

MSG     FCC   "HELLO, WORLD"
        FCB   $0D,$0A,0

        ORG   $FFFE
        FDB   START
