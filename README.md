# FlashBang

FlashBang is a modular ROM flashing system for parallel NOR chips.

## Components
- `FlashBang Device Core`: STM32 firmware (BluePill first target).
- `FlashBang Studio`: Rust host application.
- `FlashBang Link Protocol`: host-device wire protocol.

## Scope Priority
- Core scope: SST39 family (`SST39SF010A`, `SST39SF020A`, `SST39SF040`).
- Extra scope: any non-SST39 chip support.

## Core SST39 Operations
- Read
- Byte-by-byte programming
- Sector erase
- Chip erase
- Write operation status detection
- Chip ID read
- Verify with visual diff in host app

## Repository Layout
- `firmware/` Device firmware
- `host/studio/` Host app (Rust)
- `protocol/` Protocol specification
- `drivers/` Chip descriptors and schema
- `docs/` Architecture and master planning
- `hardware/mods/` Hardware modification guides and images
- `Resources/` Datasheets and external references

## Important Docs
- Master plan: `docs/MASTERPLAN.md`
- Architecture: `docs/ARCHITECTURE.md`
- Protocol spec: `protocol/flashbang-link-protocol.md`
- Hardware mods: `hardware/mods/README.md`
- Release workflow: `docs/RELEASE_WORKFLOW.md`

## Versioning
- Firmware and host derive build metadata automatically from Git.
- Version format: `<latest-tag>+build.<commit-count>.<short-sha>`.
- Dirty worktrees add `.dirty` to the version text.
- Firmware exposes this in the `HELLO` frame.
- Host shows this in CLI output and the GUI About dialog.

## Tagging
- Use annotated or lightweight Git tags such as `v0.1.0`.
- The latest reachable tag becomes the base version for subsequent builds.
- If no tag exists yet, builds fall back to `0.0.0+build.<count>.<sha>`.

## Project Rule
All progress and decisions must be documented in `docs/MASTERPLAN.md`.
