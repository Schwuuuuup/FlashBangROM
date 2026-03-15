use std::path::PathBuf;
use std::io::{Read, Write};

use eframe::egui;
use serialport::SerialPort;

use crate::{
    protocol::{parse_device_frame, DeviceFrame},
    report::{export_report_json, export_report_text, DiffReport},
    session::{
        list_serial_ports, open_serial_port, ChipId, HelloInfo,
        SerialPortEntry,
    },
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
    read_data: Vec<u8>,
    diff_report: Option<DiffReport>,
    log: Vec<String>,
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
    active_tab: usize,
    hex_scroll_rows: usize,
    data: AppData,
    available_ports: Vec<SerialPortEntry>,
    selected_port_index: usize,
    baud_rate: u32,
    connected_port_name: Option<String>,
    serial_handle: Option<Box<dyn SerialPort>>,
    wire_log: Vec<WireLogEntry>,
    show_about: bool,
    status: String,
}

impl FlashBangGuiApp {
    fn new() -> Self {
        let data = AppData::default();

        let available_ports = list_serial_ports().unwrap_or_default();

        FlashBangGuiApp {
            active_tab: 0,
            hex_scroll_rows: 0,
            data,
            available_ports,
            selected_port_index: 0,
            baud_rate: 115_200,
            connected_port_name: None,
            serial_handle: None,
            wire_log: Vec::new(),
            show_about: false,
            status: "Nicht verbunden. Verbinde ein Geraet fuer Live-Daten.".to_string(),
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
                                self.data.log.push(format!(
                                    "ID: {} (MFR=0x{:02X} DEV=0x{:02X})",
                                    chip.name, chip.manufacturer_id, chip.device_id
                                ));
                                self.status = format!("Chip erkannt: {}", chip.name);
                            } else {
                                self.data.chip = None;
                                self.data.log.push(format!(
                                    "ID unknown: MFR=0x{:02X} DEV=0x{:02X}",
                                    mfr, dev
                                ));
                                self.status =
                                    format!("Chip nicht erkannt: MFR=0x{:02X} DEV=0x{:02X}", mfr, dev);
                            }
                        }
                        return;
                    }
                }

                self.data.chip = None;
                self.status = "Keine verwertbare ID-Antwort erhalten".to_string();
            }
            Err(e) => {
                self.data.chip = None;
                self.status = format!("ID-Abfrage fehlgeschlagen: {e}");
            }
        }
    }
}

