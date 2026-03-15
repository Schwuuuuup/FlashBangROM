# FlashBang Master Plan

This file is the single source of truth for project status, decisions, risks, and next actions.

## Mandatory Process Reminder
- All information about project status must be written into this master plan.
- Every new session must start by reading this master plan.
- A task is not done until this master plan is updated.

## Current Scope
- Core target: SST39 family only.
- Extra target: non-SST39 chip support.

## Milestones

## M1 - SST39 Core Foundation
Status: In Progress

Deliverables:
- Repository scaffold with module boundaries.
- Protocol v0.1 contract-first skeleton.
- Driver schema and SST39 core descriptor.
- Firmware state-machine skeleton.
- Host app skeleton with verify/diff model design.
- Hardware mod docs skeleton.

Acceptance:
- Core SST39 operation list is explicitly specified in protocol and architecture docs.
- Verify and visual diff are explicit host requirements.
- No files in prototype directories are changed.

Progress log:
- 2026-03-15: Initialized repository structure under `STM32_ROM_Flasher`.
- 2026-03-15: Added root docs and baseline planning artifacts.
- 2026-03-15: Added protocol draft (`protocol/flashbang-link-protocol.md`) with explicit SST39 core operations.
- 2026-03-15: Added driver schema and initial `sst39-core` descriptor.
- 2026-03-15: Added firmware FSM skeleton (`firmware/src/main.cpp`) for core command flow.
- 2026-03-15: Added Rust host skeleton and verify diff module with passing unit tests (2/2).
- 2026-03-15: Added CI workflow stubs for host and firmware builds.
- 2026-03-15: Added normalized SST39 text-protocol notes in `docs/SST39_CHIP_PROTOCOL_NOTES.md` from `Resources/SST39SF_Protocol_fuer_KI.txt`.
- 2026-03-15: Updated protocol spec with normative SST39 command sequences (Program/Sector Erase/Chip Erase/ID Entry/Exit) and timing defaults.
- 2026-03-15: Updated `drivers/chips/sst39-core.yaml` timing defaults to match SST39 text spec.
- 2026-03-15: Replaced firmware mock-only flow with SST39 sequence-oriented command engine skeleton (unlock sequences, ID flow, toggle/DQ7 polling, timeout guards).
- 2026-03-15: Implemented BluePill HAL bus/control wiring in firmware (CE/OE/WE control, address/data bus access, data bus direction switching).
- 2026-03-15: Implemented `READ` command data output path with deterministic `DATA|<addr>|<len>|<hex-bytes>` chunk frames.
- 2026-03-15: Added host protocol parser module with golden tests for valid/malformed frames.
- 2026-03-15: Added verify report module with mismatch-range grouping and text/JSON export support.
- 2026-03-15: Validation status: `cargo test` passed (8/8), `pio run` passed for firmware build.
- 2026-03-15: Validation status: `cargo test` passed (8/8), `pio run` passed for firmware build.
- 2026-03-15: Added mock device simulator (`mock_device.rs`) — SST39SF040 in-memory responses (HELLO/ID/READ/ERASE/PROGRAM) without hardware.
- 2026-03-15: Added `DeviceSession` trait and `MockSession` (`session.rs`) — clean abstraction over real vs. mock serial backend.
- 2026-03-15: Added ratatui 0.27 TUI preview (`tui.rs`) — 3-tab terminal UI (Chip Info, Hex Dump, Diff View) fully runnable without MCU via `cargo run -- --demo`. Pinned ratatui=0.27.0/crossterm=0.27 for Rust 1.75 compatibility.
- 2026-03-15: Wired `--demo` flag in `main.rs`; `cargo test` still 8/8.
- 2026-03-15: Added real desktop GUI preview (`gui.rs`) using native window rendering (eframe/egui) with tabs for Chip Info, Hex Dump, Diff View and export actions.
- 2026-03-15: GUI now starts via `cargo run` (default) or `cargo run -- --gui`; existing TUI remains available via `cargo run -- --demo`.
- 2026-03-15: Pinned GUI dependency chain for Rust/Cargo 1.75 compatibility (`eframe 0.24.1`, `webbrowser 0.8.12`, `url 2.4.1`, `home 0.5.5`); validation status unchanged (`cargo test` 8/8).
- 2026-03-15: Added real serial groundwork in host (`serialport 4.3.0`) with reusable helpers in `session.rs` for port scan and open.
- 2026-03-15: Extended desktop GUI with live serial workflow: refresh/list ports, select port, set baud rate, connect/disconnect, and live connection status in top bar.
- 2026-03-15: Validation status after serial GUI integration: `cargo build` passed, `cargo test` passed (8/8).
- 2026-03-15: Added firmware version query from GUI (`HELLO` command) with parsing/display of firmware + protocol/capabilities in the Device panel.
- 2026-03-15: While connected, serial configuration controls (port selection, baud rate, refresh) are disabled to prevent runtime link changes; reconnect path remains explicit via Disconnect.
- 2026-03-15: Added serial debug monitor panel in GUI: TX lines shown in red and RX lines shown in green for trace/debugging.
- 2026-03-15: Added menu bar with `Help -> About FlashBang Studio` information dialog.
- 2026-03-15: Validation after latest GUI features: `cargo build` passed, `cargo test` passed (8/8).
- 2026-03-15: Refactored firmware from monolithic `main.cpp` into modular units for maintainability:
	- `include/`: `device_types.h`, `device_config.h`, `device_globals.h`, `protocol_io.h`, `command_parser.h`, `hal_bus.h`, `sst39_ops.h`, `command_executor.h`.
	- `src/`: `device_globals.cpp`, `protocol_io.cpp`, `command_parser.cpp`, `hal_bus.cpp`, `sst39_ops.cpp`, `command_executor.cpp`.
	- `main.cpp` now contains only setup/loop orchestration and FSM transitions.
