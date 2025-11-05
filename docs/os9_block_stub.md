OS-9 Block I/O Driver Stub (em6809 BlockDev)

This shows a minimal sequence to read/write 512-byte sectors using the emulator’s BlockDev at base 0xFF20.

Registers (base = 0xFF20):
- +0 DATA (R/W): sector data port (streams bytes)
- +1 STATUS (R): bit0=BUSY, bit1=DRDY, bit2=ERR. Reading clears ERR.
- +2 COMMAND (W/R): 0x01=READ, 0x02=WRITE, 0xFF=RESET
- +3 SECCNT (R/W): number of sectors to transfer
- +4..+7 LBA0..3 (R/W): 32-bit LBA, little-endian
- +8 CONTROL (R/W): bit0 IRQ_EN, bit1 FIRQ, bit2 clear ERR when set

Read 1 sector (LBA in X:D)
  LDX   #$0000      ; LBA hi
  LDD   #$0005      ; LBA lo = 5
  STB   $FF24       ; LBA0
  STA   $FF25       ; LBA1
  STX   $FF26       ; LBA2..3
  LDA   #$01        ; SECCNT = 1
  STA   $FF23
  LDA   #$01        ; COMMAND=READ
  STA   $FF22
wait_drdy:
  LDA   $FF21       ; STATUS
  BITA  #$02        ; DRDY?
  BEQ   wait_drdy
  LDX   #buffer
  LDU   #512
rd_loop:
  LDA   $FF20       ; DATA
  STA   ,X+
  LEAU  -1,U
  BNE   rd_loop
  ; done

Write 1 sector (LBA in X:D)
  ; Set LBA similar to above, SECCNT=1
  LDX   #buffer
  LDU   #512
wr_loop:
  LDA   ,X+
  STA   $FF20
  LEAU  -1,U
  BNE   wr_loop
  LDA   #$02
  STA   $FF22      ; COMMAND=WRITE (commits current buffer)

Error handling
  LDA   $FF21      ; STATUS (read clears ERR bit)
  BITA  #$04       ; non-zero means ERR was set

IRQ/FIRQ
  ; Optionally enable interrupts for DRDY/complete
  LDA   #$01       ; IRQ_EN
  STA   $FF28      ; CONTROL