impl eframe::App for FlashBangGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut do_refresh = false;
        let mut do_connect = false;
        let mut do_disconnect = false;
        let mut do_query_fw = false;

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Help", |ui| {
                    if ui.button("About FlashBang Studio").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
            });
        });

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("FlashBang Studio");
                ui.separator();
                ui.label("Desktop GUI Preview");
                ui.separator();
                ui.monospace(version::version_text());
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

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, 0, "Chip Info");
                ui.selectable_value(&mut self.active_tab, 1, "Hex Dump");
                ui.selectable_value(&mut self.active_tab, 2, "Diff View");
                ui.separator();
                if ui.button("Export Diff TXT").clicked() {
                    let path = PathBuf::from("flashbang-verify-report.txt");
                    match self.data.diff_report.as_ref() {
                        Some(report) => match export_report_text(&path, report) {
                            Ok(_) => self.status = format!("Exported: {}", path.display()),
                            Err(e) => self.status = format!("Export failed: {e}"),
                        },
                        None => self.status = "No diff data to export".to_string(),
                    }
                }
                if ui.button("Export Diff JSON").clicked() {
                    let path = PathBuf::from("flashbang-verify-report.json");
                    match self.data.diff_report.as_ref() {
                        Some(report) => match export_report_json(&path, report) {
                            Ok(_) => self.status = format!("Exported: {}", path.display()),
                            Err(e) => self.status = format!("Export failed: {e}"),
                        },
                        None => self.status = "No diff data to export".to_string(),
                    }
                }
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
                        // Replace mock/demo identity with live data only.
                        self.data.chip = None;
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

        egui::CentralPanel::default().show(ctx, |ui| match self.active_tab {
            0 => self.draw_chip_info(ui),
            1 => self.draw_hex_dump(ui),
            2 => self.draw_diff_view(ui),
            _ => {}
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.label(&self.status);
        });

        egui::TopBottomPanel::bottom("serial_monitor").show(ctx, |ui| {
            ui.separator();
            ui.heading("Serial Monitor (TX/RX)");
            egui::ScrollArea::vertical()
                .max_height(170.0)
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
    fn draw_chip_info(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |columns| {
            let left = &mut columns[0];
            left.group(|ui| {
                ui.heading("Device");
                ui.separator();

                if let Some(hello) = &self.data.hello {
                    ui.label(format!("FW Version: {}", hello.fw_version));
                    ui.label(format!("Protocol: {}", hello.protocol_version));
                    ui.label(format!("Capabilities: {}", hello.capabilities.join(", ")));
                }

                ui.separator();

                if let Some(chip) = &self.data.chip {
                    ui.label(format!("Chip: {}", chip.name));
                    ui.label(format!("Manufacturer: 0x{:02X}", chip.manufacturer_id));
                    ui.label(format!("Device ID: 0x{:02X}", chip.device_id));
                    ui.label(format!("Size: {} KiB", chip.size_bytes / 1024));
                    ui.label(format!("Sector Size: {} B", chip.sector_size));
                    ui.label(format!("Sector Count: {}", chip.sector_count()));
                }
            });

            let right = &mut columns[1];
            right.group(|ui| {
                ui.heading("Operation Log");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for line in &self.data.log {
                        ui.monospace(line);
                    }
                });
            });
        });
    }

    fn draw_hex_dump(&mut self, ui: &mut egui::Ui) {
        if self.data.read_data.is_empty() {
            ui.label("No read data available.");
            return;
        }

        const BYTES_PER_ROW: usize = 16;
        let max_rows = self.data.read_data.len() / BYTES_PER_ROW;

        ui.horizontal(|ui| {
            ui.label("Row Offset:");
            ui.add(egui::Slider::new(&mut self.hex_scroll_rows, 0..=max_rows).step_by(1.0));
        });

        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let start = self.hex_scroll_rows * BYTES_PER_ROW;
            let end = (start + (BYTES_PER_ROW * 64)).min(self.data.read_data.len());
            let mut offset = start;
            while offset < end {
                let row_end = (offset + BYTES_PER_ROW).min(self.data.read_data.len());
                let slice = &self.data.read_data[offset..row_end];

                let hex = slice
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                let ascii: String = slice
                    .iter()
                    .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
                    .collect();

                ui.monospace(format!("{offset:06X}: {hex:<47} {ascii}"));
                offset += BYTES_PER_ROW;
            }
        });
    }

    fn draw_diff_view(&mut self, ui: &mut egui::Ui) {
        if let Some(report) = &self.data.diff_report {
            ui.horizontal(|ui| {
                ui.label(format!("Compared: {} bytes", report.compared_len));
                ui.separator();
                ui.label(format!("Mismatches: {}", report.mismatch_count));
                ui.separator();
                ui.label(format!("Regions: {}", report.ranges.len()));
            });

            ui.separator();
            ui.heading("Mismatch Regions");
            egui::Grid::new("diff_regions").striped(true).show(ui, |ui| {
                ui.strong("Start");
                ui.strong("End");
                ui.strong("Count");
                ui.end_row();

                for r in &report.ranges {
                    ui.monospace(format!("0x{:05X}", r.start_address));
                    ui.monospace(format!("0x{:05X}", r.end_address));
                    ui.label(format!("{}", r.mismatch_count));
                    ui.end_row();
                }
            });
        } else {
            ui.label("No diff report available.");
        }
    }
}
