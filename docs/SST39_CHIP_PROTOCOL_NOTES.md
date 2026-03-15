# SST39 Chip Protocol Notes (from text source)

Source: `Resources/SST39SF_Protocol_fuer_KI.txt`

## Family
- `SST39SF010A`: 128 KiB, 17 address bits
- `SST39SF020A`: 256 KiB, 18 address bits
- `SST39SF040`: 512 KiB, 19 address bits

## Mandatory Core Operations
- Read
- Byte program (4-cycle sequence)
- Sector erase (6-cycle sequence)
- Chip erase (6-cycle sequence)
- Write completion detection (DQ6 toggle preferred, DQ7 polling alternative)
- Product ID read (entry/read/exit)

## Key Sequences
### Byte Program
1. `WRITE 0x5555 <- 0xAA`
2. `WRITE 0x2AAA <- 0x55`
3. `WRITE 0x5555 <- 0xA0`
4. `WRITE <BA> <- <DATA>`

### Sector Erase
1. `WRITE 0x5555 <- 0xAA`
2. `WRITE 0x2AAA <- 0x55`
3. `WRITE 0x5555 <- 0x80`
4. `WRITE 0x5555 <- 0xAA`
5. `WRITE 0x2AAA <- 0x55`
6. `WRITE <SAx> <- 0x30`

### Chip Erase
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
- `READ 0x0000` => manufacturer (`0xBF`)
- `READ 0x0001` => device (`0xB5`/`0xB6`/`0xB7`)

Exit:
- Fast method: `WRITE XX <- 0xF0` then wait `TIDA`

## Recommended Wait/Timeout Constants
- `WAIT_POWER_UP = 100 us`
- `WAIT_POLL_INTERVAL = 1 us`
- `TIMEOUT_BYTE_PROGRAM = 50 us`
- `TIMEOUT_SECTOR_ERASE = 50 ms`
- `TIMEOUT_CHIP_ERASE = 250 ms`

## Important Rules
- SDP sequence must not be interrupted.
- During internal program/erase, additional write commands are ignored.
- Verify completion with double-check to avoid DQ6/DQ7 race conditions.
- For sector erase command, use sector base: `addr & 0xFFFFF000`.
