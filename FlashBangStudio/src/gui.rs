use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::collections::HashMap;

use eframe::egui;
use serialport::SerialPort;
use tinyfiledialogs::{open_file_dialog, save_file_dialog};

use crate::{
    protocol::{parse_device_frame, DeviceFrame},
    report::{build_report, DiffReport},
    session::{
        list_serial_ports, open_serial_port, ChipId, HelloInfo, SerialPortEntry,
    },
    verify::compute_diff,
    version,
};

pub fn run_gui() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "FlashBang Studio - GUI Demo",
        options,
        Box::new(|_cc| Box::new(FlashBangGuiApp::new())),
    )
}

#[derive(Default)]
struct AppData {
    hello: Option<HelloInfo>,
    chip: Option<ChipId>,
    ro_data: Vec<u8>,
    ro_known: Vec<bool>,
    work_data: Vec<u8>,
    diff_report: Option<DiffReport>,
    log: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorMode {
    Diff,
    Palette,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CharacterMode {
    Hex,
    Ascii,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    Inspector,
    Workspace,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum BaseIcon {
    Chip,
    Inspector,
    Workbench,
    Disk,
    Trash,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum OverlayIcon {
    Image,
    Sector,
    Range,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ArrowIcon {
    Erase,
    Fetch,
    Flash,
    Copy,
    Load,
    Save,
}

#[derive(Clone, Copy)]
struct ButtonVisualSpec {
    left_base: BaseIcon,
    left_overlay: Option<OverlayIcon>,
    arrow: ArrowIcon,
    right_overlay: Option<OverlayIcon>,
    right_base: BaseIcon,
}

struct IconAssets {
    base: HashMap<BaseIcon, image::RgbaImage>,
    overlays: HashMap<OverlayIcon, image::RgbaImage>,
    arrows: HashMap<ArrowIcon, image::RgbaImage>,
    composites: HashMap<String, egui::TextureHandle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ByteState {
    Gray,
    Green,
    Orange,
    Red,
}

#[derive(Clone, Copy)]
enum WireDirection {
    Tx,
    Rx,
}

struct WireLogEntry {
    direction: WireDirection,
    text: String,
}

pub struct FlashBangGuiApp {
    data: AppData,
    available_ports: Vec<SerialPortEntry>,
    selected_port_index: usize,
    baud_rate: u32,
    connected_port_name: Option<String>,
    serial_handle: Option<Box<dyn SerialPort>>,
    wire_log: Vec<WireLogEntry>,
    show_about: bool,
    status: String,
    color_mode: ColorMode,
    character_mode: CharacterMode,
    show_sector_boundaries: bool,
    allow_flash_gray: bool,
    range_start_input: String,
    range_len_input: String,
    sector_input: String,
    file_path_input: String,
    clipboard: Vec<u8>,
    clipboard_desc: String,
    selected_ro_addr: Option<usize>,
    selected_work_addr: Option<usize>,
    active_pane: Pane,
    pending_hex_high_nibble: Option<u8>,
    icon_assets: Option<IconAssets>,
    upper_area_ratio: f32,
}

impl FlashBangGuiApp {
    fn fallback_chip() -> ChipId {
        ChipId {
            manufacturer_id: 0xBF,
            device_id: 0xB7,
            name: "SST39SF040",
            size_bytes: 512 * 1024,
            sector_size: 4096,
        }
    }

    fn ensure_fallback_chip(&mut self) {
        if self.data.chip.is_none() {
            self.data.chip = Some(Self::fallback_chip());
            self.ensure_chip_buffers();
        }
    }

    fn new() -> Self {
        let data = AppData::default();

        let available_ports = list_serial_ports().unwrap_or_default();

        let mut app = FlashBangGuiApp {
            data,
            available_ports,
            selected_port_index: 0,
            baud_rate: 115_200,
            connected_port_name: None,
            serial_handle: None,
            wire_log: Vec::new(),
            show_about: false,
            status: "Nicht verbunden. Verbinde ein Geraet fuer Live-Daten.".to_string(),
            color_mode: ColorMode::Diff,
            character_mode: CharacterMode::Hex,
            show_sector_boundaries: true,
            allow_flash_gray: false,
            range_start_input: "00000".to_string(),
            range_len_input: "256".to_string(),
            sector_input: "0".to_string(),
            file_path_input: "captures/rom_inspector.bin".to_string(),
            clipboard: Vec::new(),
            clipboard_desc: "empty".to_string(),
            selected_ro_addr: None,
            selected_work_addr: None,
            active_pane: Pane::Workspace,
            pending_hex_high_nibble: None,
            icon_assets: None,
            upper_area_ratio: 0.75,
        };

        app.ensure_fallback_chip();
        app
    }

    fn draw_serial_monitor(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Serial Monitor (TX/RX)");
        egui::ScrollArea::vertical()
            .id_source("serial_monitor_scroll")
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for entry in &self.wire_log {
                    let (prefix, color) = match entry.direction {
                        WireDirection::Tx => ("TX", egui::Color32::RED),
                        WireDirection::Rx => ("RX", egui::Color32::GREEN),
                    };
                    ui.label(
                        egui::RichText::new(format!("[{prefix}] {}", entry.text))
                            .monospace()
                            .color(color),
                    );
                }
            });
    }

    fn ensure_chip_buffers(&mut self) {
        let Some(chip) = &self.data.chip else {
            return;
        };
        let wanted = chip.size_bytes as usize;
        if self.data.ro_data.len() != wanted {
            self.data.ro_data = vec![0xFF; wanted];
            self.data.ro_known = vec![false; wanted];
            self.data.work_data = vec![0xFF; wanted];
            self.selected_ro_addr = None;
            self.selected_work_addr = None;
            self.pending_hex_high_nibble = None;
            self.rebuild_diff_report();
        }
    }

    fn rebuild_diff_report(&mut self) {
        if self.data.ro_data.is_empty() || self.data.work_data.is_empty() {
            self.data.diff_report = None;
            return;
        }
        let summary = compute_diff(0, &self.data.ro_data, &self.data.work_data);
        self.data.diff_report = Some(build_report(&summary));
    }

    fn chip_size(&self) -> Option<usize> {
        self.data.chip.as_ref().map(|c| c.size_bytes as usize)
    }

    fn chip_status_text(&self) -> Option<String> {
        self.data.chip.as_ref().map(|chip| {
            format!(
                "Chip erkannt: {} (man 0x{:02X} dev 0x{:02X} / {}K / {}B/S / {} Sectors)",
                chip.name,
                chip.manufacturer_id,
                chip.device_id,
                chip.size_bytes / 1024,
                chip.sector_size,
                chip.sector_count(),
            )
        })
    }

    fn sector_size(&self) -> Option<usize> {
        self.data.chip.as_ref().map(|c| c.sector_size as usize)
    }

    fn parse_int_input(text: &str) -> Result<u32, String> {
        let cleaned = text.trim();
        if cleaned.is_empty() {
            return Err("empty input".to_string());
        }
        if let Some(hex) = cleaned
            .strip_prefix("0x")
            .or_else(|| cleaned.strip_prefix("0X"))
        {
            return u32::from_str_radix(hex, 16).map_err(|e| format!("invalid hex: {e}"));
        }
        if cleaned.chars().any(|c| matches!(c, 'A'..='F' | 'a'..='f')) {
            return u32::from_str_radix(cleaned, 16).map_err(|e| format!("invalid hex: {e}"));
        }
        cleaned
            .parse::<u32>()
            .map_err(|e| format!("invalid number: {e}"))
    }

    fn parse_hex_input(text: &str) -> Result<u32, String> {
        let cleaned = text.trim();
        if cleaned.is_empty() {
            return Err("empty input".to_string());
        }
        let hex = cleaned
            .strip_prefix("0x")
            .or_else(|| cleaned.strip_prefix("0X"))
            .unwrap_or(cleaned);
        u32::from_str_radix(hex, 16).map_err(|e| format!("invalid hex: {e}"))
    }

    fn parse_range_input(&self) -> Result<(usize, usize), String> {
        let start = Self::parse_hex_input(&self.range_start_input)? as usize;
        let len = Self::parse_int_input(&self.range_len_input)? as usize;
        if len == 0 {
            return Err("range len must be > 0".to_string());
        }
        let chip_size = self.chip_size().ok_or_else(|| "chip unknown".to_string())?;
        if start >= chip_size {
            return Err("range start outside chip".to_string());
        }
        if start + len > chip_size {
            return Err("range exceeds chip size".to_string());
        }
        Ok((start, len))
    }

    fn parse_sector_input(&self) -> Result<(usize, usize, usize), String> {
        let sector_index = Self::parse_int_input(&self.sector_input)? as usize;
        let chip = self
            .data
            .chip
            .as_ref()
            .ok_or_else(|| "chip unknown".to_string())?;
        let sector_size = chip.sector_size as usize;
        let sector_count = chip.sector_count() as usize;
        if sector_index >= sector_count {
            return Err(format!("sector out of range 0..{}", sector_count.saturating_sub(1)));
        }
        let start = sector_index * sector_size;
        Ok((sector_index, start, sector_size))
    }

    fn mark_ro_unknown(&mut self, start: usize, len: usize) {
        if self.data.ro_known.is_empty() {
            return;
        }
        let end = (start + len).min(self.data.ro_known.len());
        for known in &mut self.data.ro_known[start..end] {
            *known = false;
        }
        self.rebuild_diff_report();
    }

    fn byte_state(&self, addr: usize) -> ByteState {
        if addr >= self.data.ro_data.len() || addr >= self.data.work_data.len() {
            return ByteState::Gray;
        }
        if !self.data.ro_known.get(addr).copied().unwrap_or(false) {
            return ByteState::Gray;
        }
        let ro = self.data.ro_data[addr];
        let work = self.data.work_data[addr];
        if ro == work {
            return ByteState::Green;
        }
        if (ro & work) == work {
            return ByteState::Orange;
        }
        ByteState::Red
    }

    fn diff_color_for_state(state: ByteState) -> egui::Color32 {
        match state {
            ByteState::Gray => egui::Color32::from_gray(140),
            ByteState::Green => egui::Color32::from_rgb(0x3A, 0xD1, 0x5A),
            ByteState::Orange => egui::Color32::from_rgb(0xF0, 0xA0, 0x2C),
            ByteState::Red => egui::Color32::from_rgb(0xE0, 0x50, 0x45),
        }
    }

    fn palette_color(byte: u8) -> egui::Color32 {
        let r = ((byte >> 5) & 0x07) as u16 * 255 / 7;
        let g = ((byte >> 2) & 0x07) as u16 * 255 / 7;
        let b = (byte & 0x03) as u16 * 255 / 3;
        egui::Color32::from_rgb(r as u8, g as u8, b as u8)
    }

    fn decode_latin15(byte: u8) -> char {
        match byte {
            0xA4 => '\u{20AC}',
            0xA6 => '\u{0160}',
            0xA8 => '\u{0161}',
            0xB4 => '\u{017D}',
            0xB8 => '\u{017E}',
            0xBC => '\u{0152}',
            0xBD => '\u{0153}',
            0xBE => '\u{0178}',
            _ => byte as char,
        }
    }

    fn encode_latin15(ch: char) -> Option<u8> {
        match ch {
            '\u{20AC}' => Some(0xA4),
            '\u{0160}' => Some(0xA6),
            '\u{0161}' => Some(0xA8),
            '\u{017D}' => Some(0xB4),
            '\u{017E}' => Some(0xB8),
            '\u{0152}' => Some(0xBC),
            '\u{0153}' => Some(0xBD),
            '\u{0178}' => Some(0xBE),
            _ => {
                let code = ch as u32;
                if code <= 0xFF {
                    Some(code as u8)
                } else {
                    None
                }
            }
        }
    }

    fn display_char_for_byte(byte: u8) -> char {
        match byte {
            0x00..=0x1F => char::from_u32(0x2400 + (byte as u32)).unwrap_or('\u{FFFD}'),
            0x7F => '\u{2421}',
            _ => Self::decode_latin15(byte),
        }
    }

    fn display_text_for_byte(&self, byte: u8) -> String {
        match self.character_mode {
            CharacterMode::Hex => format!("{byte:02X}"),
            CharacterMode::Ascii => Self::display_char_for_byte(byte).to_string(),
        }
    }

    fn clipboard_hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn decode_clipboard_hex(text: &str) -> Result<Vec<u8>, String> {
        let mut compact = String::new();
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch.is_ascii_whitespace() || matches!(ch, ',' | ';' | '_' | '-') {
                continue;
            }
            if ch == '0' {
                if let Some('x' | 'X') = chars.peek().copied() {
                    chars.next();
                    continue;
                }
            }
            if ch.is_ascii_hexdigit() {
                compact.push(ch);
            } else {
                return Err(format!("clipboard contains unsupported character '{ch}'"));
            }
        }

        if compact.is_empty() {
            return Err("clipboard text is empty".to_string());
        }
        if compact.len() % 2 != 0 {
            return Err("clipboard hex has odd digit count".to_string());
        }

        let mut out = Vec::with_capacity(compact.len() / 2);
        let bytes = compact.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            let chunk = std::str::from_utf8(&bytes[index..index + 2])
                .map_err(|e| format!("clipboard utf8 error: {e}"))?;
            let value = u8::from_str_radix(chunk, 16)
                .map_err(|e| format!("invalid clipboard hex byte '{chunk}': {e}"))?;
            out.push(value);
            index += 2;
        }
        Ok(out)
    }

    fn set_work_byte(&mut self, addr: usize, value: u8) {
        if addr < self.data.work_data.len() {
            self.data.work_data[addr] = value;
            self.rebuild_diff_report();
        }
    }

    fn paste_bytes_into_work(&mut self, start: usize, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty() {
            return Err("clipboard is empty".to_string());
        }
        if start + bytes.len() > self.data.work_data.len() {
            return Err("paste exceeds workspace".to_string());
        }
        let end = start + bytes.len();
        self.data.work_data[start..end].copy_from_slice(bytes);
        self.rebuild_diff_report();
        self.status = format!("Pasted {} byte(s) into workspace at 0x{start:05X}", bytes.len());
        Ok(())
    }

    fn paste_text_into_work(&mut self, start: usize, text: &str) -> Result<(), String> {
        let bytes = Self::decode_clipboard_hex(text)?;
        self.clipboard = bytes.clone();
        self.clipboard_desc = format!("System clipboard +{}", bytes.len());
        self.paste_bytes_into_work(start, &bytes)
    }

    fn choose_open_file(&mut self) -> bool {
        if let Some(path) = open_file_dialog("Open ROM image", &self.file_path_input, None) {
            self.file_path_input = path;
            return true;
        }
        false
    }

    fn choose_save_file(&mut self, suggested_name: &str) -> bool {
        let default_path = if self.file_path_input.trim().is_empty() {
            suggested_name.to_string()
        } else {
            self.file_path_input.clone()
        };
        if let Some(path) = save_file_dialog("Save ROM image", &default_path) {
            self.file_path_input = path;
            return true;
        }
        false
    }

    fn handle_workspace_typing(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            match event {
                egui::Event::Copy => {
                    match self.parse_range_input() {
                        Ok((start, len)) => {
                            if let Err(err) = self.copy_range_into_clipboard(ctx, start, len, self.active_pane) {
                                self.status = format!("Copy failed: {err}");
                            }
                        }
                        Err(e) => {
                            self.status = format!("Copy failed (invalid range): {e}");
                        }
                    }
                }
                egui::Event::Paste(text) => {
                    if self.active_pane == Pane::Workspace {
                        if let Some(current_addr) = self.selected_work_addr {
                            if let Err(err) = self.paste_text_into_work(current_addr, &text) {
                                self.status = format!("Paste failed: {err}");
                            }
                        } else {
                            self.status = "Paste failed: no workspace cursor selected".to_string();
                        }
                    }
                }
                egui::Event::Text(text) => {
                    if self.active_pane != Pane::Workspace {
                        continue;
                    }
                    for ch in text.chars() {
                        match self.character_mode {
                            CharacterMode::Hex => {
                                let Some(nibble) = ch.to_digit(16).map(|v| v as u8) else {
                                    continue;
                                };
                                let Some(current_addr) = self.selected_work_addr else {
                                    continue;
                                };
                                if current_addr >= self.data.work_data.len() {
                                    continue;
                                }
                                if let Some(high) = self.pending_hex_high_nibble {
                                    self.set_work_byte(current_addr, high | nibble);
                                    self.pending_hex_high_nibble = None;
                                    if current_addr + 1 < self.data.work_data.len() {
                                        self.selected_work_addr = Some(current_addr + 1);
                                    }
                                } else {
                                    self.pending_hex_high_nibble = Some(nibble << 4);
                                }
                            }
                            CharacterMode::Ascii => {
                                let Some(value) = Self::encode_latin15(ch) else {
                                    continue;
                                };
                                let Some(current_addr) = self.selected_work_addr else {
                                    continue;
                                };
                                self.set_work_byte(current_addr, value);
                                if current_addr + 1 < self.data.work_data.len() {
                                    self.selected_work_addr = Some(current_addr + 1);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn refresh_ports(&mut self) {
        match list_serial_ports() {
            Ok(ports) => {
                self.available_ports = ports;
                if self.selected_port_index >= self.available_ports.len() {
                    self.selected_port_index = 0;
                }
                self.status = format!("Found {} serial port(s)", self.available_ports.len());
            }
            Err(e) => {
                self.available_ports.clear();
                self.selected_port_index = 0;
                self.status = format!("Port scan failed: {e}");
            }
        }
    }

    fn push_wire(&mut self, direction: WireDirection, text: impl Into<String>) {
        self.wire_log.push(WireLogEntry {
            direction,
            text: text.into(),
        });
        if self.wire_log.len() > 500 {
            let drain = self.wire_log.len() - 500;
            self.wire_log.drain(0..drain);
        }
    }

    fn serial_send_and_read_lines(
        &mut self,
        command: &str,
        max_lines: usize,
    ) -> Result<Vec<String>, String> {
        let mut rx_to_log: Vec<String> = Vec::new();
        let lines = {
            let handle = self
                .serial_handle
                .as_mut()
                .ok_or_else(|| "not connected".to_string())?;

            let tx_line = format!("{command}\n");
            handle
                .write_all(tx_line.as_bytes())
                .map_err(|e| format!("write failed: {e}"))?;

            let mut lines = Vec::new();
            let mut buffer = Vec::new();
            let mut byte = [0_u8; 1];

            while lines.len() < max_lines {
                match handle.read(&mut byte) {
                    Ok(1) => {
                        if byte[0] == b'\n' {
                            let line = String::from_utf8_lossy(&buffer).trim().to_string();
                            buffer.clear();
                            if !line.is_empty() {
                                rx_to_log.push(line.clone());
                                lines.push(line.clone());
                                if line.starts_with("OK|") || line.starts_with("ERR|") {
                                    break;
                                }
                            }
                        } else if byte[0] != b'\r' {
                            buffer.push(byte[0]);
                        }
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                        if !buffer.is_empty() {
                            let line = String::from_utf8_lossy(&buffer).trim().to_string();
                            if !line.is_empty() {
                                rx_to_log.push(line.clone());
                                lines.push(line);
                            }
                        }
                        break;
                    }
                    Err(e) => return Err(format!("read failed: {e}")),
                }
            }
            lines
        };

        self.push_wire(WireDirection::Tx, command.to_string());
        for line in rx_to_log {
            self.push_wire(WireDirection::Rx, line);
        }
        Ok(lines)
    }

    fn query_firmware_version(&mut self) {
        match self.serial_send_and_read_lines("HELLO", 4) {
            Ok(lines) => {
                for line in lines {
                    if let Ok(DeviceFrame::Hello {
                        fw_version,
                        protocol_version,
                        capabilities,
                    }) = parse_device_frame(&line)
                    {
                        self.data.hello = Some(HelloInfo {
                            fw_version: fw_version.clone(),
                            protocol_version,
                            capabilities: capabilities.split(',').map(String::from).collect(),
                        });
                        self.status = format!("Firmware erkannt: {fw_version}");
                        self.query_chip_id();
                        return;
                    }
                }
                self.status = "Keine HELLO-Antwort der Firmware erhalten".to_string();
            }
            Err(e) => {
                self.status = format!("FW-Abfrage fehlgeschlagen: {e}");
            }
        }
    }

    fn query_chip_id(&mut self) {
        match self.serial_send_and_read_lines("ID", 4) {
            Ok(lines) => {
                for line in lines {
                    if let Ok(DeviceFrame::Ok { command, detail }) = parse_device_frame(&line) {
                        if command != "ID" {
                            continue;
                        }

                        let mut mfr = 0u8;
                        let mut dev = 0u8;
                        let mut has_mfr = false;
                        let mut has_dev = false;

                        for kv in detail.split(',') {
                            let mut parts = kv.splitn(2, '=');
                            let key = parts.next().unwrap_or("").trim().to_lowercase();
                            let value = parts
                                .next()
                                .unwrap_or("")
                                .trim()
                                .trim_start_matches("0x")
                                .trim_start_matches("0X");
                            if let Ok(v) = u8::from_str_radix(value, 16) {
                                match key.as_str() {
                                    "mf" | "manufacturer" => {
                                        mfr = v;
                                        has_mfr = true;
                                    }
                                    "dev" | "device" => {
                                        dev = v;
                                        has_dev = true;
                                    }
                                    _ => {}
                                }
                            }
                        }

                        if has_mfr && has_dev {
                            if let Some(chip) = ChipId::from_ids(mfr, dev) {
                                self.data.chip = Some(chip.clone());
                                self.ensure_chip_buffers();
                                self.data.log.push(format!(
                                    "ID: {} (MFR=0x{:02X} DEV=0x{:02X})",
                                    chip.name, chip.manufacturer_id, chip.device_id
                                ));
                                self.status = format!("Chip erkannt: {}", chip.name);
                            } else {
                                self.ensure_fallback_chip();
                                self.data.log.push(format!(
                                    "ID unknown: MFR=0x{:02X} DEV=0x{:02X}",
                                    mfr, dev
                                ));
                                self.status =
                                    format!("Chip nicht erkannt: MFR=0x{:02X} DEV=0x{:02X} (Fallback aktiv)", mfr, dev);
                            }
                        }
                        return;
                    }
                }

                self.ensure_fallback_chip();
                self.status = "Keine verwertbare ID-Antwort erhalten (Fallback aktiv)".to_string();
            }
            Err(e) => {
                self.ensure_fallback_chip();
                self.status = format!("ID-Abfrage fehlgeschlagen: {e} (Fallback aktiv)");
            }
        }
    }

    fn send_expect_ok(&mut self, command: &str, max_lines: usize) -> Result<Vec<String>, String> {
        let lines = self.serial_send_and_read_lines(command, max_lines)?;
        for line in &lines {
            match parse_device_frame(line) {
                Ok(DeviceFrame::Ok { .. }) => return Ok(lines),
                Ok(DeviceFrame::Err { code, message }) => {
                    return Err(format!("{code}: {message}"));
                }
                _ => {}
            }
        }
        Err("no OK frame received".to_string())
    }

    fn dump_range_to_ro(&mut self, start: usize, len: usize) -> Result<(), String> {
        self.ensure_chip_buffers();
        let max_lines = (len / 16) + 64;
        let cmd = format!("READ|{start:05X}|{len}");
        let lines = self.send_expect_ok(&cmd, max_lines)?;
        let mut received = 0usize;

        for line in lines {
            let Ok(frame) = parse_device_frame(&line) else {
                continue;
            };
            if let DeviceFrame::DataHex { address, data, .. } = frame {
                let addr = address as usize;
                if addr < start || addr >= start + len {
                    continue;
                }
                let local = addr - start;
                let copy_len = data.len().min(len.saturating_sub(local));
                let dst_start = start + local;
                let dst_end = dst_start + copy_len;
                self.data.ro_data[dst_start..dst_end].copy_from_slice(&data[..copy_len]);
                for known in &mut self.data.ro_known[dst_start..dst_end] {
                    *known = true;
                }
                received += copy_len;
            }
        }

        self.rebuild_diff_report();
        self.status = format!("Fetched {received} byte(s) into Inspector area at 0x{start:05X}");
        Ok(())
    }

    fn save_work_range_to_file(&mut self, start: usize, len: usize, path: &Path) -> Result<(), String> {
        if start + len > self.data.work_data.len() {
            return Err("save range exceeds Workbench area".to_string());
        }
        fs::write(path, &self.data.work_data[start..start + len]).map_err(|e| format!("save failed: {e}"))?;
        self.status = format!("Saved {} byte(s) from Workbench to {}", len, path.display());
        Ok(())
    }

    fn sector_file_path(base: &Path, start: usize, sector_size: usize) -> PathBuf {
        let stem = base
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("rom");
        let parent = base.parent().unwrap_or_else(|| Path::new("."));
        let size_tag = if sector_size % 1024 == 0 {
            format!("{}k", sector_size / 1024)
        } else {
            format!("{sector_size}b")
        };
        parent.join(format!("{stem}_sector_{start:05X}_{size_tag}.bin"))
    }

    fn load_file_into_work(&mut self, start: usize, strict_len: Option<usize>) -> Result<(), String> {
        let path = PathBuf::from(self.file_path_input.trim());
        let bytes = fs::read(&path).map_err(|e| format!("load failed: {e}"))?;
        let expected = strict_len.unwrap_or(bytes.len());
        if let Some(exact) = strict_len {
            if bytes.len() != exact {
                return Err(format!("file size must be exactly {exact} bytes for this action"));
            }
        }
        if start + expected > self.data.work_data.len() {
            return Err("file data exceeds work area".to_string());
        }
        let copy_len = bytes.len().min(expected);
        self.data.work_data[start..start + copy_len].copy_from_slice(&bytes[..copy_len]);
        self.rebuild_diff_report();
        self.status = format!("Loaded {} byte(s) from {} into workspace", copy_len, path.display());
        Ok(())
    }

    fn copy_range_into_clipboard(
        &mut self,
        ctx: &egui::Context,
        start: usize,
        len: usize,
        source: Pane,
    ) -> Result<(), String> {
        let src = match source {
            Pane::Inspector => &self.data.ro_data,
            Pane::Workspace => &self.data.work_data,
        };
        if start + len > src.len() {
            return Err("copy range exceeds source".to_string());
        }
        self.clipboard = src[start..start + len].to_vec();
        let src_name = match source {
            Pane::Inspector => "Inspector",
            Pane::Workspace => "Workspace",
        };
        ctx.output_mut(|output| {
            output.copied_text = Self::clipboard_hex(&self.clipboard);
        });
        self.clipboard_desc = format!("{src_name} 0x{start:05X} +{len}");
        self.status = format!("Copied {len} byte(s) from {src_name} into system clipboard");
        Ok(())
    }

    fn copy_ro_into_work(&mut self, start: usize, len: usize) -> Result<(), String> {
        if start + len > self.data.ro_data.len() || start + len > self.data.work_data.len() {
            return Err("copy range exceeds buffer bounds".to_string());
        }
        self.data.work_data[start..start + len].copy_from_slice(&self.data.ro_data[start..start + len]);
        self.rebuild_diff_report();
        self.status = format!("Copied {len} byte(s) from Inspector into workspace at 0x{start:05X}");
        Ok(())
    }

    fn load_rgba_image(bytes: &[u8], label: &str) -> Result<image::RgbaImage, String> {
        image::load_from_memory(bytes)
            .map_err(|err| format!("Failed to load {label}: {err}"))
            .map(|img| img.to_rgba8())
    }

    fn ensure_icon_assets_loaded(&mut self) -> Result<(), String> {
        if self.icon_assets.is_some() {
            return Ok(());
        }

        let mut base = HashMap::new();
        let mut overlays = HashMap::new();
        let mut arrows = HashMap::new();

        base.insert(
            BaseIcon::Disk,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/base/disk.png"), "base/disk.png")?,
        );
        base.insert(
            BaseIcon::Trash,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/base/trash.png"), "base/trash.png")?,
        );
        base.insert(
            BaseIcon::Workbench,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/base/workbench.png"), "base/workbench.png")?,
        );
        base.insert(
            BaseIcon::Chip,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/base/chip.png"), "base/chip.png")?,
        );
        base.insert(
            BaseIcon::Inspector,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/base/monitor.png"), "base/monitor.png")?,
        );

        overlays.insert(
            OverlayIcon::Image,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/overlays/image.png"), "overlays/image.png")?,
        );
        overlays.insert(
            OverlayIcon::Sector,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/overlays/sector.png"), "overlays/sector.png")?,
        );
        overlays.insert(
            OverlayIcon::Range,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/overlays/range.png"), "overlays/range.png")?,
        );

        arrows.insert(
            ArrowIcon::Erase,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/arrows/erase.png"), "arrows/erase.png")?,
        );
        arrows.insert(
            ArrowIcon::Fetch,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/arrows/fetch.png"), "arrows/fetch.png")?,
        );
        arrows.insert(
            ArrowIcon::Flash,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/arrows/flash.png"), "arrows/flash.png")?,
        );
        arrows.insert(
            ArrowIcon::Copy,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/arrows/copy.png"), "arrows/copy.png")?,
        );
        arrows.insert(
            ArrowIcon::Load,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/arrows/load.png"), "arrows/load.png")?,
        );
        arrows.insert(
            ArrowIcon::Save,
            Self::load_rgba_image(include_bytes!("../../Resources/Assets/Buttons/arrows/save.png"), "arrows/save.png")?,
        );

        self.icon_assets = Some(IconAssets {
            base,
            overlays,
            arrows,
            composites: HashMap::new(),
        });
        Ok(())
    }

    fn validate_tile_size(tile: &image::RgbaImage, label: &str) -> Result<(), String> {
        if tile.width() != 40 || tile.height() != 40 {
            return Err(format!("{label} must be exactly 40x40 px"));
        }
        Ok(())
    }

    fn blend_over(dst: &mut image::RgbaImage, src: &image::RgbaImage, x_off: u32, y_off: u32) {
        for y in 0..src.height() {
            for x in 0..src.width() {
                let top = src.get_pixel(x, y).0;
                let alpha = top[3] as f32 / 255.0;
                if alpha <= 0.0 {
                    continue;
                }
                let dst_px = dst.get_pixel_mut(x + x_off, y + y_off);
                let mut bottom = dst_px.0;
                for i in 0..3 {
                    let mixed = (top[i] as f32 * alpha) + (bottom[i] as f32 * (1.0 - alpha));
                    bottom[i] = mixed.round().clamp(0.0, 255.0) as u8;
                }
                bottom[3] = 255;
                *dst_px = image::Rgba(bottom);
            }
        }
    }

    fn texture_for_visual(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        spec: ButtonVisualSpec,
    ) -> Result<egui::TextureHandle, String> {
        self.ensure_icon_assets_loaded()?;
        let assets = self.icon_assets.as_mut().expect("icon assets initialized");

        if let Some(handle) = assets.composites.get(key) {
            return Ok(handle.clone());
        }

        let left_base = assets
            .base
            .get(&spec.left_base)
            .ok_or_else(|| "missing left base icon".to_string())?;
        let right_base = assets
            .base
            .get(&spec.right_base)
            .ok_or_else(|| "missing right base icon".to_string())?;
        let arrow = assets
            .arrows
            .get(&spec.arrow)
            .ok_or_else(|| "missing arrow icon".to_string())?;

        Self::validate_tile_size(left_base, "left base")?;
        Self::validate_tile_size(right_base, "right base")?;
        Self::validate_tile_size(arrow, "arrow")?;

        let mut canvas = image::RgbaImage::new(120, 40);
        Self::blend_over(&mut canvas, left_base, 0, 0);
        Self::blend_over(&mut canvas, arrow, 40, 0);
        Self::blend_over(&mut canvas, right_base, 80, 0);

        if let Some(left_overlay_kind) = spec.left_overlay {
            let left_overlay = assets
                .overlays
                .get(&left_overlay_kind)
                .ok_or_else(|| "missing left overlay icon".to_string())?;
            Self::validate_tile_size(left_overlay, "left overlay")?;
            Self::blend_over(&mut canvas, left_overlay, 0, 0);
        }

        if let Some(right_overlay_kind) = spec.right_overlay {
            let right_overlay = assets
                .overlays
                .get(&right_overlay_kind)
                .ok_or_else(|| "missing right overlay icon".to_string())?;
            Self::validate_tile_size(right_overlay, "right overlay")?;
            Self::blend_over(&mut canvas, right_overlay, 80, 0);
        }

        let pixels = canvas.into_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied([120, 40], &pixels);
        let handle = ctx.load_texture(
            format!("flashbang_button_{key}"),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        assets.composites.insert(key.to_string(), handle.clone());
        Ok(handle)
    }

    fn icon_button(ui: &mut egui::Ui, texture: &egui::TextureHandle, tooltip: &str) -> egui::Response {
        let image = egui::Image::new((texture.id(), egui::vec2(120.0, 40.0)));
        ui.add_sized([120.0, 40.0], egui::ImageButton::new(image).frame(false))
            .on_hover_text(tooltip)
    }

    fn operation_button(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        key: &str,
        spec: ButtonVisualSpec,
        tooltip: &str,
    ) -> egui::Response {
        match self.texture_for_visual(ctx, key, spec) {
            Ok(texture) => Self::icon_button(ui, &texture, tooltip),
            Err(err) => {
                self.status = err;
                ui.add_enabled(false, egui::Button::new(tooltip))
            }
        }
    }

    fn flash_range_from_work(&mut self, start: usize, len: usize) -> Result<(), String> {
        if self.serial_handle.is_none() {
            return Err("not connected".to_string());
        }
        if start + len > self.data.work_data.len() {
            return Err("flash range exceeds workspace".to_string());
        }

        let mut has_gray = false;
        let mut red_sector: Option<usize> = None;
        let sector_size = self.sector_size().unwrap_or(4096);

        for addr in start..start + len {
            match self.byte_state(addr) {
                ByteState::Gray => has_gray = true,
                ByteState::Red => {
                    red_sector = Some(addr / sector_size);
                    break;
                }
                _ => {}
            }
        }

        if let Some(sector) = red_sector {
            return Err(format!(
                "flash refused: sector {sector} needs erase (red bytes present)"
            ));
        }

        if has_gray && !self.allow_flash_gray {
            return Err(
                "flash warning: target includes gray (stale) bytes. Fetch first or enable 'Allow Flash on gray'."
                    .to_string(),
            );
        }

        let mut flashed = 0usize;
        for addr in start..start + len {
            let work = self.data.work_data[addr];
            let known = self.data.ro_known[addr];
            let ro = self.data.ro_data[addr];
            if known && ro == work {
                continue;
            }
            let cmd = format!("PROGRAM_BYTE|{addr:05X}|{work:02X}");
            self.send_expect_ok(&cmd, 6)?;
            flashed += 1;
        }

        self.mark_ro_unknown(start, len);
        self.status = format!("Flashed {flashed} byte(s). Inspector marked stale/gray in affected range.");
        Ok(())
    }

    fn erase_sector(&mut self, start: usize) -> Result<(), String> {
        let cmd = format!("SECTOR_ERASE|{start:05X}");
        self.send_expect_ok(&cmd, 6)?;
        let sector_size = self.sector_size().unwrap_or(4096);
        self.mark_ro_unknown(start, sector_size);
        self.status = format!("Erased sector at 0x{start:05X}. Inspector marked stale/gray.");
        Ok(())
    }

    fn erase_chip(&mut self) -> Result<(), String> {
        self.send_expect_ok("CHIP_ERASE", 6)?;
        let chip_size = self.chip_size().unwrap_or(0);
        self.mark_ro_unknown(0, chip_size);
        self.status = "Chip erased. Entire Inspector view marked stale/gray.".to_string();
        Ok(())
    }

    fn byte_color_for_ro(&self, addr: usize) -> egui::Color32 {
        match self.color_mode {
            ColorMode::Diff => Self::diff_color_for_state(self.byte_state(addr)),
            ColorMode::Palette => Self::palette_color(self.data.ro_data[addr]),
        }
    }

    fn byte_color_for_work(&self, addr: usize) -> egui::Color32 {
        match self.color_mode {
            ColorMode::Diff => {
                if self.data.ro_known.get(addr).copied().unwrap_or(false)
                    && self.data.ro_data[addr] == self.data.work_data[addr]
                {
                    egui::Color32::from_gray(220)
                } else {
                    egui::Color32::from_rgb(190, 230, 255)
                }
            }
            ColorMode::Palette => Self::palette_color(self.data.work_data[addr]),
        }
    }

    fn paint_outlined_cell_text(
        ui: &egui::Ui,
        rect: egui::Rect,
        text: &str,
        fill_color: egui::Color32,
        selected: bool,
    ) {
        let font_id = egui::TextStyle::Monospace.resolve(ui.style());
        let center = rect.center();

        let outline_color = if selected {
            egui::Color32::BLACK
        } else {
            egui::Color32::from_rgb(20, 20, 20)
        };

        let offsets = [
            egui::vec2(-1.0, 0.0),
            egui::vec2(1.0, 0.0),
            egui::vec2(0.0, -1.0),
            egui::vec2(0.0, 1.0),
        ];
        for offset in offsets {
            ui.painter().text(
                center + offset,
                egui::Align2::CENTER_CENTER,
                text,
                font_id.clone(),
                outline_color,
            );
        }

        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            text,
            font_id,
            fill_color,
        );
    }

    fn draw_byte_grid(&mut self, ui: &mut egui::Ui, pane: Pane, id_suffix: &str) {
        let Some(chip_size) = self.chip_size() else {
            ui.label("No chip identified.");
            return;
        };

        const BYTES_PER_ROW: usize = 16;
        let total_rows = chip_size / BYTES_PER_ROW;
        let sector_size = self.sector_size().unwrap_or(4096);
        let active_sector_from_input = Self::parse_int_input(&self.sector_input)
            .ok()
            .map(|v| v as usize);
        let selected_range = self
            .parse_range_input()
            .ok()
            .map(|(start, len)| (start, start + len - 1));

        let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 2.0;
        let byte_cell_width = match self.character_mode {
            CharacterMode::Hex => 20.0,
            CharacterMode::Ascii => 14.0,
        };
        let old_item_spacing = ui.spacing().item_spacing;
        let old_button_padding = ui.spacing().button_padding;
        let old_interact_size = ui.spacing().interact_size;
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
        ui.spacing_mut().interact_size = egui::vec2(byte_cell_width, row_height);

        egui::ScrollArea::both()
            .id_source(id_suffix)
            .show_rows(ui, row_height, total_rows, |ui, row_range| {
                egui::Grid::new(format!("hex_grid_{id_suffix}"))
                    .striped(true)
                    .show(ui, |ui| {
                        for row in row_range {
                            let offset = row * BYTES_PER_ROW;
                            if self.show_sector_boundaries && offset % sector_size == 0 {
                                let sector_idx = offset / sector_size;
                                let is_active_sector = active_sector_from_input == Some(sector_idx);
                                let sector_label = if is_active_sector {
                                    format!(">S{:03}", sector_idx)
                                } else {
                                    format!("S{:03}", sector_idx)
                                };
                                let sector_color = if is_active_sector {
                                    egui::Color32::from_rgb(96, 208, 255)
                                } else {
                                    egui::Color32::from_rgb(255, 230, 120)
                                };
                                ui.add_sized(
                                    [34.0, row_height],
                                    egui::Label::new(
                                        egui::RichText::new(sector_label)
                                            .color(sector_color)
                                            .monospace(),
                                    ),
                                );
                            } else {
                                ui.add_sized([34.0, row_height], egui::Label::new("   "));
                            }
                            ui.add_sized(
                                [52.0, row_height],
                                egui::Label::new(egui::RichText::new(format!("{offset:05X}")).monospace()),
                            );

                            for col in 0..BYTES_PER_ROW {
                                let addr = offset + col;
                                if addr >= self.data.work_data.len() {
                                    ui.label("  ");
                                    continue;
                                }

                                let byte = match pane {
                                    Pane::Inspector => self.data.ro_data[addr],
                                    Pane::Workspace => self.data.work_data[addr],
                                };

                                let color = match pane {
                                    Pane::Inspector => self.byte_color_for_ro(addr),
                                    Pane::Workspace => self.byte_color_for_work(addr),
                                };

                                let selected = match pane {
                                    Pane::Inspector => self.selected_ro_addr == Some(addr),
                                    Pane::Workspace => self.selected_work_addr == Some(addr),
                                };
                                let in_selected_range = selected_range
                                    .map(|(start, end)| addr >= start && addr <= end)
                                    .unwrap_or(false);

                                let mut text = self.display_text_for_byte(byte);
                                if self.character_mode == CharacterMode::Ascii && text.is_empty() {
                                    text.push(' ');
                                }
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(byte_cell_width, row_height),
                                    egui::Sense::click(),
                                );

                                // Always use RRRGGGBB value mapping as base cell background.
                                ui.painter().rect_filled(rect, 0.0, Self::palette_color(byte));

                                if selected {
                                    ui.painter().rect_stroke(
                                        rect.shrink(0.5),
                                        0.0,
                                        egui::Stroke::new(1.6, egui::Color32::from_rgb(255, 48, 48)),
                                    );
                                } else if response.hovered() {
                                    ui.painter().rect_stroke(
                                        rect.shrink(0.5),
                                        0.0,
                                        egui::Stroke::new(1.4, egui::Color32::from_rgb(96, 208, 255)),
                                    );
                                } else if in_selected_range {
                                    ui.painter().rect_stroke(
                                        rect.shrink(0.5),
                                        0.0,
                                        egui::Stroke::new(1.2, egui::Color32::from_rgb(255, 210, 64)),
                                    );
                                }
                                Self::paint_outlined_cell_text(ui, rect, &text, color, selected);
                                if response.clicked() {
                                    let shift_pressed = ui.ctx().input(|i| i.modifiers.shift);
                                    let anchor = match pane {
                                        Pane::Inspector => self.selected_ro_addr,
                                        Pane::Workspace => self.selected_work_addr,
                                    };

                                    if shift_pressed {
                                        if let Some(start_anchor) = anchor {
                                            let start = start_anchor.min(addr);
                                            let end = start_anchor.max(addr);
                                            let len = end - start + 1;
                                            self.range_start_input = format!("{start:05X}");
                                            self.range_len_input = len.to_string();
                                            self.status =
                                                format!("Range selected: 0x{start:05X}..0x{end:05X} ({len} byte(s))");
                                        }
                                    }

                                    match pane {
                                        Pane::Inspector => {
                                            self.selected_ro_addr = Some(addr);
                                            self.active_pane = Pane::Inspector;
                                        }
                                        Pane::Workspace => {
                                            self.selected_work_addr = Some(addr);
                                            self.active_pane = Pane::Workspace;
                                        }
                                    }
                                    self.sector_input = (addr / sector_size).to_string();
                                    self.pending_hex_high_nibble = None;
                                }
                            }
                            ui.end_row();
                        }
                    });
            });

                ui.spacing_mut().item_spacing = old_item_spacing;
                ui.spacing_mut().button_padding = old_button_padding;
                ui.spacing_mut().interact_size = old_interact_size;
    }
}

impl eframe::App for FlashBangGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut do_refresh = false;
        let mut do_connect = false;
        let mut do_disconnect = false;
        let mut do_query_fw = false;

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("FlashBang Studio");
                ui.separator();
                ui.label("Desktop GUI Preview");
                ui.separator();
                ui.monospace(version::version_text());
                ui.separator();
                if ui.button("About").clicked() {
                    self.show_about = true;
                }
                ui.separator();
                let mut status_line = format!("Status: {}", self.status);
                if let Some(chip_status) = self.chip_status_text() {
                    status_line.push_str(" | ");
                    status_line.push_str(&chip_status);
                }
                ui.label(status_line);
            });

            ui.horizontal(|ui| {
                ui.add_enabled_ui(self.serial_handle.is_none(), |ui| {
                    ui.label("Serial Port:");
                    let selected_name = self
                        .available_ports
                        .get(self.selected_port_index)
                        .map(|p| p.name.as_str())
                        .unwrap_or("<none>");

                    egui::ComboBox::from_id_source("serial_port_combo")
                        .selected_text(selected_name)
                        .width(260.0)
                        .show_ui(ui, |ui| {
                            for (idx, p) in self.available_ports.iter().enumerate() {
                                let label = if p.description.is_empty() {
                                    p.name.clone()
                                } else {
                                    format!("{} ({})", p.name, p.description)
                                };
                                ui.selectable_value(&mut self.selected_port_index, idx, label);
                            }
                        });

                    ui.label("Baud:");
                    ui.add(
                        egui::DragValue::new(&mut self.baud_rate).clamp_range(1200..=3_000_000),
                    );

                    if ui.button("Refresh Ports").clicked() {
                        do_refresh = true;
                    }
                });

                if self.serial_handle.is_some() {
                    if ui.button("Firmware abfragen").clicked() {
                        do_query_fw = true;
                    }
                    if ui.button("Disconnect").clicked() {
                        do_disconnect = true;
                    }
                } else if ui.button("Connect").clicked() {
                    do_connect = true;
                }

                match &self.connected_port_name {
                    Some(name) => ui.colored_label(egui::Color32::LIGHT_GREEN, format!("Connected: {name}")),
                    None => ui.colored_label(egui::Color32::YELLOW, "Not connected"),
                };
            });
        });

        if do_refresh {
            self.refresh_ports();
        }

        if do_disconnect {
            self.serial_handle = None;
            self.connected_port_name = None;
            self.status = "Serial port disconnected".to_string();
        }

        if do_connect {
            let selected_port = self.available_ports.get(self.selected_port_index).cloned();
            if let Some(port) = selected_port {
                match open_serial_port(&port.name, self.baud_rate, 300) {
                    Ok(handle) => {
                        self.push_wire(
                            WireDirection::Tx,
                            format!("<open {} @ {}>", port.name, self.baud_rate),
                        );
                        self.serial_handle = Some(handle);
                        self.connected_port_name = Some(port.name.clone());
                        self.status = format!("Connected to {} @ {} baud", port.name, self.baud_rate);
                        do_query_fw = true;
                    }
                    Err(e) => {
                        self.serial_handle = None;
                        self.connected_port_name = None;
                        self.status = format!("Connect failed: {e}");
                    }
                }
            } else {
                self.status = "No serial port selected".to_string();
            }
        }

        if do_query_fw {
            self.query_firmware_version();
        }

        self.handle_workspace_typing(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            const SPLITTER_HEIGHT: f32 = 6.0;
            const MIN_UPPER_HEIGHT: f32 = 140.0;
            const MIN_SERIAL_HEIGHT: f32 = 100.0;

            let avail = ui.available_size();
            let total_height = avail.y.max(0.0);

            let max_upper = (total_height - MIN_SERIAL_HEIGHT - SPLITTER_HEIGHT)
                .max(MIN_UPPER_HEIGHT)
                .min(total_height);
            let min_upper = MIN_UPPER_HEIGHT.min(max_upper);

            let mut upper_height = (total_height * self.upper_area_ratio).clamp(min_upper, max_upper);

            let upper_size = egui::vec2(avail.x, upper_height.max(0.0));
            ui.allocate_ui_with_layout(
                upper_size,
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.draw_hex_dump(ui),
            );

            let (splitter_rect, splitter_response) =
                ui.allocate_exact_size(egui::vec2(avail.x, SPLITTER_HEIGHT), egui::Sense::click_and_drag());
            let stroke_color = if splitter_response.dragged() || splitter_response.hovered() {
                egui::Color32::from_rgb(140, 180, 220)
            } else {
                egui::Color32::from_gray(110)
            };
            ui.painter().rect_filled(splitter_rect, 0.0, egui::Color32::from_gray(45));
            ui.painter().line_segment(
                [splitter_rect.left_center(), splitter_rect.right_center()],
                egui::Stroke::new(2.0, stroke_color),
            );

            if splitter_response.hovered() || splitter_response.dragged() {
                ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::ResizeVertical);
            }

            if splitter_response.dragged() {
                let delta_y = ui.ctx().input(|i| i.pointer.delta().y);
                upper_height = (upper_height + delta_y).clamp(min_upper, max_upper);
                if total_height > 0.0 {
                    self.upper_area_ratio = (upper_height / total_height).clamp(0.0, 1.0);
                }
            }

            let serial_height = (total_height - upper_height - SPLITTER_HEIGHT).max(0.0);
            ui.allocate_ui_with_layout(
                egui::vec2(avail.x, serial_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.draw_serial_monitor(ui),
            );
        });

        if self.show_about {
            egui::Window::new("About FlashBang Studio")
                .open(&mut self.show_about)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.heading("FlashBang Studio");
                    ui.label("SST39 programmer host GUI for BluePill-based hardware.");
                    ui.label("Current mode: GUI + serial debug monitor + mock fallback.");
                    ui.label("Build target: Rust/Cargo 1.75 compatible stack.");
                    ui.separator();
                    ui.monospace(format!("Version: {}", version::version_text()));
                    ui.monospace(format!("Tag: {}", version::version_tag()));
                    ui.monospace(format!("Build: {}", version::build_number()));
                    ui.monospace(format!("Git: {}", version::git_sha()));
                    ui.monospace(format!("Dirty: {}", version::is_dirty()));
                });
        }
    }
}

