        ; MC6809 trace ROM to exercise branches and page-2/3 ops
        ; Console is optional; use --trace to observe flow.
        ; If you pass --console 0xFF00, this program prints branch markers.

        ORG   $E000

; CC bits: E(80) F(40) H(20) I(10) N(08) Z(04) V(02) C(01)
EQU_DI  EQU   $50         ; disable FIRQ($40) and IRQ($10) masks
EQU_Z   EQU   $04
EQU_N   EQU   $08
EQU_V   EQU   $02
EQU_C   EQU   $01

; Macro: CC := A via stack
SETCC   MACRO
        PSHS  A
        PULS  CC
        ENDM

; Console (optional)
CONBASE EQU   $FF00
CONDATA EQU   CONBASE
CONSTAT EQU   CONBASE+1
TXRDY   EQU   $01         ; status bit0
CONON   EQU   $00FF       ; scratch flag in zero page (unused here)

; Simple TX subroutine: waits for TX ready then sends B
TXB     PSHS  A,X
        LDA   CONON       ; console present?
        BEQ   TXRET       ; skip if not
        LDX   #$00FF      ; timeout to avoid hang
TWAIT   LDA   CONSTAT
        BITA  #TXRDY
        BNE   TXGO
        LEAX  -1,X
        BNE   TWAIT
TXGO    STB   CONDATA
TXRET   PULS  A,X,PC

; Print zero-terminated string at X
PRNT    PSHS  A,B,X
        LDA   CONON       ; if no console, skip printing to keep behavior consistent
        BEQ   PDONE
PLOOP   LDB   ,X+
        BEQ   PDONE
        LBSR  TXB
        BRA   PLOOP
PDONE   PULS  A,B,X,PC

START   LDS   #$E800      ; init S
        LDU   #$E900      ; init U
        LDX   #$0100      ; scratch
        LDY   #$0200
        ; Probe console presence: if CONSTAT ever shows TXRDY, mark CONON=1
        LDX   #$0010
PLOOP2  LDA   CONSTAT
        BITA  #TXRDY
        BEQ   PSKIP
        LDA   #$01
        STA   CONON
        BRA   PEND
PSKIP   LEAX  -1,X
        BNE   PLOOP2
PEND    NOP

; --- Short branches (Bcc) ---
; BEQ taken: expect Z=1 -> taken
        LDX   #S_BEQ
        LBSR  PRNT
        LDA   #EQU_Z      ; set Z
        ORA   #EQU_DI
        SETCC              ; CC := Z
        BEQ   BEQ_T        ; taken
        LDX   #S_N
        LBSR  PRNT
        BRA   BEQ_NEXT
BEQ_T   LDX   #S_T
        LBSR  PRNT
BEQ_NEXT NOP
; BEQ not taken: expect Z=0 -> not taken
        LDX   #S_BEQ
        LBSR  PRNT
        LDA   #$00        ; clear Z
        ORA   #EQU_DI
        SETCC
        BEQ   BEQ_N       ; not taken
        LDX   #S_N
        LBSR  PRNT
        BRA   BEQ_NEND
BEQ_N   LDX   #S_T
        LBSR  PRNT
BEQ_NEND NOP

; BNE tests
; BNE with Z=0 -> taken
        LDX   #S_BNE
        LBSR  PRNT
        LDA   #$00        ; Z=0
        ORA   #EQU_DI
        SETCC
        BNE   BNE_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BNE_NEXT
BNE_T   LDX   #S_T
        LBSR  PRNT
BNE_NEXT NOP
; BNE with Z=1 -> not taken
        LDX   #S_BNE
        LBSR  PRNT
        LDA   #EQU_Z
        ORA   #EQU_DI
        SETCC
        BNE   BNE_N
        LDX   #S_N
        LBSR  PRNT
        BRA   BNE_NEND
BNE_N   LDX   #S_T
        LBSR  PRNT
BNE_NEND NOP

