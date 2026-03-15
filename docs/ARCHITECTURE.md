# FlashBang Architecture

## System
- Device: STM32 firmware running on BluePill (`STM32F103C8`) in first phase.
- Host: Rust desktop tool (`FlashBang Studio`).
- Protocol: Versioned line protocol (`FlashBang Link Protocol`).
- Drivers: Data-driven chip descriptors (`YAML`) validated by schema.

## Core Principle
SST39 first. All SST39 workflows are first-class and mandatory. Non-SST39 support is optional and isolated in `drivers/extras/`.

## Firmware Layers
- `app/`: startup and integration.
- `fsm/`: explicit state machine and transitions.
- `protocol/`: parse, validate, dispatch.
- `chip/`: SST39 command algorithms (read/program/erase/id/status).
- `bus/`: address/data/control bus services.
- `hal/`: board-specific pin and timing abstraction.

## Host Layers
- `transport/`: serial transport.
- `protocol/`: request/response codec.
- `device-service/`: high-level operations.
- `verify/`: byte-accurate compare and diff generation.
- `ui/`: operation views and visual diff output.

## Verify Requirement
Host must support verify and visual differences:
- Mismatch list with address, expected, actual.
- Aggregated mismatch count.
- View filters (all/mismatches only).
- Exportable report (text or JSON).

## Hardware Targets
- Primary: BluePill (required).
- Secondary: BlackPill (evaluation only until M4 complete).
