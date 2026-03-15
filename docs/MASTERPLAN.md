# FlashBangROM Master Plan

This file is the single source of truth for long-term project direction, durable decisions, constraints, and planning-relevant knowledge.

## Mandatory Process Reminder
- All long-term planning relevance must be written into this master plan.
- Every new session must start by reading this master plan.
- This section is mandatory and must remain at the top of the file.
- Execution history, implemented steps, investigations, and operational cleanup must be documented in `docs/DEV_LOG.md`.
- If an action results in a conclusion, that should be obayed in the future, this master plan must be updated to reflect the new durable truth.
- A task is not done until:
	- its durable planning implications are reflected here, and
	- its concrete execution steps are recorded in `docs/DEV_LOG.md`.

## Project Identity
- Project name: `FlashBangROM`.
- Product goal: ROM programmer platform built around STM32 boards and external parallel ROM/flash devices.
- Repository strategy: monorepo.
- Host application language: Rust.
- Current firmware platform: BluePill (`STM32F103C8`) using PlatformIO with Arduino framework.

## Scope
- Minimal viable scope:
	- BluePill + SST39 flash family.
- Next scope expansion:
	- BluePill + additional ROM/flash families.
	- Winbond devices are the highest-priority next family after SST39.
- Long-term hardware expansion:
	- BlackPill as a future platform target (if viable).

## Goals
- Deliver a reliable SST39 programming workflow with identify, read, erase, program, verify, and diff support.
- Preserve a path for adding more device families without rewriting the host/firmware contract.
- Keep firmware modular enough that command parsing, bus control, and chip-specific operations remain separable.
- Maintain GitHub-based release automation for tagged builds.
- Provide a beginner-friendly documentation to recreate the hardware and software setup.

## Nice-To-Haves
- Keep host-side UI and protocol work testable without requiring permanent hardware access.
- Support visual diffing of readback data against expected values in the host app.

## Non-Goals
- Support additional Microcontrollers that require additional Hardware to get to the 30 GPIOs needed for the target bus and control lines.

## Hardware Baseline
- Bus assignment strategy: GPIO ports A and B are assigned to buses in whole 8-bit blocks, which allows the firmware to read/write an entire byte via a single masked register access (`GPIOX->ODR` / `GPIOX->IDR`) without bit-banging individual pins.
- Current control pin allocation from firmware:
	- `PA14` -> `WE`
	- `PA13` -> `OE`
	- `PA15` -> `CE`
- Current address bus allocation from firmware:
	- `PA0..PA7` -> `A0..A7`   (low byte of address — Port A low byte)
	- `PB0..PB7` -> `A8..A15`  (high byte of address — Port B low byte)
	- `PA8..PA10` -> `A16..A18` (upper 3 address bits — Port A high byte, bits 8–10)
- Current data bus allocation from firmware:
	- `PB8..PB15` -> `D0..D7`  (data byte — Port B high byte, 5 V-tolerant pins)
- Host/device communication baseline:
	- serial line protocol over `115200` baud.

## Hardware Notes And Required Documentation
- The project depends on reclaiming almost all BluePill GPIOs for the ROM interface.
- `PA13`, `PA14`, and `PA15` are currently used as ROM control pins, which implies debug/SWD/JTAG default usage must be disabled or remapped appropriately.
- GPIO release procedure (must be called early in firmware init before configuring any of the affected pins):
  ```c
	__HAL_RCC_AFIO_CLK_ENABLE();       // Enable AFIO clock - required before any AFIO configuration

	__HAL_AFIO_REMAP_SWJ_DISABLE();    // Disable JTAG and SWD completely - releases PA13, PA14, PA15, PB3, PB4 as GPIO

	__HAL_RCC_SPI1_CLK_DISABLE();      // Shut down SPI1 peripheral - releases PA4, PA5, PA6, PA7
	__HAL_RCC_I2C1_CLK_DISABLE();      // Shut down I2C1 peripheral - releases PB6, PB7 (or PB8, PB9 if remapped)
	__HAL_RCC_USART1_CLK_DISABLE();    // Shut down USART1 peripheral - releases PA9, PA10
	__HAL_RCC_USART2_CLK_DISABLE();    // Shut down USART2 peripheral - releases PA2, PA3
  ```