- 2026-03-15: Validation after firmware modularization: `pio run` passed (Flash 47.0%, RAM 21.8%).
- 2026-03-15: Prepared repository for Git/GitHub workflows: added `.gitattributes`, expanded `.gitignore`, and updated CI checkout to fetch full history/tags for build metadata.
- 2026-03-15: Added automatic Git-derived version/build metadata for firmware and host.
	- Firmware: PlatformIO pre-build script generates `firmware/include/generated_build_info.h`; `HELLO` now reports dynamic version text.
	- Host: `build.rs` injects version/build/sha/dirty metadata; CLI and GUI About dialog display it.
	- Version format: `<latest-tag>+build.<commit-count>.<short-sha>` with optional `.dirty` suffix.
- 2026-03-15: Current repository state has no first commit yet; fallback build identifier is `0.0.0+build.0.nogit` until the initial commit exists.
- 2026-03-15: Validation after versioning integration: `cargo test` passed (8/8), `pio run` passed.
- 2026-03-15: Created initial repository commit (`4aa4a355`) and first semantic tag `v0.1.0`.
- 2026-03-15: Automatic versioning is now active from real history: firmware/host both report `0.1.0+build.1.4aa4a355` (clean tree).
- 2026-03-15: Post-tag validation passed: `pio run` success, `cargo test` success (8/8), host CLI prints dynamic version text.
- 2026-03-15: Added formal release process documentation (`docs/RELEASE_WORKFLOW.md`) and linked it from README.
- 2026-03-15: Added GitHub tag-release workflow (`.github/workflows/release.yml`) to build firmware, run host tests, upload firmware artifact, and create GitHub release notes on `v*` tags.
- 2026-03-15: Added ignore rules for generated host verify report files to keep working tree clean during normal usage.
- 2026-03-15: Validation after release-workflow prep: `cargo test` passed (8/8), `pio run` passed.
- 2026-03-15: Configured GitHub remote (`origin`) and published local repository to `https://github.com/Schwuuuuup/FlashBangROM.git`.
- 2026-03-15: Resolved non-fast-forward push by replacing remote `main` with local history as requested (`--force-with-lease`), then pushed tag `v0.1.0`.
- 2026-03-15: Local `main` now tracks `origin/main`; CI/release workflows are now active on GitHub.
- 2026-03-15: Cleaned up obsolete remote branch `copilot/extend-flashbang-system-features`; only `origin/main` remains.
- 2026-03-15: First release workflow run from tag `v0.1.1` failed in `Test Host` on GitHub Actions.
- 2026-03-15: Hardened CI/release host test environment by installing Linux build deps (`pkg-config`, `libudev-dev`) and enforcing `cargo test --locked` in both `host-ci` and `release` workflows.

## M2 - Firmware Command Engine (SST39)
Status: In Progress

Deliverables:
- Parser and explicit FSM transitions.
- Read/ID/erase/program/status paths wired to BluePill HAL.
- Unit-testable command handlers.

## M3 - Host Protocol + Verify UI
Status: In Progress

Deliverables:
- Rust protocol client and command queue.
- Read/write/erase/identify workflows.
- Verify execution and visual diff output.

## M4 - BluePill Hardware Validation
Status: Not Started

Deliverables:
- Validation checklist for wiring and boot pin mod.
- Smoke tests on real hardware.
- Known issues and mitigations documented.

## M5 - GitHub Release Readiness
Status: Not Started

Deliverables:
- CI workflows.
- Contribution and security docs.
- First tagged pre-release.

## Decision Log
- 2026-03-15: Product name is `FlashBang`.
- 2026-03-15: Use monorepo strategy.
- 2026-03-15: Host app will be Rust-based.
- 2026-03-15: SST39 family is core scope; other chips are extras.

## Risks
- Datasheet interpretation drift if command timings are not encoded and tested.
- Hardware variance across BluePill clones.
- Protocol changes can break host/firmware compatibility without version gating.
- Hardware correctness still depends on live board verification (logic/timing on real BluePill + flash chip).
- Current protocol payload format is hex for bring-up; binary-safe framing/base64 remains a planned protocol evolution.

## Next Actions
1. Perform real hardware validation of HAL timing and control sequencing on BluePill (scope/logic-analyzer checks for WE/OE/CE and bus stability).
2. Add firmware-side protocol self-check tests for parser/command routing and error paths (`E_PARSE`, `E_RANGE`, `E_TIMEOUT`) against the new modular parser/executor units.
3. Build `RealSession` on top of connected serial port for `ID/READ/ERASE/PROGRAM` command/response framing and timeout/error handling (HELLO query already available in GUI).
4. Extend desktop GUI Diff View: per-byte detail table, mismatch-highlighted Hex Dump, and range jump/navigation.
5. Add detailed BluePill hardware modification steps with photo placeholders and checklist.
6. **GUI demo available now**: `cargo run` (or `cargo run -- --gui`) launches a native desktop preview without hardware.
7. Optional fallback demo: `cargo run -- --demo` launches the terminal UI preview.
8. Create a follow-up release tag after CI fix and confirm GitHub release generation plus attached firmware artifact.
