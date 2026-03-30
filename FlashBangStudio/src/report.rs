use serde::Serialize;
use std::fs;
use std::path::Path;

use crate::verify::{ByteMismatch, DiffSummary};

#[derive(Debug, Clone, Serialize)]
pub struct MismatchRange {
    pub start_address: u32,
    pub end_address: u32,
    pub mismatch_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub start_address: u32,
    pub compared_len: usize,
    pub mismatch_count: usize,
    pub ranges: Vec<MismatchRange>,
    pub mismatches: Vec<ByteMismatch>,
}

pub fn build_report(summary: &DiffSummary) -> DiffReport {
    DiffReport {
        start_address: summary.start_address,
        compared_len: summary.compared_len,
        mismatch_count: summary.mismatch_count,
        ranges: group_mismatches(&summary.mismatches),
        mismatches: summary.mismatches.clone(),
    }
}

pub fn group_mismatches(mismatches: &[ByteMismatch]) -> Vec<MismatchRange> {
    if mismatches.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut start = mismatches[0].address;
    let mut end = start;
    let mut count = 1usize;

    for m in mismatches.iter().skip(1) {
        if m.address == end + 1 {
            end = m.address;
            count += 1;
        } else {
            ranges.push(MismatchRange {
                start_address: start,
                end_address: end,
                mismatch_count: count,
            });
            start = m.address;
            end = m.address;
            count = 1;
        }
    }

    ranges.push(MismatchRange {
        start_address: start,
        end_address: end,
        mismatch_count: count,
    });
    ranges
}

pub fn export_report_json(path: &Path, report: &DiffReport) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(report)
        .map_err(std::io::Error::other)?;
    fs::write(path, json)
}

pub fn export_report_text(path: &Path, report: &DiffReport) -> std::io::Result<()> {
    let mut out = String::new();
    out.push_str("FlashBang Verify Report\n");
    out.push_str("=====================\n");
    out.push_str(&format!("Start: 0x{:05X}\n", report.start_address));
    out.push_str(&format!("Compared length: {}\n", report.compared_len));
    out.push_str(&format!("Mismatch count: {}\n", report.mismatch_count));
    out.push_str("\nRanges:\n");
    for r in &report.ranges {
        out.push_str(&format!(
            "- 0x{:05X}..0x{:05X} ({} bytes)\n",
            r.start_address, r.end_address, r.mismatch_count
        ));
    }
    out.push_str("\nMismatches:\n");
    for m in &report.mismatches {
        out.push_str(&format!(
            "- 0x{:05X}: expected=0x{:02X} actual=0x{:02X}\n",
            m.address, m.expected, m.actual
        ));
    }
    fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::compute_diff;

    #[test]
    fn groups_contiguous_mismatches() {
        let expected = vec![0x00, 0x01, 0x02, 0x03, 0x04];
        let actual = vec![0xFF, 0xEE, 0x02, 0xAA, 0xBB];
        let summary = compute_diff(0x1000, &expected, &actual);
        let report = build_report(&summary);

        assert_eq!(report.mismatch_count, 4);
        assert_eq!(report.ranges.len(), 2);
        assert_eq!(report.ranges[0].start_address, 0x1000);
        assert_eq!(report.ranges[0].end_address, 0x1001);
        assert_eq!(report.ranges[1].start_address, 0x1003);
        assert_eq!(report.ranges[1].end_address, 0x1004);
    }
}