- `BOOT1` / `PB2` hardware modification required to use `PB2` as address line `A10`:
  - `PB2` is physically tied to the `BOOT1` jumper on the BluePill through a `100 kΩ` series resistor.
  - That resistor must be **shorted** so the STM32 can drive the pin without it being loaded by the jumper circuit.
  - A `100 kΩ` pull-down to GND must be added between the `PB2` node and ground to prevent the HID-flash bootloader from being accidentally triggered.
  - Schematic before modification: `STM32(PB2) ──[100K]── BOOT1`
  - Schematic after modification:
    ```
    STM32(PB2) ────+──── BOOT1
                   |
                [100K]
                   |
                  GND
    ```
  - This modification does not affect any other normal BluePill functionality.
- Physical ROM socket wiring and electrical assumptions (SST39 DIP-32 baseline):
	- The definitive connection list is the table below (single source of truth for wiring).
	- Level assumptions:
		- Address and control outputs from STM32 are `3.3V` logic.
		- Data lines use `PB8..PB15`, which are 5V-tolerant on STM32F103 input side.
		- ROM at `5V` power must recognize STM32 `3.3V` high level on address/control inputs; this is accepted for current SST39 baseline wiring.
	- Pull-up/pull-down requirements:
		- Keep the `PB2/BOOT1` modification pull-down (`100K` to GND) in place as documented above.
		- No additional mandatory external pull resistors are currently required on address/data/control lines for baseline operation.
- Definitive hardware mapping table (BluePill <-> ROM, with reservation notes):

	| ROM signal | ROM pin | BluePill pin | Electrical note | Reservation note |
	|---|---:|---|---|---|
	| A0 | 12 | PA0 | 3.3V output | - |
	| A1 | 11 | PA1 | 3.3V output | - |
	| A2 | 10 | PA2 | 3.3V output | USART2 default pin (released when disabled) |
	| A3 | 9 | PA3 | 3.3V output | USART2 default pin (released when disabled) |
	| A4 | 8 | PA4 | 3.3V output | SPI1 default pin (released when disabled) |
	| A5 | 7 | PA5 | 3.3V output | SPI1 default pin (released when disabled) |
	| A6 | 6 | PA6 | 3.3V output | SPI1 default pin (released when disabled) |
	| A7 | 5 | PA7 | 3.3V output | SPI1 default pin (released when disabled) |
	| A8 | 27 | PB0 | 3.3V output | - |
	| A9 | 26 | PB1 | 3.3V output | - |
	| A10 | 23 | PB2 | 3.3V output | BOOT1 jumper pin, hardware mod required |
	| A11 | 25 | PB3 | 3.3V output | JTAG TDO default pin, SWJ disable required |
	| A12 | 4 | PB4 | 3.3V output | JTAG TRST default pin, SWJ disable required |
	| A13 | 28 | PB5 | 3.3V output | - |
	| A14 | 29 | PB6 | 3.3V output | I2C1 default pin (released when disabled) |
	| A15 | 3 | PB7 | 3.3V output | I2C1 default pin (released when disabled) |
	| A16 | 2 | PA8 | 3.3V output | - |
	| A17 | 30 | PA9 | 3.3V output | USART1 default pin (released when disabled) |
	| A18 | 1 | PA10 | 3.3V output | USART1 default pin (released when disabled) |
	| D0 | 13 | PB8 | 5V-tolerant data pin | I2C1 remap alt pin |
	| D1 | 14 | PB9 | 5V-tolerant data pin | I2C1 remap alt pin |
	| D2 | 15 | PB10 | 5V-tolerant data pin | - |
	| D3 | 17 | PB11 | 5V-tolerant data pin | - |
	| D4 | 18 | PB12 | 5V-tolerant data pin | - |
	| D5 | 19 | PB13 | 5V-tolerant data pin | - |
	| D6 | 20 | PB14 | 5V-tolerant data pin | - |
	| D7 | 21 | PB15 | 5V-tolerant data pin | - |
	| WE# | 31 | PA14 | 3.3V control | SWDCLK default pin, SWJ disable required |
	| OE# | 24 | PA13 | 3.3V control | SWDIO default pin, SWJ disable required |
	| CE# | 22 | PA15 | 3.3V control | JTAG TDI default pin, SWJ disable required |
	| VDD | 32 | +5V rail | ROM supply | - |
	| VSS | 16 | GND | Ground | - |

	CRITICAL: Reserved/unavailable BluePill pins for this baseline:
	- `PA11`, `PA12` reserved for USB/HID bootloader path.
	- `PC13` reserved for onboard LED.
	- `PC14`, `PC15` reserved for LSE/oscillator usage on many boards.

