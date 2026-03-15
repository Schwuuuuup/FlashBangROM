# FlashBang Link Protocol v0.1 (Draft)

## Goals
- Keep parser deterministic and testable.
- Support SST39 core operations.
- Carry enough status information for robust host verify workflows.

## Versioning
- Host sends: `HELLO|<host-version>|<protocol-version>`
- Device responds: `HELLO|<fw-version>|<protocol-version>|<capabilities>`

## Core Commands (SST39)
- `ID` Chip ID read
- `READ|<addr-hex>|<len-dec>`
- `PROGRAM_BYTE|<addr-hex>|<value-hex>`
- `SECTOR_ERASE|<addr-hex>`
- `CHIP_ERASE`
- `WRITE_STATUS|<addr-hex>|<expected-hex>|<timeout-ms-dec>`

## SST39 Internal Sequences (Normative)
The firmware implementation for SST39 core must execute the following chip-side sequences.

### Byte Program (4 write cycles)
1. `WRITE 0x5555 <- 0xAA`
2. `WRITE 0x2AAA <- 0x55`
3. `WRITE 0x5555 <- 0xA0`
4. `WRITE <BA> <- <DATA>`

### Sector Erase (6 write cycles)
1. `WRITE 0x5555 <- 0xAA`
2. `WRITE 0x2AAA <- 0x55`
3. `WRITE 0x5555 <- 0x80`
4. `WRITE 0x5555 <- 0xAA`
5. `WRITE 0x2AAA <- 0x55`
6. `WRITE <SAx> <- 0x30`

### Chip Erase (6 write cycles)
1. `WRITE 0x5555 <- 0xAA`
2. `WRITE 0x2AAA <- 0x55`
3. `WRITE 0x5555 <- 0x80`
4. `WRITE 0x5555 <- 0xAA`
5. `WRITE 0x2AAA <- 0x55`
6. `WRITE 0x5555 <- 0x10`

### Software Product ID
Entry:
1. `WRITE 0x5555 <- 0xAA`
2. `WRITE 0x2AAA <- 0x55`
3. `WRITE 0x5555 <- 0x90`
4. wait `TIDA <= 150 ns`

Read:
- `READ 0x0000` => `0xBF`
- `READ 0x0001` => `0xB5`/`0xB6`/`0xB7`

Exit:
- `WRITE 0x5555 <- 0xAA`
- `WRITE 0x2AAA <- 0x55`
- `WRITE 0x5555 <- 0xF0`
- then wait `TIDA <= 150 ns`

## Status Detection
- Preferred: DQ6 toggle polling with stability double-check.
- Alternative: DQ7 data polling with stability double-check.
- Polling starts after the final write pulse of each operation.

## Timing Baseline (configuration defaults)
- `WAIT_POWER_UP_US = 100`
- `WAIT_POLL_INTERVAL_US = 1`
- `TIMEOUT_BYTE_PROGRAM_US = 50`
- `TIMEOUT_SECTOR_ERASE_US = 50000`
- `TIMEOUT_CHIP_ERASE_US = 250000`

## Optional Bulk Commands
- `PROGRAM_STREAM_BEGIN|<addr-hex>|<len-dec>`
- `PROGRAM_STREAM_DATA|<base64-chunk>`
- `PROGRAM_STREAM_END`

## Optional Diagnostic Commands
- `DATA_BUS_MONITOR_START` starts continuous sampling of `D7..D0` and emits `STATUS|DATA_BUS|SAMPLE|0|<8-bit-binary>` frames.
- `DATA_BUS_MONITOR_STOP` stops the continuous sampling stream.

## Responses
- `OK|<command>|<context>`
- `ERR|<code>|<message>`
- `DATA|<addr-hex>|<len-dec>|<hex-bytes>`
- `STATUS|<operation>|<phase>|<progress-dec>|<detail>`

## Error Codes
- `E_PARSE` malformed command
- `E_RANGE` address/length out of range
- `E_ALIGN` alignment constraint not met
- `E_UNSUPPORTED` unsupported command/chip/feature
- `E_TIMEOUT` operation timeout
- `E_VERIFY` verify mismatch detected
- `E_HW` hardware-level bus/chip failure

## Verify Flow
1. Host performs write operations.
2. Host issues readback for same range.
3. Host computes mismatch list and visual diff.
4. Host may persist report file.

## Notes
- This draft intentionally keeps line-based framing for easier bring-up.
- Current data payload framing is hex (`DATA|...|<hex-bytes>`) for deterministic parser/testing bring-up.
- Binary-safe framing upgrade (base64 or framed binary) can be introduced in protocol v0.2.
- Chip protocol details are tracked in `docs/SST39_CHIP_PROTOCOL_NOTES.md`.