impl FlashBangGuiApp {
    fn draw_hex_dump(&mut self, ui: &mut egui::Ui) {
        self.ensure_chip_buffers();
        if self.data.work_data.is_empty() {
            ui.label("No chip buffer allocated yet. Connect and query chip ID first.");
            return;
        }

        ui.horizontal_wrapped(|ui| {
            ui.label("Color Mode:");
            ui.selectable_value(&mut self.color_mode, ColorMode::Diff, "Diff");
            ui.selectable_value(&mut self.color_mode, ColorMode::Palette, "Palette");
            ui.separator();
            ui.label("Character Mode:");
            ui.selectable_value(&mut self.character_mode, CharacterMode::Hex, "Hex");
            ui.selectable_value(&mut self.character_mode, CharacterMode::Ascii, "ASCII (Latin-15)");
            ui.separator();
            ui.checkbox(&mut self.show_sector_boundaries, "Show Sector Boundaries");
            ui.checkbox(&mut self.allow_flash_gray, "Allow Flash on gray");
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Range Start:");
            ui.add(egui::TextEdit::singleline(&mut self.range_start_input).desired_width(58.0));
            ui.label("Len:");
            ui.add(egui::TextEdit::singleline(&mut self.range_len_input).desired_width(58.0));
            ui.label("Sector:");
            ui.add(egui::TextEdit::singleline(&mut self.sector_input).desired_width(40.0));
            ui.separator();
            ui.label("File:");
            ui.add(egui::TextEdit::singleline(&mut self.file_path_input).desired_width(260.0));
            ui.separator();
            ui.label(format!("Clipboard: {}", self.clipboard_desc));
            ui.label("Ctrl+C copies active range, Ctrl+V pastes at workspace cursor");
        });

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Inspector cursor:");
            ui.monospace(
                self.selected_ro_addr
                    .map(|a| format!("0x{a:05X}"))
                    .unwrap_or_else(|| "-".to_string()),
            );
            ui.separator();
            ui.label("Work cursor:");
            ui.monospace(
                self.selected_work_addr
                    .map(|a| format!("0x{a:05X}"))
                    .unwrap_or_else(|| "-".to_string()),
            );
            if self.character_mode == CharacterMode::Hex {
                ui.separator();
                ui.label("Hex nibble:");
                ui.monospace(match self.pending_hex_high_nibble {
                    Some(v) => format!("0x{v:02X}"),
                    None => "-".to_string(),
                });
            }
        });

        ui.separator();

        let available_width = ui.available_width();
        let available_height = ui.available_height();
        let spacing_x = ui.spacing().item_spacing.x;
        const TRANSFER_BUTTON_WIDTH: f32 = 120.0;
        const TRANSFER_COL_PADDING_X: f32 = 12.0;
        const PANEL_GAP_Y: f32 = 6.0;

        let ideal_transfer_width = TRANSFER_BUTTON_WIDTH + TRANSFER_COL_PADDING_X;
        let transfer_col_width = ideal_transfer_width
            .min((available_width - spacing_x * 2.0).max(TRANSFER_BUTTON_WIDTH));
        let remaining_width = (available_width - transfer_col_width - spacing_x * 2.0).max(0.0);
        let side_width = remaining_width * 0.5;
        let top_height = (available_height * 0.75).max(120.0);
        let lower_height = (available_height - top_height - PANEL_GAP_Y).max(90.0);
        let ctx = ui.ctx().clone();

        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(side_width, available_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(side_width, top_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.group(|ui| {
                                self.draw_byte_grid(ui, Pane::Inspector, "inspector");
                            });
                        },
                    );
                    ui.add_space(PANEL_GAP_Y);
                    ui.allocate_ui_with_layout(
                        egui::vec2(side_width, lower_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.group(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    if self.operation_button(
                                        ui,
                                        &ctx,
                                        "fetch_image",
                                        ButtonVisualSpec {
                                            left_base: BaseIcon::Chip,
                                            left_overlay: None,
                                            arrow: ArrowIcon::Fetch,
                                            right_overlay: None,
                                            right_base: BaseIcon::Inspector,
                                        },
                                        "Fetch Image (Chip -> Inspector)",
                                    ).clicked() {
                                        if let Some(size) = self.chip_size() {
                                            if let Err(e) = self.dump_range_to_ro(0, size) {
                                                self.status = format!("Fetch image failed: {e}");
                                            }
                                        }
                                    }
                                    if self.operation_button(
                                        ui,
                                        &ctx,
                                        "fetch_range",
                                        ButtonVisualSpec {
                                            left_base: BaseIcon::Chip,
                                            left_overlay: Some(OverlayIcon::Range),
                                            arrow: ArrowIcon::Fetch,
                                            right_overlay: Some(OverlayIcon::Range),
                                            right_base: BaseIcon::Inspector,
                                        },
                                        "Fetch Range (Chip+R -> Inspector+R)",
                                    ).clicked() {
                                        match self.parse_range_input() {
                                            Ok((start, len)) => {
                                                if let Err(e) = self.dump_range_to_ro(start, len) {
                                                    self.status = format!("Fetch range failed: {e}");
                                                }
                                            }
                                            Err(e) => self.status = format!("Invalid range: {e}"),
                                        }
                                    }
                                    if self.operation_button(
                                        ui,
                                        &ctx,
                                        "fetch_sector",
                                        ButtonVisualSpec {
                                            left_base: BaseIcon::Chip,
                                            left_overlay: Some(OverlayIcon::Sector),
                                            arrow: ArrowIcon::Fetch,
                                            right_overlay: Some(OverlayIcon::Sector),
                                            right_base: BaseIcon::Inspector,
                                        },
                                        "Fetch Sector (Chip+S -> Inspector+S)",
                                    ).clicked() {
                                        match self.parse_sector_input() {
                                            Ok((_idx, start, size)) => {
                                                if let Err(e) = self.dump_range_to_ro(start, size) {
                                                    self.status = format!("Fetch sector failed: {e}");
                                                }
                                            }
                                            Err(e) => self.status = format!("Invalid sector: {e}"),
                                        }
                                    }
                                    if self.operation_button(
                                        ui,
                                        &ctx,
                                        "erase_image",
                                        ButtonVisualSpec {
                                            left_base: BaseIcon::Chip,
                                            left_overlay: None,
                                            arrow: ArrowIcon::Erase,
                                            right_overlay: None,
                                            right_base: BaseIcon::Trash,
                                        },
                                        "Erase Image (Chip -> Trash)",
                                    ).clicked() {
                                        if let Err(e) = self.erase_chip() {
                                            self.status = format!("Erase all failed: {e}");
                                        }
                                    }
                                    if self.operation_button(
                                        ui,
                                        &ctx,
                                        "erase_sector",
                                        ButtonVisualSpec {
                                            left_base: BaseIcon::Chip,
                                            left_overlay: Some(OverlayIcon::Sector),
                                            arrow: ArrowIcon::Erase,
                                            right_overlay: None,
                                            right_base: BaseIcon::Trash,
                                        },
                                        "Erase Sector (Chip+S -> Trash)",
                                    ).clicked() {
                                        match self.parse_sector_input() {
                                            Ok((_idx, start, _size)) => {
                                                if let Err(e) = self.erase_sector(start) {
                                                    self.status = format!("Erase sector failed: {e}");
                                                }
                                            }
                                            Err(e) => self.status = format!("Invalid sector: {e}"),
                                        }
                                    }
                                });
                            });
                        },
                    );
                },
            );

            ui.allocate_ui_with_layout(
                egui::vec2(transfer_col_width, available_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.group(|ui| {
                        egui::ScrollArea::vertical()
                            .id_source("btn_col_scroll")
                            .show(ui, |ui| {
                                if self.operation_button(
                                    ui,
                                    &ctx,
                                    "copy_image",
                                    ButtonVisualSpec {
                                        left_base: BaseIcon::Inspector,
                                        left_overlay: None,
                                        arrow: ArrowIcon::Copy,
                                        right_overlay: None,
                                        right_base: BaseIcon::Workbench,
                                    },
                                    "Copy Image (Inspector -> Workbench)",
                                ).clicked() {
                                    if let Some(size) = self.chip_size() {
                                        if let Err(e) = self.copy_ro_into_work(0, size) {
                                            self.status = format!("Copy all failed: {e}");
                                        }
                                    }
                                }
                                if self.operation_button(
                                    ui,
                                    &ctx,
                                    "copy_sector",
                                    ButtonVisualSpec {
                                        left_base: BaseIcon::Inspector,
                                        left_overlay: Some(OverlayIcon::Sector),
                                        arrow: ArrowIcon::Copy,
                                        right_overlay: Some(OverlayIcon::Sector),
                                        right_base: BaseIcon::Workbench,
                                    },
                                    "Copy Sector (Inspector+S -> Workbench+S)",
                                ).clicked() {
                                    match self.parse_sector_input() {
                                        Ok((_idx, start, size)) => {
                                            if let Err(e) = self.copy_ro_into_work(start, size) {
                                                self.status = format!("Copy sector failed: {e}");
                                            }
                                        }
                                        Err(e) => self.status = format!("Invalid sector: {e}"),
                                    }
                                }
                                if self.operation_button(
                                    ui,
                                    &ctx,
                                    "copy_range",
                                    ButtonVisualSpec {
                                        left_base: BaseIcon::Inspector,
                                        left_overlay: Some(OverlayIcon::Range),
                                        arrow: ArrowIcon::Copy,
                                        right_overlay: Some(OverlayIcon::Range),
                                        right_base: BaseIcon::Workbench,
                                    },
                                    "Copy Range (Inspector+R -> Workbench+R)",
                                ).clicked() {
                                    match self.parse_range_input() {
                                        Ok((start, len)) => {
                                            if let Err(e) = self.copy_ro_into_work(start, len) {
                                                self.status = format!("Copy range failed: {e}");
                                            }
                                        }
                                        Err(e) => self.status = format!("Invalid range: {e}"),
                                    }
                                }

                                ui.separator();

                                if self.operation_button(
                                    ui,
                                    &ctx,
                                    "flash_image",
                                    ButtonVisualSpec {
                                        left_base: BaseIcon::Chip,
                                        left_overlay: None,
                                        arrow: ArrowIcon::Flash,
                                        right_overlay: None,
                                        right_base: BaseIcon::Workbench,
                                    },
                                    "Flash Image (Chip <- Workbench)",
                                ).clicked() {
                                    if let Some(size) = self.chip_size() {
                                        if let Err(e) = self.flash_range_from_work(0, size) {
                                            self.status = format!("Flash all failed: {e}");
                                        }
                                    }
                                }
                                if self.operation_button(
                                    ui,
                                    &ctx,
                                    "flash_sector",
                                    ButtonVisualSpec {
                                        left_base: BaseIcon::Chip,
                                        left_overlay: Some(OverlayIcon::Sector),
                                        arrow: ArrowIcon::Flash,
                                        right_overlay: Some(OverlayIcon::Sector),
                                        right_base: BaseIcon::Workbench,
                                    },
                                    "Flash Sector (Chip+S <- Workbench+S)",
                                ).clicked() {
                                    match self.parse_sector_input() {
                                        Ok((_idx, start, size)) => {
                                            if let Err(e) = self.flash_range_from_work(start, size) {
                                                self.status = format!("Flash sector failed: {e}");
                                            }
                                        }
                                        Err(e) => self.status = format!("Invalid sector: {e}"),
                                    }
                                }
                                if self.operation_button(
                                    ui,
                                    &ctx,
                                    "flash_range",
                                    ButtonVisualSpec {
                                        left_base: BaseIcon::Chip,
                                        left_overlay: Some(OverlayIcon::Range),
                                        arrow: ArrowIcon::Flash,
                                        right_overlay: Some(OverlayIcon::Range),
                                        right_base: BaseIcon::Workbench,
                                    },
                                    "Flash Range (Chip+R <- Workbench+R)",
                                ).clicked() {
                                    match self.parse_range_input() {
                                        Ok((start, len)) => {
                                            if let Err(e) = self.flash_range_from_work(start, len) {
                                                self.status = format!("Flash range failed: {e}");
                                            }
                                        }
                                        Err(e) => self.status = format!("Invalid range: {e}"),
                                    }
                                }
                            });
                    });
                },
            );

            ui.allocate_ui_with_layout(
                egui::vec2(side_width, available_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(side_width, top_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.group(|ui| {
                                self.draw_byte_grid(ui, Pane::Workspace, "work");
                            });
                        },
                    );
                    ui.add_space(PANEL_GAP_Y);
                    ui.allocate_ui_with_layout(
                        egui::vec2(side_width, lower_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.group(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    if self.operation_button(
                                        ui,
                                        &ctx,
                                        "load_image",
                                        ButtonVisualSpec {
                                            left_base: BaseIcon::Disk,
                                            left_overlay: None,
                                            arrow: ArrowIcon::Load,
                                            right_overlay: None,
                                            right_base: BaseIcon::Workbench,
                                        },
                                        "Load Image (Disk -> Workbench)",
                                    ).clicked() {
                                        if self.choose_open_file() {
                                            if let Err(e) = self.load_file_into_work(0, None) {
                                                self.status = e;
                                            }
                                        } else {
                                            self.status = "Load cancelled".to_string();
                                        }
                                    }
                                    if self.operation_button(
                                        ui,
                                        &ctx,
                                        "load_sector",
                                        ButtonVisualSpec {
                                            left_base: BaseIcon::Disk,
                                            left_overlay: Some(OverlayIcon::Sector),
                                            arrow: ArrowIcon::Load,
                                            right_overlay: Some(OverlayIcon::Sector),
                                            right_base: BaseIcon::Workbench,
                                        },
                                        "Load Sector (Disk+S -> Workbench+S)",
                                    ).clicked() {
                                        if self.choose_open_file() {
                                            match self.parse_sector_input() {
                                                Ok((_idx, start, size)) => {
                                                    if let Err(e) = self.load_file_into_work(start, Some(size)) {
                                                        self.status = e;
                                                    }
                                                }
                                                Err(e) => self.status = format!("Invalid sector: {e}"),
                                            }
                                        } else {
                                            self.status = "Load cancelled".to_string();
                                        }
                                    }
                                    if self.operation_button(
                                        ui,
                                        &ctx,
                                        "save_image",
                                        ButtonVisualSpec {
                                            left_base: BaseIcon::Workbench,
                                            left_overlay: None,
                                            arrow: ArrowIcon::Save,
                                            right_overlay: None,
                                            right_base: BaseIcon::Disk,
                                        },
                                        "Save Image (Workbench -> Disk)",
                                    ).clicked() {
                                        if self.file_path_input.trim().is_empty() && !self.choose_save_file("rom_inspector.bin") {
                                            self.status = "Save cancelled".to_string();
                                        } else {
                                            let path = PathBuf::from(self.file_path_input.trim());
                                            if !path.as_os_str().is_empty() {
                                                if let Some(size) = self.chip_size() {
                                                    if let Err(e) = self.save_work_range_to_file(0, size, &path) {
                                                        self.status = e;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if self.operation_button(
                                        ui,
                                        &ctx,
                                        "save_sector",
                                        ButtonVisualSpec {
                                            left_base: BaseIcon::Workbench,
                                            left_overlay: Some(OverlayIcon::Sector),
                                            arrow: ArrowIcon::Save,
                                            right_overlay: Some(OverlayIcon::Sector),
                                            right_base: BaseIcon::Disk,
                                        },
                                        "Save Sector (Workbench+S -> Disk+S)",
                                    ).clicked() {
                                        match self.parse_sector_input() {
                                            Ok((_idx, start, size)) => {
                                                let previous = self.file_path_input.clone();
                                                let suggested =
                                                    Self::sector_file_path(Path::new(self.file_path_input.trim()), start, size);
                                                let suggested_name = suggested
                                                    .file_name()
                                                    .and_then(|name| name.to_str())
                                                    .unwrap_or("rom_sector.bin")
                                                    .to_string();
                                                self.file_path_input = suggested.display().to_string();
                                                if self.choose_save_file(&suggested_name) {
                                                    let sector_path = PathBuf::from(self.file_path_input.trim());
                                                    if let Err(e) = self.save_work_range_to_file(start, size, &sector_path) {
                                                        self.status = e;
                                                    }
                                                } else {
                                                    self.file_path_input = previous;
                                                    self.status = "Save cancelled".to_string();
                                                }
                                            }
                                            Err(e) => self.status = format!("Invalid sector: {e}"),
                                        }
                                    }
                                });
                            });
                        },
                    );
                },
            );
        });
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_app() -> FlashBangGuiApp {
        let mut app = FlashBangGuiApp::new();
        app.data.chip = Some(ChipId {
            manufacturer_id: 0xBF,
            device_id: 0xB7,
            name: "SST39SF040",
            size_bytes: 512 * 1024,
            sector_size: 4096,
        });
        app.ensure_chip_buffers();
        app
    }

    #[test]
    fn classifies_unknown_byte_as_gray() {
        let app = seeded_app();
        assert_eq!(app.byte_state(0), ByteState::Gray);
    }

    #[test]
    fn classifies_equal_byte_as_green() {
        let mut app = seeded_app();
        app.data.ro_known[0x20] = true;
        app.data.ro_data[0x20] = 0x5A;
        app.data.work_data[0x20] = 0x5A;
        assert_eq!(app.byte_state(0x20), ByteState::Green);
    }

    #[test]
    fn classifies_program_only_byte_as_orange() {
        let mut app = seeded_app();
        app.data.ro_known[0x40] = true;
        app.data.ro_data[0x40] = 0b1111_0011;
        app.data.work_data[0x40] = 0b1111_0001;
        assert_eq!(app.byte_state(0x40), ByteState::Orange);
    }

    #[test]
    fn classifies_erase_required_byte_as_red() {
        let mut app = seeded_app();
        app.data.ro_known[0x80] = true;
        app.data.ro_data[0x80] = 0b1111_0000;
        app.data.work_data[0x80] = 0b1111_1000;
        assert_eq!(app.byte_state(0x80), ByteState::Red);
    }

    #[test]
    fn builds_sector_file_name_with_address_and_size() {
        let base = PathBuf::from("captures/rom_dump.bin");
        let path = FlashBangGuiApp::sector_file_path(&base, 0x1A000, 4096);
        assert_eq!(path, PathBuf::from("captures/rom_dump_sector_1A000_4k.bin"));
    }

    #[test]
    fn decodes_clipboard_hex_with_prefixes_and_separators() {
        let bytes = FlashBangGuiApp::decode_clipboard_hex("0xDE, 0xAD 0xBE\n0xEF")
            .expect("clipboard hex should decode");
        assert_eq!(bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }
}
