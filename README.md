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
- `FlashBangFirmware/` Device firmware
- `FlashBangStudio/` Host app (Rust)
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

---

## AI Disclaimer

This project was made possible with massive AI assistance (GitHub Copilot / Claude / ChatGPT).
Without it, this would have taken a hobbyist significantly longer — or simply never happened.

**Context:** This is a one-man hobby project.
No jobs were threatened by the use of AI.
It was used purely to save time and to bridge knowledge gaps in areas like Rust GUI, STM32 peripheral details, and flash chip protocols.

**Safety:** This software should **not** be used in any safety-critical environment.
Use it at your own risk.
No guarantees are made regarding correctness, reliability, or fitness for any particular purpose.

**Security:** The GUI application is written in Rust.
Beyond that, no special security measures have been taken.
Yes, this confirms every preconception Rust sceptics have — but this is not a kernel module.

**Human contributions are very welcome.**
If you have licence-free drop-in replacements for any code or assets (icons, fonts, protocol implementations, chip driver definitions, …), feel free to open a pull request or send them in.