; BCC (C=0 -> taken) / BCS (C=1 -> taken)
        LDX   #S_BCC
        LBSR  PRNT
        LDA   #$00        ; C=0
        ORA   #EQU_DI
        SETCC
        BCC   BCC_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BCC_NEXT
BCC_T   LDX   #S_T
        LBSR  PRNT
BCC_NEXT NOP
        LDX   #S_BCS
        LBSR  PRNT
        LDA   #EQU_C      ; C=1
        ORA   #EQU_DI
        SETCC
        BCS   BCS_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BCS_NEXT
BCS_T   LDX   #S_T
        LBSR  PRNT
BCS_NEXT NOP

; BHI ( !(C||Z) ) and BLS (C||Z)
        LDX   #S_BHI
        LBSR  PRNT
        LDA   #$00        ; C=0,Z=0 -> taken
        ORA   #EQU_DI
        SETCC
        BHI   BHI_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BHI_NEXT
BHI_T   LDX   #S_T
        LBSR  PRNT
BHI_NEXT NOP
        LDX   #S_BLS
        LBSR  PRNT
        LDA   #EQU_C      ; C=1 -> taken
        ORA   #EQU_DI
        SETCC
        BLS   BLS_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BLS_NEXT
BLS_T   LDX   #S_T
        LBSR  PRNT
BLS_NEXT NOP

; BVC/BVS
        LDX   #S_BVC
        LBSR  PRNT
        LDA   #$00        ; V=0 -> BVC taken
        ORA   #EQU_DI
        SETCC
        BVC   BVC_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BVC_NEXT
BVC_T   LDX   #S_T
        LBSR  PRNT
BVC_NEXT NOP
        LDX   #S_BVS
        LBSR  PRNT
        LDA   #EQU_V      ; V=1 -> BVS taken
        ORA   #EQU_DI
        SETCC
        BVS   BVS_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BVS_NEXT
BVS_T   LDX   #S_T
        LBSR  PRNT
BVS_NEXT NOP

; BPL/BMI
        LDX   #S_BPL
        LBSR  PRNT
        LDA   #$00        ; N=0 -> BPL taken
        ORA   #EQU_DI
        SETCC
        BPL   BPL_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BPL_NEXT
BPL_T   LDX   #S_T
        LBSR  PRNT
BPL_NEXT NOP
        LDX   #S_BMI
        LBSR  PRNT
        LDA   #EQU_N      ; N=1 -> BMI taken
        ORA   #EQU_DI
        SETCC
        BMI   BMI_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BMI_NEXT
BMI_T   LDX   #S_T
        LBSR  PRNT
BMI_NEXT NOP

; BGE/BLT uses N^V
        LDX   #S_BGE
        LBSR  PRNT
        LDA   #$00        ; N=0,V=0 => N^V=0 => BGE taken
        ORA   #EQU_DI
        SETCC
        BGE   BGE_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BGE_NEXT
BGE_T   LDX   #S_T
        LBSR  PRNT
BGE_NEXT NOP
        LDX   #S_BLT
        LBSR  PRNT
        LDA   #EQU_N|EQU_V ; N=1,V=1 => N^V=0 => BLT not taken
        ORA   #EQU_DI
        SETCC
        BLT   BLT_N
        LDX   #S_N
        LBSR  PRNT
        BRA   BLT_NEXT
BLT_N   LDX   #S_T
        LBSR  PRNT
BLT_NEXT NOP

; BGT/ BLE
        LDX   #S_BGT
        LBSR  PRNT
        LDA   #$00        ; Z=0, N^V=0 => BGT taken
        ORA   #EQU_DI
        SETCC
        BGT   BGT_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BGT_NEXT
BGT_T   LDX   #S_T
        LBSR  PRNT
BGT_NEXT NOP
        LDX   #S_BLE
        LBSR  PRNT
        LDA   #EQU_Z      ; Z=1 => BLE taken
        ORA   #EQU_DI
        SETCC
        BLE   BLE_T
        LDX   #S_N
        LBSR  PRNT
        BRA   BLE_NEXT