## Protocol Requirements
- The protocol must remain deterministic and easy to parse in firmware and host code.
- Protocol versioning is mandatory to prevent host/firmware drift.
- Current handshake requirement:
	- host sends `HELLO|<host-version>|<protocol-version>`
	- device responds `HELLO|<fw-version>|<protocol-version>|<capabilities>`
- Required core command set for the current minimal scope:
	- `ID`
	- `READ|<addr-hex>|<len-dec>`
	- `PROGRAM_BYTE|<addr-hex>|<value-hex>`
	- `SECTOR_ERASE|<addr-hex>`
	- `CHIP_ERASE`
	- `WRITE_STATUS|<addr-hex>|<expected-hex>|<timeout-ms-dec>`
- Required response types:
	- `OK|<command>|<context>`
	- `ERR|<code>|<message>`
	- `DATA|<addr-hex>|<len-dec>|<hex-bytes>`
	- `STATUS|<operation>|<phase>|<progress-dec>|<detail>`
- Required error vocabulary:
	- `E_PARSE`, `E_RANGE`, `E_ALIGN`, `E_UNSUPPORTED`, `E_TIMEOUT`, `E_VERIFY`, `E_HW`
- Current bring-up transport decision:
	- line-based framing
	- hex payloads for data frames
- Required verify flow:
	- host performs write operation
	- host reads back the affected range
	- host computes mismatch list and visual diff
	- host may persist a report file
- Normative chip behavior for SST39 must remain encoded in firmware and protocol docs:
	- unlock/program sequence
	- sector erase sequence
	- chip erase sequence
	- software product ID entry/read/exit
	- DQ6/DQ7-based completion detection
- Binary-safe framing remains a planned future protocol evolution, but not the current baseline.

## Planning Decisions
- `FlashBangROM` is the canonical project name going forward.
- BluePill + SST39 is the minimum shippable platform.
- BluePill + Winbond support is the next device-family priority after SST39.
- BlackPill support is a strategic later step, not a current baseline requirement.
- Rust remains the host language.
- PlatformIO + Arduino remains the firmware build stack until there is a concrete reason to migrate.
- GitHub release publishing is desired and should stay tag-driven.
- GUI work is allowed to progress in mock/demo mode before full hardware integration is finished.

## Insights For Future Sessions
- Early GUI and protocol work did not need permanent hardware access; the mock-device path was useful and should be preserved.
- A purely historical activity list in the master plan makes future planning harder; execution history belongs in `docs/DEV_LOG.md` instead.
- Rust/toolchain compatibility mattered in practice:
	- newer dependency versions pulled in requirements that did not fit the available Rust baseline,
	- therefore dependencies were pinned to versions compatible with Rust/Cargo `1.75`.
- GUI/library compatibility mattered in practice:
	- the native GUI path needed version pinning in the `eframe`/supporting crate stack to remain compatible with the chosen Rust baseline.
- Linux CI for the host build required additional system packages for the serial stack.
- GitHub release automation required explicit workflow token permissions (`contents: write`) in addition to a working build/test pipeline.
- Git-derived version metadata is useful and should stay, but it requires full tag/history availability in CI.
- Hardware timing assumptions are still provisional until confirmed on real boards with measurement tools.

## Constraints And Environment
- Hardware constraints:
	- BluePill GPIO count is tight for the targeted bus width and control lines.
	- BluePill clone variance can affect timing and electrical behavior.
	- Safe bus-direction switching is critical to avoid contention on the data bus.
- Build environment constraints:
	- Host build baseline should remain compatible with Rust/Cargo `1.75` unless deliberately upgraded.
	- Linux CI for the Rust host requires serial-stack build dependencies.
	- Firmware and host version strings are derived from Git tags/history.
- Quality constraints:
	- Datasheet timing and command assumptions must remain encoded, not left implicit.
	- Protocol changes must not silently break compatibility between host and firmware.
	- Real hardware validation is still required before trusting timing-sensitive behavior.

