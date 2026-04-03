use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::session::ChipId;

#[derive(Debug, Deserialize)]
struct DriverModel {
    jedec_id: String,
    name: String,
    size_bytes: u32,
}

#[derive(Debug, Deserialize)]
struct DriverFile {
    id: String,
    address_bits: u8,
    sector_size_bytes: u32,
    sequences: DriverSequences,
    models: Vec<DriverModel>,
}

#[derive(Debug, Deserialize)]
struct DriverSequences {
    id_entry: String,
    id_read: String,
    id_exit: String,
    program_byte: String,
    program_range: Option<String>,
    sector_erase: String,
    chip_erase: String,
}

#[derive(Debug, Clone)]
pub struct DriverEntry {
    pub id: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DriverUploadPlan {
    pub driver_id: String,
    pub upload_lines: Vec<String>,
}

fn parse_jedec(jedec_id: &str) -> Option<(u8, u8)> {
    let s = jedec_id.trim();
    let raw = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    if raw.len() != 4 {
        return None;
    }
    let value = u16::from_str_radix(raw, 16).ok()?;
    Some(((value >> 8) as u8, (value & 0xFF) as u8))
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("drivers/chips"));
        dirs.push(cwd.join("../drivers/chips"));
    }
    dirs
}

fn parse_driver_file(path: &Path) -> Result<DriverFile, String> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    serde_yaml::from_str::<DriverFile>(&text)
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

pub fn list_drivers() -> Vec<DriverEntry> {
    let mut out = Vec::new();
    for dir in candidate_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("yaml") {
                continue;
            }
            if let Ok(driver) = parse_driver_file(&path) {
                out.push(DriverEntry {
                    id: driver.id,
                    path,
                });
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub fn build_upload_plan(path: &Path) -> Result<DriverUploadPlan, String> {
    let driver = parse_driver_file(path)?;
    let max_size_model = driver
        .models
        .iter()
        .max_by_key(|m| m.size_bytes)
        .ok_or_else(|| format!("driver {} has no models", driver.id))?;

    let mut upload_lines = Vec::new();
    // Before ID we do not know which model is physically inserted.
    // Use the largest model size in the selected family to avoid premature E_RANGE.
    upload_lines.push(format!("PARAMETER|CHIP_SIZE|{:X}", max_size_model.size_bytes));
    upload_lines.push(format!("PARAMETER|SECTOR_SIZE|{:X}", driver.sector_size_bytes));
    upload_lines.push(format!("PARAMETER|ADDR_BITS|{:X}", driver.address_bits));

    upload_lines.push(format!("SEQUENCE|ID_ENTRY|{}", driver.sequences.id_entry));
    upload_lines.push(format!("SEQUENCE|ID_READ|{}", driver.sequences.id_read));
    upload_lines.push(format!("SEQUENCE|ID_EXIT|{}", driver.sequences.id_exit));
    upload_lines.push(format!("SEQUENCE|PROGRAM_BYTE|{}", driver.sequences.program_byte));
    if let Some(program_range) = driver.sequences.program_range {
        upload_lines.push(format!("SEQUENCE|PROGRAM_RANGE|{}", program_range));
    }
    upload_lines.push(format!("SEQUENCE|SECTOR_ERASE|{}", driver.sequences.sector_erase));
    upload_lines.push(format!("SEQUENCE|CHIP_ERASE|{}", driver.sequences.chip_erase));

    Ok(DriverUploadPlan {
        driver_id: driver.id,
        upload_lines,
    })
}

fn lookup_in_dir(dir: &Path, mfr: u8, dev: u8) -> Option<ChipId> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("yaml") {
            continue;
        }
        let parsed = parse_driver_file(&path).ok()?;
        for model in parsed.models {
            let Some((mm, dd)) = parse_jedec(&model.jedec_id) else {
                continue;
            };
            if mm == mfr && dd == dev {
                return Some(ChipId {
                    manufacturer_id: mfr,
                    device_id: dev,
                    name: model.name,
                    size_bytes: model.size_bytes,
                    sector_size: parsed.sector_size_bytes,
                    driver_id: parsed.id,
                });
            }
        }
    }
    None
}

static CATALOG_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

pub fn lookup_chip(mfr: u8, dev: u8) -> Option<ChipId> {
    let selected = CATALOG_DIR.get_or_init(|| {
        for d in candidate_dirs() {
            if d.is_dir() {
                return Some(d);
            }
        }
        None
    });

    let dir = selected.as_ref()?;
    lookup_in_dir(dir, mfr, dev)
}