BLE_T   LDX   #S_T
        LBSR  PRNT
BLE_NEXT NOP

; --- Long branches (LBcc) ---
        LDX   #S_LBEQ
        LBSR  PRNT
        LDA   #EQU_Z
        ORA   #EQU_DI
        SETCC
        LBEQ  LBEQ_T
        LDX   #S_N
        LBSR  PRNT
        BRA   LBEQ_NEXT
LBEQ_T  LDX   #S_T
        LBSR  PRNT
LBEQ_NEXT NOP

        LDX   #S_LBNE
        LBSR  PRNT
        LDA   #$00
        ORA   #EQU_DI
        SETCC
        LBNE  LBNE_T
        LDX   #S_N
        LBSR  PRNT
        BRA   LBNE_NEXT
LBNE_T  LDX   #S_T
        LBSR  PRNT
LBNE_NEXT NOP

; --- Page-2 Y/CPY/LDY/STY variants ---
        LDY   #$1234      ; LDY sets N/Z, clears V
        STY   <$20        ; STY sets N/Z, clears V (based on Y)
        LDY   <$20        ; reload
        LDX   #$4321
        ; CMPY immediate/direct/indexed/extended
        LDY   #$1111      ; A(1111)-B(2222): Z=0,N=1,V=0,C=0  (borrow)
        CMPY  #$2222      ; expect NZVC as above
        CMPY  <$20        ; 1111 - 1234: Z=0,N=1,V=0,C=0
        LEAX  $20,X       ; ensure indexed path exists (8-bit offset indexed)
        CMPY  ,X          ; 1111 - 5555: Z=0,N=1,V=0,C=0
        CMPY  $0020       ; 1111 - 0020: Z=0,N=1,V=0,C=0

; --- Page-2 S-register LDS/STS ---
        LDS   #$AAAA      ; LDS sets N/Z, clears V
        STS   <$30        ; STS sets N/Z, clears V (based on S)
        LDS   <$30        ; reload
        LEAY  $0030,Y
        STS   ,Y
        STS   $0030

; --- Page-3 CMPS ---
        LDS   #$BEEF
        CMPS  #$BEEF      ; equal: Z=1,N=0,V=0,C=1
        CMPS  <$30        ; compare against previously stored value
        CMPS  ,Y
        CMPS  $0030

DONE    BRA   DONE        ; spin; use --trace --step to inspect

; ISR: runs on each timer tick. After 60 ticks, 
; No action at this time.
IRQ_ENT RTI

; Strings for markers
S_T     FCC   " T\r\n"  
        FCB   0
S_N     FCC   " N\r\n"
        FCB   0
S_BEQ   FCC   "BEQ:"
        FCB   0
S_BNE   FCC   "BNE:"
        FCB   0
S_BCC   FCC   "BCC:"
        FCB   0
S_BCS   FCC   "BCS:"
        FCB   0
S_BHI   FCC   "BHI:"
        FCB   0
S_BLS   FCC   "BLS:"
        FCB   0
S_BVC   FCC   "BVC:"
        FCB   0
S_BVS   FCC   "BVS:"
        FCB   0
S_BPL   FCC   "BPL:"
        FCB   0
S_BMI   FCC   "BMI:"
        FCB   0
S_BGE   FCC   "BGE:"
        FCB   0
S_BLT   FCC   "BLT:"
        FCB   0
S_BGT   FCC   "BGT:"
        FCB   0
S_BLE   FCC   "BLE:"
        FCB   0
S_LBEQ  FCC   "LBEQ:"
        FCB   0
S_LBNE  FCC   "LBNE:"
        FCB   0

        ORG   $FFF4
        FDB   IRQ_ENT     ; FIRQ vector
        ORG   $FFF6
        FDB   IRQ_ENT     ; IRQ vector        
        ORG   $FFFE
        FDB   START