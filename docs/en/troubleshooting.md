# Troubleshooting

Common pitfalls and how to fix them.

## Output is choppy/slow

- Increase flush interval/buffering: `--console-flush-every 128`
- Mirror large logs to a file: `--console-out <FILE>`

## Input not reaching the guest

- Add `--console-stdin`
- If you want local echo, add `--console-echo ON`
- For OS‑9, confirm device init order (use `--trace`)

## Start address is not as expected

- `--pc` has highest priority; otherwise: Reset vector > SREC entry (if invalid) > first loaded range start

## Verify branch/PC flow

- Use `--trace` to see instructions/registers/targets
- Disassemble ranges: `--disasm 0xE000:0xE100`

## Encoding/input issues

- Use a UTF‑8 terminal and a compatible font
- On Windows, note Ctrl+C exits the emulator (raw stdin)
