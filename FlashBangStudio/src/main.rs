mod mock_device;
mod engine;
mod driver_catalog;
mod protocol;
mod report;
mod session;
mod tui;
mod gui;
mod verify;
mod version;

use protocol::parse_device_frame;
use report::{build_report, export_report_text};
use std::path::PathBuf;
use verify::{compute_diff, DiffSummary};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--demo") {
        if let Err(e) = tui::run_tui() {
            eprintln!("TUI error: {e}");
        }
        return;
    }

    if args.iter().any(|a| a == "--gui") || args.len() == 1 {
        if let Err(e) = gui::run_gui() {
            eprintln!("GUI error: {e}");
        }
        return;
    }

    println!("FlashBang Studio {}", version::version_text());

    // Placeholder demo for verify + visual diff pipeline bootstrap.
    let expected = vec![0xFF, 0xAA, 0x55, 0x10, 0x20];
    let actual = vec![0xFF, 0xAA, 0x50, 0x10, 0x21];
    let summary: DiffSummary = compute_diff(0x0000, &expected, &actual);

    println!("verify mismatches: {}", summary.mismatch_count);
    for m in summary.mismatches.iter().take(10) {
        println!(
            "diff @0x{addr:05X}: expected=0x{exp:02X} actual=0x{act:02X}",
            addr = m.address,
            exp = m.expected,
            act = m.actual
        );
    }

    let report = build_report(&summary);
    let report_path = PathBuf::from("flashbang-verify-report.txt");
    if let Err(e) = export_report_text(&report_path, &report) {
        eprintln!("failed to write report: {e}");
    }

    let hello_demo = format!(
        "HELLO|flashbang-fw-dev-{}|{}|driver-upload",
        version::supported_protocol_version(),
        version::supported_protocol_version()
    );
    if let Ok(frame) = parse_device_frame(&hello_demo) {
        println!("parsed frame: {frame:?}");
    }
}
