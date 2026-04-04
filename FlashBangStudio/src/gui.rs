use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::collections::HashMap;
use std::time::Duration;

use chrono::Local;
use eframe::egui;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serialport::SerialPort;
use tar::Builder;
use tinyfiledialogs::{input_box, message_box_yes_no, open_file_dialog, save_file_dialog_with_filter, MessageBoxIcon, YesNo};

use crate::{
    driver_catalog,
    protocol::{parse_device_frame, DeviceFrame},
    report::{build_report, DiffReport},
    session::{
        list_serial_ports, open_serial_port, parse_id_detail, ChipId, HelloInfo, SerialPortEntry,
    },
    verify::compute_diff,
    version,
};

pub fn run_gui() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 700.0])
            .with_maximized(true),
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
    Ui,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SaveFormat {
    Bin,
    Hex,
    Sector,
    BinGz,
    HexGz,
    SectorGz,
    SectorsTgz,
    Gif,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ImageSaveFormat {
    Bin,
    Hex,
    BinGz,
    HexGz,
    SectorsTgz,
}

impl ImageSaveFormat {
    const ALL: [ImageSaveFormat; 5] = [
        ImageSaveFormat::Bin,
        ImageSaveFormat::Hex,
        ImageSaveFormat::BinGz,
        ImageSaveFormat::HexGz,
        ImageSaveFormat::SectorsTgz,
    ];

    fn label(self) -> &'static str {
        match self {
            ImageSaveFormat::Bin => ".bin",
            ImageSaveFormat::Hex => ".hex",
            ImageSaveFormat::BinGz => ".bin.gz",
            ImageSaveFormat::HexGz => ".hex.gz",
            ImageSaveFormat::SectorsTgz => ".sectors.tgz",
        }
    }

    fn filter_pattern(self) -> &'static str {
        match self {
            ImageSaveFormat::Bin => "*.bin",
            ImageSaveFormat::Hex => "*.hex",
            ImageSaveFormat::BinGz => "*.bin.gz",
            ImageSaveFormat::HexGz => "*.hex.gz",
            ImageSaveFormat::SectorsTgz => "*.sectors.tgz",
        }
    }

    fn as_save_format(self) -> SaveFormat {
        match self {
            ImageSaveFormat::Bin => SaveFormat::Bin,
            ImageSaveFormat::Hex => SaveFormat::Hex,
            ImageSaveFormat::BinGz => SaveFormat::BinGz,
            ImageSaveFormat::HexGz => SaveFormat::HexGz,
            ImageSaveFormat::SectorsTgz => SaveFormat::SectorsTgz,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SectorSaveFormat {
    Sector,
    SectorGz,
}

impl SectorSaveFormat {
    const ALL: [SectorSaveFormat; 2] = [SectorSaveFormat::Sector, SectorSaveFormat::SectorGz];

    fn label(self) -> &'static str {
        match self {
            SectorSaveFormat::Sector => ".sector",
            SectorSaveFormat::SectorGz => ".sector.gz",
        }
    }

    fn filter_pattern(self) -> &'static str {
        match self {
            SectorSaveFormat::Sector => "*.sector",
            SectorSaveFormat::SectorGz => "*.sector.gz",
        }
    }

    fn as_save_format(self) -> SaveFormat {
        match self {
            SectorSaveFormat::Sector => SaveFormat::Sector,
            SectorSaveFormat::SectorGz => SaveFormat::SectorGz,
        }
    }
}

#[derive(Clone, Copy)]
enum SaveScope {
    Image,
    Sector,
}

struct WireLogEntry {
    direction: WireDirection,
    text: String,
}

#[derive(Clone)]
enum WarningAction {
    SwitchDriverAndInitialize { driver_id: String },
    ResizeWorkbench { new_size: usize },
}

#[derive(Clone)]
enum DeferredAction {
    Connect,
    QueryFirmware,
    UploadDriverAndId,
    FetchImage,
    FetchRange { start: usize, len: usize },
    FetchSector { start: usize, size: usize },
    EraseImage,
    EraseSector { start: usize },
    FlashImage,
    FlashRange { start: usize, len: usize },
    FlashSector { start: usize, size: usize },
}

#[derive(Clone)]
struct WarningDialogState {
    message: String,
    action: Option<WarningAction>,
}

#[derive(Clone, Copy)]
enum SaveFormatDialogState {
    Image,
    Sector { start: usize, size: usize },
}

pub struct FlashBangGuiApp {
    data: AppData,
    available_ports: Vec<SerialPortEntry>,
    selected_port_index: usize,
    baud_rate: u32,
    connected_port_name: Option<String>,
    serial_handle: Option<Box<dyn SerialPort>>,
    wire_log: Vec<WireLogEntry>,
    serial_monitor_text: String,
    serial_primary_selection: String,
    available_drivers: Vec<driver_catalog::DriverEntry>,
    selected_driver_index: usize,
    uploaded_driver_id: Option<String>,
    show_about: bool,
    warning_dialog: Option<WarningDialogState>,
    save_format_dialog: Option<SaveFormatDialogState>,
    is_busy: bool,
    busy_action: Option<String>,
    pending_action: Option<DeferredAction>,
    pending_action_armed: bool,
    status: String,
    diff_foreground_enabled: bool,
    palette_background_enabled: bool,
    inspector_input_mode: CharacterMode,
    workspace_input_mode: CharacterMode,
    show_sector_boundaries: bool,
    allow_flash_gray: bool,
    auto_fetch: bool,
    pending_connect_auto_fetch: bool,
    range_start_input: String,
    range_len_input: String,
    sector_input: String,
    bytes_per_row: usize,
    file_path_input: String,
    image_save_format: ImageSaveFormat,
    sector_save_format: SectorSaveFormat,
    clipboard: Vec<u8>,
    clipboard_desc: String,
    workbench_dirty: bool,
    selected_ro_addr: Option<usize>,
    selected_work_addr: Option<usize>,
    active_pane: Pane,
    drag_select_pane: Option<Pane>,
    drag_select_anchor: Option<usize>,
    pending_hex_high_nibble: Option<u8>,
    icon_assets: Option<IconAssets>,
    preview_window_open: bool,
    preview_pixels_per_row: usize,
    preview_zoom: usize,
    preview_texture: Option<egui::TextureHandle>,
    preview_texture_size: [usize; 2],
    preview_dirty: bool,
    png_import_window_open: bool,
    png_import_path: String,
    png_import_quantized: Vec<u8>,
    png_import_width: usize,
    png_import_height: usize,
    png_import_zoom: usize,
    png_import_texture: Option<egui::TextureHandle>,
    png_import_texture_dirty: bool,
    png_import_rows_per_slice: usize,
    png_import_tile_x: usize,
    png_import_tile_y: usize,
    upper_area_ratio: f32,
    hex_scroll_y: f32,
    scroll_style_initialized: bool,
}

impl FlashBangGuiApp {
    fn parse_upload_param_hex(upload_lines: &[String], key: &str) -> Option<usize> {
        let prefix = format!("PARAMETER|{key}|");
        upload_lines
            .iter()
            .find_map(|line| line.strip_prefix(&prefix))
            .and_then(|hex| usize::from_str_radix(hex.trim(), 16).ok())
    }

    fn selected_driver_geometry(&self) -> Option<(usize, usize)> {
        let selected = self.available_drivers.get(self.selected_driver_index)?;
        let plan = driver_catalog::build_upload_plan(&selected.path).ok()?;
        let chip_size = Self::parse_upload_param_hex(&plan.upload_lines, "CHIP_SIZE")?;
        let sector_size = Self::parse_upload_param_hex(&plan.upload_lines, "SECTOR_SIZE")?;
        Some((chip_size, sector_size))
    }

    fn new() -> Self {
        const PREVIEW_SIZE: usize = 0x10000;

        let mut data = AppData::default();
        // Keep the upper GUI panes visible before a chip is identified.
        data.ro_data = vec![0xFF; PREVIEW_SIZE];
        data.ro_known = vec![false; PREVIEW_SIZE];
        data.work_data = Vec::new();

        let available_ports = list_serial_ports().unwrap_or_default();
        let available_drivers = driver_catalog::list_drivers();

        let app = FlashBangGuiApp {
            data,
            available_ports,
            selected_port_index: 0,
            baud_rate: 115_200,
            connected_port_name: None,
            serial_handle: None,
            wire_log: Vec::new(),
            serial_monitor_text: String::new(),
            serial_primary_selection: String::new(),
            available_drivers,
            selected_driver_index: 0,
            uploaded_driver_id: None,
            show_about: false,
            warning_dialog: None,
            save_format_dialog: None,
            is_busy: false,
            busy_action: None,
            pending_action: None,
            pending_action_armed: false,
            status: "Nicht verbunden. Verbinde ein Geraet fuer Live-Daten (Preview aktiv).".to_string(),
            diff_foreground_enabled: true,
            palette_background_enabled: true,
            inspector_input_mode: CharacterMode::Hex,
            workspace_input_mode: CharacterMode::Hex,
            show_sector_boundaries: true,
            allow_flash_gray: false,
            auto_fetch: true,
            pending_connect_auto_fetch: false,
            range_start_input: "".to_string(),
            range_len_input: "".to_string(),
            sector_input: "0".to_string(),
            bytes_per_row: 16,
            file_path_input: "captures/rom_inspector.bin".to_string(),
            image_save_format: ImageSaveFormat::Bin,
            sector_save_format: SectorSaveFormat::Sector,
            clipboard: Vec::new(),
            clipboard_desc: "empty".to_string(),
            workbench_dirty: false,
            selected_ro_addr: None,
            selected_work_addr: None,
            active_pane: Pane::Workspace,
            drag_select_pane: None,
            drag_select_anchor: None,
            pending_hex_high_nibble: None,
            icon_assets: None,
            preview_window_open: false,
            preview_pixels_per_row: 16,
            preview_zoom: 12,
            preview_texture: None,
            preview_texture_size: [1, 1],
            preview_dirty: true,
            png_import_window_open: false,
            png_import_path: String::new(),
            png_import_quantized: Vec::new(),
            png_import_width: 0,
            png_import_height: 0,
            png_import_zoom: 8,
            png_import_texture: None,
            png_import_texture_dirty: true,
            png_import_rows_per_slice: 16,
            png_import_tile_x: 0,
            png_import_tile_y: 0,
            upper_area_ratio: 0.75,
            hex_scroll_y: 0.0,
            scroll_style_initialized: false,
        };
        app
    }

    fn ensure_solid_scrollbars(&mut self, ctx: &egui::Context) {
        if self.scroll_style_initialized {
            return;
        }

        ctx.style_mut(|style| {
            style.spacing.scroll = egui::style::ScrollStyle::solid();
            style.spacing.scroll.bar_width = 12.0;
            style.spacing.scroll.handle_min_length = 20.0;
            style.spacing.scroll.bar_inner_margin = 2.0;
            style.spacing.scroll.bar_outer_margin = 1.0;
        });

        self.scroll_style_initialized = true;
    }

    fn pane_input_mode(&self, pane: Pane) -> CharacterMode {
        match pane {
            Pane::Inspector => self.inspector_input_mode,
            Pane::Workspace => self.workspace_input_mode,
        }
    }

    fn set_pane_input_mode(&mut self, pane: Pane, mode: CharacterMode) {
        match pane {
            Pane::Inspector => self.inspector_input_mode = mode,
            Pane::Workspace => {
                self.workspace_input_mode = mode;
                if mode == CharacterMode::Ascii {
                    self.pending_hex_high_nibble = None;
                }
            }
        }
    }

    fn rebuild_preview_texture(&mut self, ctx: &egui::Context) {
        if !self.preview_dirty {
            return;
        }

        const MAX_PREVIEW_TEXTURE_SIDE: usize = 16_384;
        let data_len = self.data.work_data.len();
        let mut width = self.preview_pixels_per_row.max(1).min(MAX_PREVIEW_TEXTURE_SIDE);

        // Ensure resulting height does not exceed texture limits.
        let min_width_for_height = data_len.max(1).div_ceil(MAX_PREVIEW_TEXTURE_SIDE);
        width = width.max(min_width_for_height).min(MAX_PREVIEW_TEXTURE_SIDE);

        // Fallback for datasets that exceed single-texture capacity.
        let max_pixels = MAX_PREVIEW_TEXTURE_SIDE * MAX_PREVIEW_TEXTURE_SIDE;
        let effective_len = data_len.min(max_pixels);
        let height = effective_len.max(1).div_ceil(width);

        if width != self.preview_pixels_per_row {
            self.preview_pixels_per_row = width;
            self.status = format!(
                "Preview width angepasst auf {} (Texture-Limit {}x{}).",
                width, MAX_PREVIEW_TEXTURE_SIDE, MAX_PREVIEW_TEXTURE_SIDE
            );
        }

        if effective_len < data_len {
            self.status = format!(
                "Preview zeigt nur die ersten {} Byte (Texture-Limit {}x{}).",
                effective_len, MAX_PREVIEW_TEXTURE_SIDE, MAX_PREVIEW_TEXTURE_SIDE
            );
        }

        let mut image = egui::ColorImage::new([width, height], egui::Color32::BLACK);

        for (idx, byte) in self.data.work_data.iter().take(effective_len).enumerate() {
            image.pixels[idx] = Self::palette_color(*byte);
        }

        if let Some(texture) = &mut self.preview_texture {
            texture.set(image, egui::TextureOptions::NEAREST);
        } else {
            self.preview_texture = Some(ctx.load_texture(
                "workbench_preview",
                image,
                egui::TextureOptions::NEAREST,
            ));
        }

        self.preview_texture_size = [width, height];
        self.preview_dirty = false;
    }

    fn rebuild_png_import_texture(&mut self, ctx: &egui::Context) {
        if !self.png_import_texture_dirty {
            return;
        }

        if self.png_import_quantized.is_empty() || self.png_import_width == 0 || self.png_import_height == 0 {
            self.png_import_texture = None;
            self.png_import_texture_dirty = false;
            return;
        }

        let mut image = egui::ColorImage::new(
            [self.png_import_width, self.png_import_height],
            egui::Color32::BLACK,
        );

        for (idx, byte) in self.png_import_quantized.iter().enumerate() {
            image.pixels[idx] = Self::palette_color(*byte);
        }

        if let Some(texture) = &mut self.png_import_texture {
            texture.set(image, egui::TextureOptions::NEAREST);
        } else {
            self.png_import_texture = Some(ctx.load_texture(
                "png_import_texture",
                image,
                egui::TextureOptions::NEAREST,
            ));
        }

        self.png_import_texture_dirty = false;
    }

    fn draw_serial_monitor(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal(|ui| {
            ui.heading("Log");
            if ui.button("Clear").clicked() {
                self.serial_monitor_text.clear();
                self.serial_primary_selection.clear();
            }
            ui.label("(TX rot, RX gruen, UI blau | Markieren kopiert auch in Linux-Primary)");
        });
        egui::ScrollArea::vertical()
            .id_source("serial_monitor_scroll")
            .stick_to_bottom(true)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                let mut layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
                    let mut job = Self::serial_layout_job(text);
                    job.wrap.max_width = wrap_width;
                    ui.fonts(|f| f.layout_job(job))
                };

                let output = egui::TextEdit::multiline(&mut self.serial_monitor_text)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(10)
                    .desired_width(f32::INFINITY)
                    .layouter(&mut layouter)
                    .show(ui);

                if let Some(range) = output.cursor_range {
                    if let Some(selected) = Self::selected_text_from_range(&self.serial_monitor_text, range) {
                        if selected != self.serial_primary_selection {
                            self.serial_primary_selection = selected.clone();
                            self.copy_to_linux_primary_selection(&selected);
                        }
                    }
                }
            });
    }

    fn serial_layout_job(text: &str) -> egui::text::LayoutJob {
        let mut job = egui::text::LayoutJob::default();

        for line in text.split_inclusive('\n') {
            let color = if line.starts_with("[TX]") {
                egui::Color32::from_rgb(255, 80, 80)
            } else if line.starts_with("[RX]") {
                egui::Color32::from_rgb(100, 220, 100)
            } else {
                egui::Color32::from_rgb(136, 136, 255)
            };

            job.append(
                line,
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::monospace(12.0),
                    color,
                    ..Default::default()
                },
            );
        }

        job
    }

    fn selected_text_from_range(text: &str, range: egui::text_edit::CursorRange) -> Option<String> {
        let start_char = range.primary.ccursor.index.min(range.secondary.ccursor.index);
        let end_char = range.primary.ccursor.index.max(range.secondary.ccursor.index);
        if start_char == end_char {
            return None;
        }

        let start_byte = Self::char_to_byte_index(text, start_char);
        let end_byte = Self::char_to_byte_index(text, end_char);
        if start_byte >= end_byte || end_byte > text.len() {
            return None;
        }

        Some(text[start_byte..end_byte].to_string())
    }

    fn char_to_byte_index(text: &str, char_index: usize) -> usize {
        if char_index == 0 {
            return 0;
        }
        text.char_indices()
            .nth(char_index)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len())
    }

    fn copy_to_linux_primary_selection(&mut self, text: &str) {
        #[cfg(target_os = "linux")]
        {
            if text.is_empty() {
                return;
            }

            if Self::run_selection_command("wl-copy", &["--primary", "--type", "text/plain;charset=utf-8"], text)
                || Self::run_selection_command("xclip", &["-selection", "primary", "-in"], text)
                || Self::run_selection_command("xsel", &["--primary", "--input"], text)
            {
                return;
            }

            self.log_action("Hinweis: Linux-Primary-Clipboard konnte nicht gesetzt werden (wl-copy/xclip/xsel fehlt).".to_string());
        }
    }

    #[cfg(target_os = "linux")]
    fn run_selection_command(bin: &str, args: &[&str], text: &str) -> bool {
        let mut child = match Command::new(bin)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => return false,
        };

        if let Some(mut stdin) = child.stdin.take() {
            if stdin.write_all(text.as_bytes()).is_err() {
                let _ = child.wait();
                return false;
            }
        } else {
            let _ = child.wait();
            return false;
        }

        child.wait().map(|status| status.success()).unwrap_or(false)
    }

    fn warning_action_label(action: &WarningAction) -> &'static str {
        match action {
            WarningAction::SwitchDriverAndInitialize { .. } => "Treiber wechseln & initialisieren",
            WarningAction::ResizeWorkbench { .. } => "Workbench vergroessern",
        }
    }

    fn execute_warning_action(&mut self, action: WarningAction) {
        match action {
            WarningAction::SwitchDriverAndInitialize { driver_id } => {
                self.log_action(format!(
                    "Dialog-Aktion: Treiber wechseln & initialisieren -> {}",
                    driver_id
                ));
                if let Err(e) = self.switch_to_driver_and_initialize(&driver_id) {
                    self.status = format!("Treiberwechsel fehlgeschlagen: {e}");
                }
            }
            WarningAction::ResizeWorkbench { new_size } => {
                self.log_action(format!(
                    "Dialog-Aktion: Workbench vergroessern -> {} bytes",
                    new_size
                ));
                self.init_workbench(new_size);
                self.status = format!("Workbench vergroessert auf {} byte(s)", new_size);
            }
        }
    }

    fn ensure_chip_buffers(&mut self) {
        let Some(chip) = &self.data.chip else {
            return;
        };
        let wanted = chip.size_bytes as usize;
        if self.data.ro_data.len() != wanted {
            self.data.ro_data = vec![0xFF; wanted];
            self.data.ro_known = vec![false; wanted];
            self.selected_ro_addr = None;
            self.rebuild_diff_report();
        }
    }

    fn init_workbench(&mut self, size: usize) {
        self.data.work_data = vec![0xFF; size];
        self.workbench_dirty = false;
        self.preview_dirty = true;
        self.selected_work_addr = None;
        self.pending_hex_high_nibble = None;
        self.rebuild_diff_report();
    }

    fn confirm_discard_unsaved_workbench(&mut self) -> bool {
        if !self.workbench_dirty {
            return true;
        }

        match message_box_yes_no(
            "Workbench ersetzen",
            "Die Workbench enthaelt ungespeicherte Aenderungen. Wirklich verwerfen?",
            MessageBoxIcon::Warning,
            YesNo::No,
        ) {
            YesNo::Yes => true,
            YesNo::No => {
                self.status = "New Workbench abgebrochen (ungespeicherte Aenderungen beibehalten)".to_string();
                false
            }
        }
    }

    fn prompt_new_workbench(&mut self) {
        if !self.confirm_discard_unsaved_workbench() {
            return;
        }

        let default_size = self
            .data
            .chip
            .as_ref()
            .map(|c| format!("0x{:X}", c.size_bytes))
            .unwrap_or_else(|| "0x80000".to_string());

        let Some(input) = input_box(
            "Neue Workbench",
            "Groesse in Bytes (dezimal oder hex, z.B. 524288 oder 0x80000)",
            &default_size,
        ) else {
            self.status = "Neue Workbench abgebrochen".to_string();
            return;
        };

        match Self::parse_int_input(&input) {
            Ok(size_u32) => {
                let size = size_u32 as usize;
                if size == 0 {
                    self.status = "Neue Workbench fehlgeschlagen: Groesse muss > 0 sein".to_string();
                    return;
                }
                self.init_workbench(size);
                self.status = format!("Neue leere Workbench erstellt: {} byte(s)", size);
            }
            Err(e) => {
                self.status = format!("Neue Workbench fehlgeschlagen: {e}");
            }
        }
    }

    fn visible_grid_size(&self) -> usize {
        let chip = self.data.chip.as_ref().map(|c| c.size_bytes as usize).unwrap_or(0);
        chip.max(self.data.ro_data.len()).max(self.data.work_data.len())
    }

    fn check_or_init_workbench_for_fetch_image(&mut self, chip_size: usize) {
        let work_size = self.data.work_data.len();
        if work_size == 0 {
            self.init_workbench(chip_size);
            self.log_action(format!(
                "Workbench auto-initialisiert fuer Fetch Image: {} byte(s)",
                chip_size
            ));
            return;
        }

        if work_size > chip_size {
            self.warn_dialog(format!(
                "Hinweis: Workbench ({work_size} Bytes) ist groesser als erkannter Chip ({chip_size} Bytes)."
            ));
            return;
        }

        if work_size < chip_size {
            self.warn_dialog_with_action(
                format!(
                    "Hinweis: Erkannter Chip ({chip_size} Bytes) ist groesser als Workbench ({work_size} Bytes). Workbench vergroessern?"
                ),
                Some(WarningAction::ResizeWorkbench {
                    new_size: chip_size,
                }),
            );
        }
    }

    fn rebuild_diff_report(&mut self) {
        if self.data.ro_data.is_empty()
            || self.data.work_data.is_empty()
            || self.data.ro_data.len() != self.data.work_data.len()
        {
            self.data.diff_report = None;
            return;
        }
        let summary = compute_diff(0, &self.data.ro_data, &self.data.work_data);
        self.data.diff_report = Some(build_report(&summary));
    }

    fn chip_size(&self) -> Option<usize> {
        if let Some(chip) = self.data.chip.as_ref() {
            Some(chip.size_bytes as usize)
        } else if !self.data.ro_data.is_empty() {
            Some(self.data.ro_data.len())
        } else {
            None
        }
    }

    fn chip_status_text(&self) -> Option<String> {
        self.data.chip.as_ref().map(|chip| {
            format!(
                "Chip erkannt: {} (man 0x{:02X} dev 0x{:02X} / {}K / {}B/S / {} Sectors / driver {})",
                chip.name,
                chip.manufacturer_id,
                chip.device_id,
                chip.size_bytes / 1024,
                chip.sector_size,
                chip.sector_count(),
                chip.driver_id,
            )
        })
    }

    fn sector_size(&self) -> Option<usize> {
        if let Some(chip) = self.data.chip.as_ref() {
            Some(chip.sector_size as usize)
        } else {
            self.selected_driver_geometry().map(|(_, sector_size)| sector_size)
        }
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
        let sector_size = self
            .sector_size()
            .ok_or_else(|| "sector size unknown".to_string())?;
        let chip_size = if let Some(chip) = self.data.chip.as_ref() {
            chip.size_bytes as usize
        } else if let Some((driver_chip_size, _)) = self.selected_driver_geometry() {
            driver_chip_size
        } else {
            self.data.work_data.len()
        };
        if sector_size == 0 || chip_size == 0 {
            return Err("invalid chip/sector geometry".to_string());
        }
        let sector_count = chip_size / sector_size;
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

    fn reset_inspector_buffers(&mut self) {
        if self.data.ro_data.is_empty() || self.data.ro_known.is_empty() {
            return;
        }
        self.data.ro_data.fill(0xFF);
        self.data.ro_known.fill(false);
        self.selected_ro_addr = None;
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

    fn is_ro_known_range(&self, start: usize, len: usize) -> bool {
        if len == 0 || start + len > self.data.ro_known.len() {
            return false;
        }
        self.data.ro_known[start..start + len].iter().all(|known| *known)
    }

    fn can_flash_range(&self, start: usize, len: usize) -> bool {
        self.flash_disable_reason(start, len).is_none()
    }

    fn flash_disable_reason(&self, start: usize, len: usize) -> Option<String> {
        let mut reasons: Vec<&str> = Vec::new();

        if self.serial_handle.is_none() {
            reasons.push("Not Connected");
        }
        if self.data.chip.is_none() {
            reasons.push("Kein erkannter Chip");
        }
        if len == 0 || start + len > self.data.work_data.len() {
            reasons.push("Ungueltiger Bereich");
        }
        if self.sector_size().is_none() {
            reasons.push("Sektor-Geometrie unbekannt");
        }

        if !reasons.is_empty() {
            return Some(reasons.join(" | "));
        }

        let mut has_gray = false;
        for addr in start..start + len {
            match self.byte_state(addr) {
                ByteState::Red => {
                    reasons.push("Nicht programmierbar (rote Bytes vorhanden)");
                    break;
                }
                ByteState::Gray => has_gray = true,
                _ => {}
            }
        }

        if has_gray && !self.allow_flash_gray {
            reasons.push("Unbekannte Bytes (erst Fetch oder 'Allow Flash on gray')");
        }

        if reasons.is_empty() {
            None
        } else {
            Some(reasons.join(" | "))
        }
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
            if self.data.work_data[addr] == value {
                return;
            }
            self.data.work_data[addr] = value;
            self.workbench_dirty = true;
            self.preview_dirty = true;
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
        self.workbench_dirty = true;
        self.preview_dirty = true;
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

    fn choose_open_png_file(&mut self) -> bool {
        if let Some(path) = open_file_dialog("Open PNG", &self.png_import_path, None) {
            self.png_import_path = path;
            return true;
        }
        false
    }

    fn quantize_rgb332_nearest(r: u8, g: u8, b: u8) -> u8 {
        let mut best = 0u8;
        let mut best_dist = u32::MAX;
        for candidate in u8::MIN..=u8::MAX {
            let palette = Self::palette_color(candidate);
            let dr = i32::from(r) - i32::from(palette.r());
            let dg = i32::from(g) - i32::from(palette.g());
            let db = i32::from(b) - i32::from(palette.b());
            let dist = (dr * dr + dg * dg + db * db) as u32;
            if dist < best_dist {
                best_dist = dist;
                best = candidate;
            }
        }
        best
    }

    fn load_png_into_import_buffer(&mut self, path: &Path) -> Result<(), String> {
        let dyn_img = image::open(path).map_err(|e| format!("PNG konnte nicht geladen werden: {e}"))?;
        let rgba = dyn_img.to_rgba8();
        let width = rgba.width() as usize;
        let height = rgba.height() as usize;
        if width == 0 || height == 0 {
            return Err("PNG hat ungueltige Abmessungen".to_string());
        }

        let mut out = Vec::with_capacity(width * height);
        for px in rgba.pixels() {
            out.push(Self::quantize_rgb332_nearest(px[0], px[1], px[2]));
        }

        self.png_import_quantized = out;
        self.png_import_width = width;
        self.png_import_height = height;
        self.png_import_tile_x = 0;
        self.png_import_tile_y = 0;
        self.png_import_path = path.display().to_string();
        self.png_import_texture_dirty = true;
        Ok(())
    }

    fn png_tile_counts(&self, tile_width: usize, rows_per_slice: usize) -> (usize, usize) {
        let tx = self.png_import_width.max(1).div_ceil(tile_width.max(1));
        let ty = self.png_import_height.max(1).div_ceil(rows_per_slice.max(1));
        (tx.max(1), ty.max(1))
    }

    fn extract_png_slice(&self, tile_x: usize, tile_y: usize, tile_width: usize, rows_per_slice: usize) -> Vec<u8> {
        let tw = tile_width.max(1);
        let rh = rows_per_slice.max(1);
        let mut out = vec![0xFF; tw * rh];
        if self.png_import_quantized.is_empty() || self.png_import_width == 0 || self.png_import_height == 0 {
            return out;
        }

        let src_x0 = tile_x.saturating_mul(tw);
        let src_y0 = tile_y.saturating_mul(rh);

        for row in 0..rh {
            let sy = src_y0 + row;
            if sy >= self.png_import_height {
                break;
            }
            for col in 0..tw {
                let sx = src_x0 + col;
                if sx >= self.png_import_width {
                    break;
                }
                let src_idx = sy * self.png_import_width + sx;
                let dst_idx = row * tw + col;
                out[dst_idx] = self.png_import_quantized[src_idx];
            }
        }
        out
    }

    fn paste_bytes_into_inspector(&mut self, start: usize, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty() {
            return Err("clipboard is empty".to_string());
        }
        if start + bytes.len() > self.data.ro_data.len() {
            return Err("paste exceeds inspector".to_string());
        }

        let end = start + bytes.len();
        self.data.ro_data[start..end].copy_from_slice(bytes);
        for known in &mut self.data.ro_known[start..end] {
            *known = true;
        }
        self.rebuild_diff_report();
        self.status = format!("Pasted {} byte(s) into inspector at 0x{start:05X}", bytes.len());
        Ok(())
    }

    fn choose_save_file_with_filter(
        &mut self,
        title: &str,
        suggested_path: &str,
        filters: &[&str],
        description: &str,
    ) -> Option<PathBuf> {
        let default_path = if self.file_path_input.trim().is_empty() {
            suggested_path.to_string()
        } else {
            let previous = Path::new(self.file_path_input.trim());
            let suggested_name = Path::new(suggested_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(suggested_path);
            if let Some(parent) = previous.parent() {
                parent.join(suggested_name).display().to_string()
            } else {
                suggested_path.to_string()
            }
        };

        save_file_dialog_with_filter(title, &default_path, filters, description).map(|path| {
            self.file_path_input = path.clone();
            PathBuf::from(path)
        })
    }

    fn extension_for_format(fmt: SaveFormat) -> &'static str {
        match fmt {
            SaveFormat::Bin => "bin",
            SaveFormat::Hex => "hex",
            SaveFormat::Sector => "sector",
            SaveFormat::BinGz => "bin.gz",
            SaveFormat::HexGz => "hex.gz",
            SaveFormat::SectorGz => "sector.gz",
            SaveFormat::SectorsTgz => "sectors.tgz",
            SaveFormat::Gif => "gif",
        }
    }

    fn format_allowed_for_scope(fmt: SaveFormat, scope: SaveScope) -> bool {
        match scope {
            SaveScope::Image => matches!(
                fmt,
                SaveFormat::Bin
                    | SaveFormat::Hex
                    | SaveFormat::BinGz
                    | SaveFormat::HexGz
                    | SaveFormat::SectorsTgz
            ),
            SaveScope::Sector => matches!(fmt, SaveFormat::Sector | SaveFormat::SectorGz),
        }
    }

    fn normalize_save_path(
        path: &Path,
        scope: SaveScope,
        default_fmt: SaveFormat,
    ) -> Result<(PathBuf, SaveFormat), String> {
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| "ungueltiger Dateiname".to_string())?;

        if file_name.contains('.') {
            let fmt = Self::save_format_from_path(path)?;
            if !Self::format_allowed_for_scope(fmt, scope) {
                return Err("Dateiendung passt nicht zum gewaehlten Save-Typ".to_string());
            }
            return Ok((path.to_path_buf(), fmt));
        }

        let ext = Self::extension_for_format(default_fmt);
        let appended = PathBuf::from(format!("{}.{}", path.display(), ext));
        Ok((appended, default_fmt))
    }

    fn save_image_with_format(&mut self, selected: ImageSaveFormat) {
        let suggested = self.default_image_save_path(selected.label().trim_start_matches('.'));
        let filters = [selected.filter_pattern()];
        if let Some(path) = self.choose_save_file_with_filter(
            "Save Image",
            &suggested,
            &filters,
            "Image formats",
        ) {
            match Self::normalize_save_path(&path, SaveScope::Image, selected.as_save_format()) {
                Ok((normalized_path, fmt)) => {
                    self.file_path_input = normalized_path.display().to_string();
                    if matches!(fmt, SaveFormat::SectorsTgz) {
                        if let Err(e) = self.save_work_as_sectors_tgz(&normalized_path) {
                            self.status = e;
                        }
                    } else {
                        let size = self.data.work_data.len();
                        if let Err(e) = self.save_work_range_to_file(0, size, &normalized_path) {
                            self.status = e;
                        }
                    }
                }
                Err(e) => {
                    self.status = e;
                }
            }
        } else {
            self.status = "Save cancelled".to_string();
        }
    }

    fn save_sector_with_format(&mut self, start: usize, size: usize, selected: SectorSaveFormat) {
        let suggested_path =
            self.default_sector_save_path(start, size, selected.label().trim_start_matches('.'));
        let filters = [selected.filter_pattern()];
        if let Some(sector_path) = self.choose_save_file_with_filter(
            "Save Sector",
            &suggested_path,
            &filters,
            "Sector formats",
        ) {
            match Self::normalize_save_path(&sector_path, SaveScope::Sector, selected.as_save_format()) {
                Ok((normalized_path, _fmt)) => {
                    self.file_path_input = normalized_path.display().to_string();
                    if let Err(e) = self.save_work_range_to_file(start, size, &normalized_path) {
                        self.status = e;
                    }
                }
                Err(e) => {
                    self.status = e;
                }
            }
        } else {
            self.status = "Save cancelled".to_string();
        }
    }

    fn sanitize_file_token(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for ch in text.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
            } else if matches!(ch, '_' | '-') {
                out.push(ch);
            } else {
                out.push('_');
            }
        }
        let trimmed = out.trim_matches('_');
        if trimmed.is_empty() {
            "unknown_chip".to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn default_save_stem(&self) -> String {
        let chip_name = self
            .data
            .chip
            .as_ref()
            .map(|chip| chip.name.as_str())
            .unwrap_or("unknown_chip");
        let chip = Self::sanitize_file_token(chip_name);
        let ts = Local::now().format("%y%m%d-%H%M").to_string();
        format!("{chip}_{ts}")
    }

    fn default_image_save_path(&self, fmt: &str) -> String {
        format!("captures/{}.{}", self.default_save_stem(), fmt)
    }

    fn default_sector_save_path(&self, start: usize, len: usize, fmt: &str) -> String {
        let _ = len;
        format!("captures/{}_@{start:05X}.{}", self.default_save_stem(), fmt)
    }

    fn infer_start_from_filename(path: &Path) -> Option<usize> {
        let name = path.file_name()?.to_str()?;

        if let Some(at) = name.rfind('@') {
            let hex: String = name[at + 1..]
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            if !hex.is_empty() {
                if let Ok(v) = usize::from_str_radix(&hex, 16) {
                    return Some(v);
                }
            }
        }

        if let (Some(lb), Some(rb)) = (name.find('['), name.find(']')) {
            if rb > lb + 1 {
                let inner = &name[lb + 1..rb];
                let first = inner
                    .split(|c| c == '-' || c == ',')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_start_matches("0x")
                    .trim_start_matches("0X");
                if !first.is_empty() {
                    if let Ok(v) = usize::from_str_radix(first, 16) {
                        return Some(v);
                    }
                    if let Ok(v) = first.parse::<usize>() {
                        return Some(v);
                    }
                }
            }
        }

        let lower = name.to_ascii_lowercase();
        if let Some(idx) = lower.find("from-0x") {
            let src = &name[idx + 7..];
            let hex: String = src.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
            if !hex.is_empty() {
                if let Ok(v) = usize::from_str_radix(&hex, 16) {
                    return Some(v);
                }
            }
        }

        None
    }

    fn save_format_from_path(path: &Path) -> Result<SaveFormat, String> {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| "ungueltiger Dateiname".to_string())?
            .to_ascii_lowercase();
        if name.ends_with(".bin.gz") {
            return Ok(SaveFormat::BinGz);
        }
        if name.ends_with(".hex.gz") {
            return Ok(SaveFormat::HexGz);
        }
        if name.ends_with(".sector.gz") {
            return Ok(SaveFormat::SectorGz);
        }
        if name.ends_with(".sectors.tgz") {
            return Ok(SaveFormat::SectorsTgz);
        }
        if name.ends_with(".gif") {
            return Ok(SaveFormat::Gif);
        }
        if name.ends_with(".bin") {
            return Ok(SaveFormat::Bin);
        }
        if name.ends_with(".hex") {
            return Ok(SaveFormat::Hex);
        }
        if name.ends_with(".sector") {
            return Ok(SaveFormat::Sector);
        }
        Err(
            "Unbekanntes Dateiformat. Erlaubt: .bin, .hex, .sector, .bin.gz, .hex.gz, .sector.gz, .sectors.tgz, .gif"
                .to_string(),
        )
    }

    fn save_work_as_sectors_tgz(&mut self, path: &Path) -> Result<(), String> {
        let total_size = self.data.work_data.len();
        if total_size == 0 {
            return Err("Workbench ist leer".to_string());
        }

        let sector_size = self
            .sector_size()
            .ok_or_else(|| "Sektor-Groesse unbekannt".to_string())?;
        if sector_size == 0 {
            return Err("Sektor-Groesse ungueltig".to_string());
        }

        #[derive(serde::Serialize)]
        struct SectorEntry {
            path: String,
            start: usize,
            end: usize,
            len: usize,
        }

        #[derive(serde::Serialize)]
        struct Manifest {
            format: String,
            created_at: String,
            chip: String,
            total_size: usize,
            sector_size: usize,
            sectors: Vec<SectorEntry>,
        }

        let chip_name = self
            .data
            .chip
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "unknown_chip".to_string());

        let file = fs::File::create(path).map_err(|e| format!("save failed: {e}"))?;
        let gz = GzEncoder::new(file, Compression::default());
        let mut tar = Builder::new(gz);

        let mut sectors = Vec::new();
        for start in (0..total_size).step_by(sector_size) {
            let len = (total_size - start).min(sector_size);
            let end = start + len - 1;
            let entry_path = format!("sectors/from-0x{start:05X}_to-0x{end:05X}.sector");

            let mut header = tar::Header::new_gnu();
            header.set_mode(0o644);
            header.set_size(len as u64);
            header.set_cksum();
            tar.append_data(
                &mut header,
                entry_path.as_str(),
                &self.data.work_data[start..start + len],
            )
            .map_err(|e| format!("save failed: {e}"))?;

            sectors.push(SectorEntry {
                path: entry_path,
                start,
                end,
                len,
            });
        }

        let manifest = Manifest {
            format: "flashbang.sectors.v1".to_string(),
            created_at: Local::now().to_rfc3339(),
            chip: chip_name,
            total_size,
            sector_size,
            sectors,
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| format!("save failed: {e}"))?;
        let mut manifest_header = tar::Header::new_gnu();
        manifest_header.set_mode(0o644);
        manifest_header.set_size(manifest_bytes.len() as u64);
        manifest_header.set_cksum();
        tar.append_data(&mut manifest_header, "manifest.json", manifest_bytes.as_slice())
            .map_err(|e| format!("save failed: {e}"))?;

        let gz = tar.into_inner().map_err(|e| format!("save failed: {e}"))?;
        gz.finish().map_err(|e| format!("save failed: {e}"))?;
        self.workbench_dirty = false;
        self.status = format!(
            "Saved sectors bundle ({} sector file(s)) to {}",
            total_size.div_ceil(sector_size),
            path.display()
        );
        Ok(())
    }

    fn encode_hex_text(bytes: &[u8]) -> Vec<u8> {
        let mut text = String::new();
        for (i, b) in bytes.iter().enumerate() {
            if i > 0 {
                if i % 16 == 0 {
                    text.push('\n');
                } else {
                    text.push(' ');
                }
            }
            text.push_str(&format!("{b:02X}"));
        }
        text.push('\n');
        text.into_bytes()
    }

    fn handle_workspace_typing(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        let mut paste_event_seen = false;
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
                    paste_event_seen = true;
                    if let Some(current_addr) = self.selected_work_addr {
                        if let Err(err) = self.paste_text_into_work(current_addr, &text) {
                            self.status = format!("Paste failed: {err}");
                        }
                    } else {
                        self.status = "Paste failed: no workspace cursor selected".to_string();
                    }
                }
                egui::Event::Text(text) => {
                    if self.active_pane != Pane::Workspace {
                        continue;
                    }
                    for ch in text.chars() {
                        match self.workspace_input_mode {
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

        // Fallback: if Ctrl+V produced no egui paste event, use internal clipboard bytes.
        let command_v_pressed = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::V));
        if command_v_pressed && !paste_event_seen {
            if self.clipboard.is_empty() {
                self.status = "Paste failed: clipboard is empty".to_string();
            } else if let Some(current_addr) = self.selected_work_addr {
                let bytes = self.clipboard.clone();
                if let Err(err) = self.paste_bytes_into_work(current_addr, &bytes) {
                    self.status = format!("Paste failed: {err}");
                }
            } else {
                self.status = "Paste failed: no workspace cursor selected".to_string();
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

    fn upload_selected_driver(&mut self) -> Result<(), String> {
        if self.serial_handle.is_none() {
            return Err("not connected".to_string());
        }

        let selected = self
            .available_drivers
            .get(self.selected_driver_index)
            .ok_or_else(|| "no driver selected".to_string())?
            .clone();
        let plan = driver_catalog::build_upload_plan(&selected.path)?;

        self.data
            .log
            .push(format!("Upload driver: {}", plan.driver_id));
        for line in &plan.upload_lines {
            self.send_expect_ok(line, 6)?;
        }
        self.uploaded_driver_id = Some(plan.driver_id.clone());
        self.status = format!("Treiber hochgeladen: {}", plan.driver_id);
        Ok(())
    }

    fn push_wire(&mut self, direction: WireDirection, text: impl Into<String>) {
        let entry = WireLogEntry {
            direction,
            text: text.into(),
        };
        let prefix = match entry.direction {
            WireDirection::Tx => "[TX]",
            WireDirection::Rx => "[RX]",
            WireDirection::Ui => "[UI]",
        };
        self.serial_monitor_text
            .push_str(&format!("{} {}\n", prefix, entry.text));
        self.wire_log.push(entry);
        if self.wire_log.len() > 500 {
            let drain = self.wire_log.len() - 500;
            self.wire_log.drain(0..drain);
        }
    }

    fn log_action(&mut self, text: impl Into<String>) {
        self.push_wire(WireDirection::Ui, text.into());
    }

    fn find_driver_index_by_id(&self, driver_id: &str) -> Option<usize> {
        self.available_drivers
            .iter()
            .position(|d| d.id == driver_id)
    }

    fn switch_to_driver_and_initialize(&mut self, driver_id: &str) -> Result<(), String> {
        let idx = self
            .find_driver_index_by_id(driver_id)
            .ok_or_else(|| format!("driver not available: {driver_id}"))?;
        self.selected_driver_index = idx;
        self.upload_selected_driver()?;
        self.query_chip_id();
        Ok(())
    }

    fn warn_dialog(&mut self, text: impl Into<String>) {
        self.warn_dialog_with_action(text, None);
    }

    fn warn_dialog_with_action(&mut self, text: impl Into<String>, action: Option<WarningAction>) {
        let msg = text.into();
        self.log_action(msg.clone());
        self.warning_dialog = Some(WarningDialogState {
            message: msg,
            action,
        });
    }

    fn queue_action(&mut self, ctx: &egui::Context, label: &str, action: DeferredAction) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.busy_action = Some(label.to_string());
        self.status = format!("Laufend: {label}");
        self.log_action(format!("Action queued: {label}"));
        self.pending_action = Some(action);
        self.pending_action_armed = true;
        ctx.request_repaint();
    }

    fn execute_deferred_action(&mut self) -> Result<(), String> {
        let action = self
            .pending_action
            .take()
            .ok_or_else(|| "no deferred action".to_string())?;
        match action {
            DeferredAction::Connect => {
                let selected_port = self.available_ports.get(self.selected_port_index).cloned();
                let Some(port) = selected_port else {
                    self.status = "No serial port selected".to_string();
                    return Err("no serial port selected".to_string());
                };
                let handle = open_serial_port(&port.name, self.baud_rate, 300)
                    .map_err(|e| format!("connect failed: {e}"))?;
                if self.available_drivers.is_empty() {
                    self.status = "Kein Treiber gefunden. Bitte 'Refresh Driver' pruefen.".to_string();
                    return Err("no drivers available".to_string());
                }
                self.push_wire(
                    WireDirection::Tx,
                    format!("<open {} @ {}>", port.name, self.baud_rate),
                );
                self.serial_handle = Some(handle);
                self.connected_port_name = Some(port.name.clone());
                self.status = format!("Connected to {} @ {} baud", port.name, self.baud_rate);
                self.pending_connect_auto_fetch = self.auto_fetch;
                self.query_firmware_version();
                Ok(())
            }
            DeferredAction::QueryFirmware => {
                self.query_firmware_version();
                Ok(())
            }
            DeferredAction::UploadDriverAndId => {
                self.upload_selected_driver()?;
                self.query_chip_id();
                Ok(())
            }
            DeferredAction::FetchImage => {
                let size = self
                    .chip_size()
                    .ok_or_else(|| "fetch image failed: kein erkannter Chip".to_string())?;
                self.check_or_init_workbench_for_fetch_image(size);
                self.dump_range_to_ro(0, size)
            }
            DeferredAction::FetchRange { start, len } => self.dump_range_to_ro(start, len),
            DeferredAction::FetchSector { start, size } => self.dump_range_to_ro(start, size),
            DeferredAction::EraseImage => self.erase_chip(),
            DeferredAction::EraseSector { start } => self.erase_sector(start),
            DeferredAction::FlashImage => {
                let size = self
                    .chip_size()
                    .ok_or_else(|| "flash image failed: kein erkannter Chip".to_string())?;
                self.flash_range_from_work(0, size)
            }
            DeferredAction::FlashRange { start, len } => self.flash_range_from_work(start, len),
            DeferredAction::FlashSector { start, size } => self.flash_range_from_work(start, size),
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
                        if protocol_version != "0.4.1" {
                            self.status = format!(
                                "Protokoll nicht kompatibel: erwartet 0.4.1, erhalten {}",
                                protocol_version
                            );
                            return;
                        }
                        self.data.hello = Some(HelloInfo {
                            fw_version: fw_version.clone(),
                            protocol_version,
                            capabilities: capabilities.split(',').map(String::from).collect(),
                        });
                        self.status = format!("Firmware erkannt: {fw_version}");

                        if let Err(e) = self.upload_selected_driver() {
                            self.status = format!("Driver-Upload fehlgeschlagen: {e}");
                            return;
                        }

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

                        let (mfr_opt, dev_opt) = parse_id_detail(&detail);
                        let has_mfr = mfr_opt.is_some();
                        let has_dev = dev_opt.is_some();
                        let mfr = mfr_opt.unwrap_or(0);
                        let dev = dev_opt.unwrap_or(0);

                        if has_mfr && has_dev {
                            if let Some(chip) = ChipId::from_ids(mfr, dev) {
                                if let Some(uploaded) = &self.uploaded_driver_id {
                                    if uploaded != &chip.driver_id {
                                        self.data.chip = None;
                                        self.data.log.push(format!(
                                            "Driver mismatch: uploaded={}, detected={} (MFR=0x{:02X} DEV=0x{:02X})",
                                            uploaded, chip.driver_id, mfr, dev
                                        ));
                                        let action = if self.find_driver_index_by_id(&chip.driver_id).is_some() {
                                            Some(WarningAction::SwitchDriverAndInitialize {
                                                driver_id: chip.driver_id.clone(),
                                            })
                                        } else {
                                            None
                                        };
                                        let warn = format!(
                                            "WARN: Treiber passt nicht zum Chip: hochgeladen={}, erkannt={}. Du kannst direkt auf '{}' wechseln und initialisieren.",
                                            uploaded, chip.driver_id, chip.driver_id
                                        );
                                        self.warn_dialog_with_action(warn.clone(), action);
                                        self.status = warn;
                                        return;
                                    }
                                }

                                let chip_size_cmd =
                                    format!("PARAMETER|CHIP_SIZE|{:X}", chip.size_bytes);
                                if let Err(e) = self.send_expect_ok(&chip_size_cmd, 6) {
                                    self.status = format!(
                                        "Chip erkannt, aber CHIP_SIZE-Update fehlgeschlagen: {e}"
                                    );
                                    return;
                                }

                                self.data.chip = Some(chip.clone());
                                self.ensure_chip_buffers();
                                self.data.log.push(format!(
                                    "ID: {} (MFR=0x{:02X} DEV=0x{:02X}, driver={})",
                                    chip.name, chip.manufacturer_id, chip.device_id, chip.driver_id
                                ));
                                self.status = format!("Chip erkannt: {}", chip.name);

                                if self.pending_connect_auto_fetch && self.auto_fetch {
                                    self.pending_connect_auto_fetch = false;
                                    self.reset_inspector_buffers();
                                    self.check_or_init_workbench_for_fetch_image(chip.size_bytes as usize);
                                    self.log_action(
                                        "Auto-Fetch: running Fetch Image after connect/driver validation"
                                            .to_string(),
                                    );
                                    if let Err(e) = self.dump_range_to_ro(0, chip.size_bytes as usize)
                                    {
                                        self.status =
                                            format!("Auto-Fetch nach Connect fehlgeschlagen: {e}");
                                    }
                                }
                            } else {
                                self.data.chip = None;
                                self.data.log.push(format!(
                                    "ID unknown: MFR=0x{:02X} DEV=0x{:02X}",
                                    mfr, dev
                                ));
                                let warn = format!(
                                    "WARN: Chip nicht im Driver-Katalog: MFR=0x{:02X} DEV=0x{:02X}. Bitte anderen Treiber waehlen oder neuen Treiber anlegen.",
                                    mfr, dev
                                );
                                self.warn_dialog(warn.clone());
                                self.status = warn;
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

    fn read_single_byte(&mut self, addr: usize) -> Result<u8, String> {
        let cmd = format!("READ|{addr:05X}|1");
        let lines = self.send_expect_ok(&cmd, 8)?;
        for line in lines {
            let Ok(frame) = parse_device_frame(&line) else {
                continue;
            };
            if let DeviceFrame::DataHex { address, data, .. } = frame {
                if address as usize == addr && !data.is_empty() {
                    return Ok(data[0]);
                }
            }
        }
        Err("readback failed: no DATA frame".to_string())
    }

    fn read_single_byte_stable(&mut self, addr: usize, expected: u8) -> Result<u8, String> {
        // Some devices may briefly expose transitional values directly after
        // PROGRAM_BYTE. Require two consecutive expected reads with short retries.
        const VERIFY_ATTEMPTS: usize = 8;
        const VERIFY_DELAY_MS: u64 = 1;

        let mut last = self.read_single_byte(addr)?;
        if last == expected {
            let confirm = self.read_single_byte(addr)?;
            if confirm == expected {
                return Ok(confirm);
            }
            last = confirm;
        }

        for _ in 0..VERIFY_ATTEMPTS {
            std::thread::sleep(Duration::from_millis(VERIFY_DELAY_MS));
            let a = self.read_single_byte(addr)?;
            let b = self.read_single_byte(addr)?;
            last = b;
            if a == expected && b == expected {
                return Ok(b);
            }
        }

        Ok(last)
    }

    fn dump_range_to_ro(&mut self, start: usize, len: usize) -> Result<(), String> {
        if let (Some(chip), Some(uploaded)) = (&self.data.chip, &self.uploaded_driver_id) {
            if &chip.driver_id != uploaded {
                return Err(format!(
                    "driver mismatch: uploaded={}, detected={}. Please upload matching driver first.",
                    uploaded, chip.driver_id
                ));
            }
        }

        let chip_size = self
            .chip_size()
            .ok_or_else(|| "chip unknown - cannot fetch".to_string())?;
        if len == 0 {
            return Err("fetch length must be > 0".to_string());
        }
        if start >= chip_size || start + len > chip_size {
            return Err(format!(
                "fetch range out of bounds: start=0x{start:05X} len={len} chip_size={chip_size}"
            ));
        }

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
        let slice = &self.data.work_data[start..start + len];
        let format = Self::save_format_from_path(path)?;

        match format {
            SaveFormat::Bin | SaveFormat::Sector => {
                fs::write(path, slice).map_err(|e| format!("save failed: {e}"))?;
            }
            SaveFormat::Hex => {
                let payload = Self::encode_hex_text(slice);
                fs::write(path, payload).map_err(|e| format!("save failed: {e}"))?;
            }
            SaveFormat::BinGz | SaveFormat::SectorGz => {
                let file = fs::File::create(path).map_err(|e| format!("save failed: {e}"))?;
                let mut encoder = GzEncoder::new(file, Compression::default());
                encoder
                    .write_all(slice)
                    .map_err(|e| format!("save failed: {e}"))?;
                encoder.finish().map_err(|e| format!("save failed: {e}"))?;
            }
            SaveFormat::HexGz => {
                let payload = Self::encode_hex_text(slice);
                let file = fs::File::create(path).map_err(|e| format!("save failed: {e}"))?;
                let mut encoder = GzEncoder::new(file, Compression::default());
                encoder
                    .write_all(&payload)
                    .map_err(|e| format!("save failed: {e}"))?;
                encoder.finish().map_err(|e| format!("save failed: {e}"))?;
            }
            SaveFormat::SectorsTgz => {
                return Err(".sectors.tgz ist nur fuer Save Image verfuegbar".to_string());
            }
            SaveFormat::Gif => {
                return Err(".gif ist nur als Inputformat fuer Load verfuegbar".to_string());
            }
        }

        if start == 0 && len == self.data.work_data.len() {
            self.workbench_dirty = false;
        }
        self.status = format!("Saved {} byte(s) from Workbench to {}", len, path.display());
        Ok(())
    }

    fn load_file_into_work(&mut self, start: usize, strict_len: Option<usize>) -> Result<(), String> {
        let path = PathBuf::from(self.file_path_input.trim());
        let fmt = Self::save_format_from_path(&path)?;
        let bytes = match fmt {
            SaveFormat::Bin | SaveFormat::Sector => {
                fs::read(&path).map_err(|e| format!("load failed: {e}"))?
            }
            SaveFormat::Hex => {
                let text = fs::read_to_string(&path).map_err(|e| format!("load failed: {e}"))?;
                Self::decode_clipboard_hex(&text)?
            }
            SaveFormat::BinGz | SaveFormat::SectorGz => {
                let file = fs::File::open(&path).map_err(|e| format!("load failed: {e}"))?;
                let mut decoder = GzDecoder::new(file);
                let mut out = Vec::new();
                decoder
                    .read_to_end(&mut out)
                    .map_err(|e| format!("load failed: {e}"))?;
                out
            }
            SaveFormat::HexGz => {
                let file = fs::File::open(&path).map_err(|e| format!("load failed: {e}"))?;
                let mut decoder = GzDecoder::new(file);
                let mut text = String::new();
                decoder
                    .read_to_string(&mut text)
                    .map_err(|e| format!("load failed: {e}"))?;
                Self::decode_clipboard_hex(&text)?
            }
            SaveFormat::SectorsTgz => {
                if start != 0 || strict_len.is_some() {
                    return Err(".sectors.tgz kann nur als komplettes Image geladen werden".to_string());
                }

                #[derive(serde::Deserialize)]
                struct ManifestSector {
                    path: String,
                    start: usize,
                    end: usize,
                    len: usize,
                }

                #[derive(serde::Deserialize)]
                struct Manifest {
                    total_size: usize,
                    sectors: Vec<ManifestSector>,
                }

                let file = fs::File::open(&path).map_err(|e| format!("load failed: {e}"))?;
                let decoder = GzDecoder::new(file);
                let mut archive = tar::Archive::new(decoder);
                let mut manifest: Option<Manifest> = None;
                let mut blobs: HashMap<String, Vec<u8>> = HashMap::new();

                let entries = archive.entries().map_err(|e| format!("load failed: {e}"))?;
                for entry in entries {
                    let mut entry = entry.map_err(|e| format!("load failed: {e}"))?;
                    let path_name = entry
                        .path()
                        .map_err(|e| format!("load failed: {e}"))?
                        .to_string_lossy()
                        .to_string();
                    let mut buf = Vec::new();
                    entry
                        .read_to_end(&mut buf)
                        .map_err(|e| format!("load failed: {e}"))?;

                    if path_name == "manifest.json" {
                        manifest = Some(
                            serde_json::from_slice(&buf)
                                .map_err(|e| format!("load failed: invalid manifest.json ({e})"))?,
                        );
                    } else if path_name.ends_with(".sector") {
                        blobs.insert(path_name, buf);
                    }
                }

                let manifest = manifest.ok_or_else(|| "load failed: manifest.json fehlt".to_string())?;
                let mut out = vec![0xFF; manifest.total_size];
                for sector in manifest.sectors {
                    let data = blobs
                        .get(&sector.path)
                        .ok_or_else(|| format!("load failed: fehlender Sektor {}", sector.path))?;
                    if data.len() != sector.len {
                        return Err(format!(
                            "load failed: Sektorlaenge ungueltig fuer {} ({} statt {})",
                            sector.path,
                            data.len(),
                            sector.len
                        ));
                    }
                    if sector.end + 1 != sector.start + sector.len || sector.start + sector.len > out.len() {
                        return Err(format!("load failed: ungueltige Manifest-Range {}", sector.path));
                    }
                    out[sector.start..sector.start + sector.len].copy_from_slice(data);
                }
                out
            }
            SaveFormat::Gif => {
                let file = fs::File::open(&path).map_err(|e| format!("load failed: {e}"))?;
                let mut options = gif::DecodeOptions::new();
                options.set_color_output(gif::ColorOutput::Indexed);
                let mut decoder = options
                    .read_info(file)
                    .map_err(|e| format!("load failed: gif decode error ({e})"))?;
                let frame = decoder
                    .read_next_frame()
                    .map_err(|e| format!("load failed: gif frame error ({e})"))?
                    .ok_or_else(|| "load failed: gif enthaelt keine Frames".to_string())?;
                frame.buffer.to_vec()
            }
        };

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
        self.workbench_dirty = true;
        self.preview_dirty = true;
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
        self.workbench_dirty = true;
        self.preview_dirty = true;
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

    fn icon_button(ui: &mut egui::Ui, texture: &egui::TextureHandle, enabled: bool) -> egui::Response {
        let mut image = egui::Image::new((texture.id(), egui::vec2(120.0, 40.0)));
        if !enabled {
            image = image.tint(egui::Color32::from_gray(110));
        }

        if enabled {
            ui.add_sized([120.0, 40.0], egui::ImageButton::new(image).frame(false))
        } else {
            ui.add_sized([120.0, 40.0], image.sense(egui::Sense::hover()))
        }
    }

    fn short_tooltip_label(tooltip: &str) -> &str {
        tooltip.split(" (").next().unwrap_or(tooltip)
    }

    fn operation_button_enabled(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        key: &str,
        spec: ButtonVisualSpec,
        enabled: bool,
        disabled_reason: Option<&str>,
        tooltip: &str,
    ) -> egui::Response {
        match self.texture_for_visual(ctx, key, spec) {
            Ok(texture) => {
                let enabled = enabled && !self.is_busy;
                let response = Self::icon_button(ui, &texture, enabled);
                let short = Self::short_tooltip_label(tooltip);
                if enabled {
                    response.on_hover_text(short)
                } else {
                    let reason = if self.is_busy {
                        self.busy_action
                            .as_deref()
                            .map(|a| format!("GUI beschaeftigt: {a}"))
                            .unwrap_or_else(|| "GUI beschaeftigt".to_string())
                    } else {
                        disabled_reason.unwrap_or("Derzeit nicht verfuegbar").to_string()
                    };
                    response.on_hover_text(format!("{}\n\nNicht verfuegbar: {}", short, reason))
                }
            }
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
        let sector_size = self
            .sector_size()
            .ok_or_else(|| "chip unknown - cannot flash".to_string())?;

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
            self.log_action(format!(
                "Flash blocked: red bytes present (erase required), first red sector={sector}"
            ));
            return Err(format!(
                "flash refused: sector {sector} needs erase (red bytes present)"
            ));
        }

        if has_gray && !self.allow_flash_gray {
            self.log_action(
                "Flash blocked: gray bytes present and 'Allow Flash on gray' is disabled".to_string(),
            );
            return Err(
                "flash warning: target includes gray (stale) bytes. Fetch first or enable 'Allow Flash on gray'."
                    .to_string(),
            );
        }

        self.log_action(format!(
            "Flash start: range=0x{start:05X}+{len} allow_gray={}"
            , self.allow_flash_gray
        ));

        let mut flashed = 0usize;
        let mut skipped_equal = 0usize;
        let mut programmed_unknown = 0usize;
        let mut programmed_changed = 0usize;
        for addr in start..start + len {
            let work = self.data.work_data[addr];
            let known = self.data.ro_known[addr];
            let ro = self.data.ro_data[addr];
            if known && ro == work {
                skipped_equal += 1;
                continue;
            }
            let cmd = format!("PROGRAM_BYTE|{addr:05X}|{work:02X}");
            self.send_expect_ok(&cmd, 6)?;

            let read_back = self.read_single_byte_stable(addr, work)?;
            if read_back != work {
                self.log_action(format!(
                    "Flash verify mismatch: addr=0x{addr:05X} expected=0x{work:02X} observed=0x{read_back:02X}"
                ));
                return Err(format!(
                    "flash verify mismatch at 0x{addr:05X}: expected 0x{work:02X}, observed 0x{read_back:02X}"
                ));
            }

            flashed += 1;
            if known {
                programmed_changed += 1;
            } else {
                programmed_unknown += 1;
            }
        }

        let total = len;
        if self.auto_fetch {
            self.log_action(format!(
                "Auto-Fetch: refreshing flashed range 0x{start:05X}+{len}"
            ));
            self.dump_range_to_ro(start, len)?;
        } else {
            self.mark_ro_unknown(start, len);
        }
        self.log_action(format!(
            "Flash done: total={} programmed={} skipped_equal={} programmed_changed={} programmed_unknown={} verify_failures=0",
            total, flashed, skipped_equal, programmed_changed, programmed_unknown
        ));
        if self.auto_fetch {
            self.status = format!(
                "Flash done: total={total}, programmed={flashed}, skipped_equal={skipped_equal}, changed={programmed_changed}, unknown={programmed_unknown}, verify_failures=0. Auto-Fetch refreshed flashed range."
            );
        } else {
            self.status = format!(
                "Flash done: total={total}, programmed={flashed}, skipped_equal={skipped_equal}, changed={programmed_changed}, unknown={programmed_unknown}, verify_failures=0. Inspector marked stale/gray in affected range."
            );
        }
        Ok(())
    }

    fn erase_sector(&mut self, start: usize) -> Result<(), String> {
        let cmd = format!("SECTOR_ERASE|{start:05X}");
        self.send_expect_ok(&cmd, 6)?;
        let sector_size = self
            .sector_size()
            .ok_or_else(|| "chip unknown - cannot erase sector".to_string())?;
        if self.auto_fetch {
            self.log_action(format!(
                "Auto-Fetch: refreshing erased sector 0x{start:05X}+{sector_size}"
            ));
            self.dump_range_to_ro(start, sector_size)?;
            self.status = format!(
                "Erased sector at 0x{start:05X}. Auto-Fetch refreshed erased sector."
            );
        } else {
            self.mark_ro_unknown(start, sector_size);
            self.status = format!("Erased sector at 0x{start:05X}. Inspector marked stale/gray.");
        }
        Ok(())
    }

    fn erase_chip(&mut self) -> Result<(), String> {
        self.send_expect_ok("CHIP_ERASE", 6)?;
        let chip_size = self
            .chip_size()
            .ok_or_else(|| "chip unknown - cannot erase chip".to_string())?;
        if self.auto_fetch {
            self.log_action("Auto-Fetch: refreshing entire chip after erase".to_string());
            self.dump_range_to_ro(0, chip_size)?;
            self.status = "Chip erased. Auto-Fetch refreshed entire Inspector view.".to_string();
        } else {
            self.mark_ro_unknown(0, chip_size);
            self.status = "Chip erased. Entire Inspector view marked stale/gray.".to_string();
        }
        Ok(())
    }

    fn byte_category_color(byte: u8) -> egui::Color32 {
        let ch = Self::decode_latin15(byte);

        // Eigene Kategorie: 0xFF (mittel-dunkles Grau)
        if byte == 0xFF {
            return egui::Color32::from_rgb(112, 112, 112);
        }

        // Gruppe E: Whitespace (weiss/hellgrau)
        if byte == b' ' || byte == b'\t' || byte == b'\n' || byte == b'\r' {
            return egui::Color32::from_rgb(242, 242, 242);
        }

        // Gruppe F: sonstige Steuerzeichen (Signal-Rot)
        if byte < 0x20 || byte == 0x7F {
            return egui::Color32::from_rgb(220, 94, 94);
        }

        // Gruppe D: Waehrung / Typografie / Akzentzeichen
        if matches!(
            ch,
            '€' | '$' | '£' | '¥' | '§' | '©' | '®' | 'ª' | 'º' | '«' | '»' | 'µ' | '±' | '×' | '÷'
                | '´' | '^' | '¨' | '~' | '`'
        ) {
            return egui::Color32::from_rgb(204, 184, 116);
        }

        // Gruppe A: ASCII-Ziffern/Gross/Klein (Gruen bis Cyan)
        if byte.is_ascii_digit() {
            return egui::Color32::from_rgb(122, 190, 160);
        }

        if byte.is_ascii_lowercase() {
            return egui::Color32::from_rgb(116, 182, 178);
        }

        if byte.is_ascii_uppercase() {
            return egui::Color32::from_rgb(128, 198, 168);
        }

        // Gruppe C: Umlaute + westeuropaeische Buchstaben
        if matches!(
            ch,
            'ä' | 'ö' | 'ü' | 'Ä' | 'Ö' | 'Ü' | 'ß' | 'é' | 'è' | 'ê' | 'ë' | 'à' | 'â' | 'ç'
                | 'ñ' | 'å' | 'ø' | 'œ' | 'Œ' | 'š' | 'ž' | 'Ÿ'
        ) {
            return egui::Color32::from_rgb(148, 146, 214);
        }

        // Gruppe B1: einfache ASCII-Sonderzeichen
        if matches!(
            ch,
            '!' | '"' | '#' | '%' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | '-' | '.' | '/'
        ) {
            return egui::Color32::from_rgb(198, 144, 112);
        }

        // Gruppe B2: erweiterte ASCII-Sonderzeichen
        if matches!(
            ch,
            ':' | ';' | '<' | '=' | '>' | '?' | '@' | '[' | '\\' | ']' | '_' | '{' | '|' | '}'
        ) {
            return egui::Color32::from_rgb(190, 126, 108);
        }

        // Gruppe 6 (Fallback): Sonstiges / exotische Sonderzeichen
        egui::Color32::from_rgb(166, 154, 184)
    }

    fn byte_color_for_ro(&self, addr: usize) -> egui::Color32 {
        if self.diff_foreground_enabled {
            Self::diff_color_for_state(self.byte_state(addr))
        } else {
            self.data
                .ro_data
                .get(addr)
                .copied()
                .map(Self::byte_category_color)
                .unwrap_or_else(|| egui::Color32::from_rgb(178, 178, 178))
        }
    }

    fn byte_color_for_work(&self, addr: usize) -> egui::Color32 {
        if self.diff_foreground_enabled {
            Self::diff_color_for_state(self.byte_state(addr))
        } else {
            self.data
                .work_data
                .get(addr)
                .copied()
                .map(Self::byte_category_color)
                .unwrap_or_else(|| egui::Color32::from_rgb(178, 178, 178))
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

    fn is_non_printable_byte(byte: u8) -> bool {
        if byte < 0x20 || byte == 0x7F {
            return true;
        }
        Self::decode_latin15(byte).is_control()
    }

    fn paint_ascii_cell_text(
        ui: &egui::Ui,
        rect: egui::Rect,
        byte: u8,
        fill_color: egui::Color32,
        selected: bool,
    ) {
        let base_font = egui::TextStyle::Monospace.resolve(ui.style());
        let ascii_font = egui::FontId::new(base_font.size * 1.5, base_font.family.clone());
        let outline_color = if selected {
            egui::Color32::BLACK
        } else {
            egui::Color32::from_rgb(20, 20, 20)
        };

        let draw_outlined = |pos: egui::Pos2, text: &str, font: &egui::FontId| {
            let offsets = [
                egui::vec2(-1.0, 0.0),
                egui::vec2(1.0, 0.0),
                egui::vec2(0.0, -1.0),
                egui::vec2(0.0, 1.0),
            ];
            for offset in offsets {
                ui.painter().text(
                    pos + offset,
                    egui::Align2::CENTER_CENTER,
                    text,
                    font.clone(),
                    outline_color,
                );
            }
            ui.painter().text(
                pos,
                egui::Align2::CENTER_CENTER,
                text,
                font.clone(),
                fill_color,
            );
        };

        if Self::is_non_printable_byte(byte) {
            let hex = format!("{byte:02X}");
            let mut chars = hex.chars();
            let hi = chars.next().unwrap_or('0').to_string();
            let lo = chars.next().unwrap_or('0').to_string();
            let stacked_font = egui::FontId::new(base_font.size * 0.95, base_font.family);
            let top = egui::pos2(rect.center().x, rect.top() + rect.height() * 0.30);
            let bottom = egui::pos2(rect.center().x, rect.top() + rect.height() * 0.72);
            draw_outlined(top, &hi, &stacked_font);
            draw_outlined(bottom, &lo, &stacked_font);
        } else {
            let ch = Self::decode_latin15(byte);
            let text = ch.to_string();
            draw_outlined(rect.center(), &text, &ascii_font);
        }
    }

    fn draw_byte_grid(&mut self, ui: &mut egui::Ui, pane: Pane, id_suffix: &str) {
        let chip_size = self.visible_grid_size();
        if chip_size == 0 {
            ui.label("No chip identified.");
            return;
        }

        let bytes_per_row = self.bytes_per_row.max(1);
        let total_rows = chip_size.div_ceil(bytes_per_row);
        let sector_size = self.sector_size().unwrap_or(4096);
        let sector_label_inactive = egui::Color32::from_rgb(166, 154, 120);
        let sector_label_active_bg = egui::Color32::from_rgb(166, 154, 120);
        let sector_label_active_fg = egui::Color32::from_rgb(24, 22, 18);
        let sector_active_border = egui::Color32::from_rgb(124, 112, 84);
        let cursor_color = egui::Color32::from_rgb(0x00, 0xE5, 0xFF);
        let selection_color = egui::Color32::from_rgb(0xF3, 0x7F, 0xFB);
        let active_sector_from_input = Self::parse_int_input(&self.sector_input)
            .ok()
            .map(|v| v as usize);
        let selected_range = self
            .parse_range_input()
            .ok()
            .map(|(start, len)| (start, start + len - 1));

        let base_text_height = ui.text_style_height(&egui::TextStyle::Monospace);
        let row_height = (base_text_height * 1.5) + 2.0;
        let hex_cell_width = 20.0;
        let ascii_cell_width = 10.0;
        let old_item_spacing = ui.spacing().item_spacing;
        let old_button_padding = ui.spacing().button_padding;
        let old_interact_size = ui.spacing().interact_size;
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
        ui.spacing_mut().interact_size = egui::vec2(hex_cell_width, row_height);

        let scroll_output = egui::ScrollArea::both()
            .id_source(id_suffix)
            .vertical_scroll_offset(self.hex_scroll_y)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show_rows(ui, row_height, total_rows, |ui, row_range| {
                egui::Grid::new(format!("hex_grid_{id_suffix}"))
                    .striped(true)
                    .min_col_width(0.0)
                    .spacing(egui::vec2(0.0, 0.0))
                    .show(ui, |ui| {
                        for row in row_range {
                            let offset = row * bytes_per_row;
                            if self.show_sector_boundaries && offset % sector_size == 0 {
                                let sector_idx = offset / sector_size;
                                let is_active_sector = active_sector_from_input == Some(sector_idx);
                                let sector_label = format!("S{:03}", sector_idx);
                                if is_active_sector {
                                    let rich = egui::RichText::new(sector_label)
                                        .color(sector_label_active_fg)
                                        .monospace();
                                    let button = egui::Button::new(rich)
                                        .fill(sector_label_active_bg)
                                        .stroke(egui::Stroke::new(1.0, sector_active_border));
                                    ui.add_sized([34.0, row_height], button);
                                } else {
                                    ui.add_sized(
                                        [34.0, row_height],
                                        egui::Label::new(
                                            egui::RichText::new(sector_label)
                                                .color(sector_label_inactive)
                                                .monospace(),
                                        ),
                                    );
                                }
                            } else {
                                ui.add_sized([34.0, row_height], egui::Label::new("   "));
                            }
                            ui.add_sized(
                                [52.0, row_height],
                                egui::Label::new(
                                    egui::RichText::new(format!("{offset:05X}"))
                                        .color(egui::Color32::from_rgb(0x57, 0xAD, 0xCA))
                                        .monospace(),
                                ),
                            );

                            for col in 0..bytes_per_row {
                                let addr = offset + col;

                                let byte = match pane {
                                    Pane::Inspector => self.data.ro_data.get(addr).copied(),
                                    Pane::Workspace => self.data.work_data.get(addr).copied(),
                                };

                                let color = if byte.is_some() {
                                    match pane {
                                        Pane::Inspector => self.byte_color_for_ro(addr),
                                        Pane::Workspace => self.byte_color_for_work(addr),
                                    }
                                } else {
                                    egui::Color32::TRANSPARENT
                                };

                                let selected = match pane {
                                    Pane::Inspector => self.selected_ro_addr == Some(addr),
                                    Pane::Workspace => self.selected_work_addr == Some(addr),
                                };
                                let in_selected_range = selected_range
                                    .map(|(start, end)| addr >= start && addr <= end)
                                    .unwrap_or(false);
                                let in_active_sector = active_sector_from_input
                                    .map(|sector_idx| addr / sector_size == sector_idx)
                                    .unwrap_or(false);

                                let unknown_inspector_cell = matches!(pane, Pane::Inspector)
                                    && !self.data.ro_known.get(addr).copied().unwrap_or(false);
                                let draw_cell_content = byte.is_some() && !unknown_inspector_cell;

                                let mut text = byte.map(|value| format!("{value:02X}")).unwrap_or_default();
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(hex_cell_width, row_height),
                                    egui::Sense::click_and_drag(),
                                );

                                if !draw_cell_content {
                                    text.clear();
                                }

                                if draw_cell_content && self.palette_background_enabled {
                                    if let Some(value) = byte {
                                        ui.painter().rect_filled(rect, 0.0, Self::palette_color(value));
                                    }
                                }

                                let outline_width = 1.4;
                                if selected {
                                    ui.painter().rect_stroke(
                                        rect.shrink(0.5),
                                        0.0,
                                        egui::Stroke::new(outline_width, cursor_color),
                                    );
                                } else if in_selected_range {
                                    ui.painter().rect_stroke(
                                        rect.shrink(0.5),
                                        0.0,
                                        egui::Stroke::new(outline_width, selection_color),
                                    );
                                } else if in_active_sector {
                                    ui.painter().rect_stroke(
                                        rect.shrink(0.5),
                                        0.0,
                                        egui::Stroke::new(outline_width, sector_active_border),
                                    );
                                }
                                if draw_cell_content {
                                    Self::paint_outlined_cell_text(ui, rect, &text, color, selected);
                                }
                                if self.drag_select_pane.is_none()
                                    && ui.ctx().input(|i| i.pointer.primary_pressed())
                                    && ui
                                        .ctx()
                                        .input(|i| i.pointer.interact_pos())
                                        .map(|pos| rect.contains(pos))
                                        .unwrap_or(false)
                                {
                                    let shift_pressed = ui.ctx().input(|i| i.modifiers.shift);
                                    self.drag_select_pane = Some(pane);

                                    if shift_pressed {
                                        let anchor = match pane {
                                            Pane::Inspector => self.selected_ro_addr,
                                            Pane::Workspace => self.selected_work_addr,
                                        }
                                        .unwrap_or(addr);
                                        self.drag_select_anchor = Some(anchor);
                                    } else {
                                        self.drag_select_anchor = Some(addr);
                                        match pane {
                                            Pane::Inspector => {
                                                self.selected_ro_addr = Some(addr);
                                                self.active_pane = Pane::Inspector;
                                                self.set_pane_input_mode(Pane::Inspector, CharacterMode::Hex);
                                            }
                                            Pane::Workspace => {
                                                self.selected_work_addr = Some(addr);
                                                self.active_pane = Pane::Workspace;
                                                self.set_pane_input_mode(Pane::Workspace, CharacterMode::Hex);
                                            }
                                        }
                                        self.range_start_input = format!("{addr:05X}");
                                        self.range_len_input = "1".to_string();
                                        self.pending_hex_high_nibble = None;
                                    }
                                    self.sector_input = (addr / sector_size).to_string();
                                }

                                if ui.ctx().input(|i| i.pointer.primary_down())
                                    && self.drag_select_pane == Some(pane)
                                    && ui
                                        .ctx()
                                        .input(|i| i.pointer.interact_pos())
                                        .map(|pos| rect.contains(pos))
                                        .unwrap_or(false)
                                {
                                    if let Some(start_anchor) = self.drag_select_anchor {
                                        let start = start_anchor.min(addr);
                                        let end = start_anchor.max(addr);
                                        let len = end - start + 1;
                                        self.range_start_input = format!("{start:05X}");
                                        self.range_len_input = len.to_string();
                                        self.status = format!(
                                            "Range selected: 0x{start:05X}..0x{end:05X} ({len} byte(s))"
                                        );
                                        self.sector_input = (addr / sector_size).to_string();
                                    }
                                }

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
                                            self.set_pane_input_mode(Pane::Inspector, CharacterMode::Hex);
                                        }
                                        Pane::Workspace => {
                                            self.selected_work_addr = Some(addr);
                                            self.active_pane = Pane::Workspace;
                                            self.set_pane_input_mode(Pane::Workspace, CharacterMode::Hex);
                                        }
                                    }
                                    self.sector_input = (addr / sector_size).to_string();
                                    self.pending_hex_high_nibble = None;
                                }
                            }

                            ui.add_space(8.0);

                            for col in 0..bytes_per_row {
                                let addr = offset + col;

                                let byte = match pane {
                                    Pane::Inspector => self.data.ro_data.get(addr).copied(),
                                    Pane::Workspace => self.data.work_data.get(addr).copied(),
                                };

                                let color = if byte.is_some() {
                                    match pane {
                                        Pane::Inspector => self.byte_color_for_ro(addr),
                                        Pane::Workspace => self.byte_color_for_work(addr),
                                    }
                                } else {
                                    egui::Color32::TRANSPARENT
                                };

                                let selected = match pane {
                                    Pane::Inspector => self.selected_ro_addr == Some(addr),
                                    Pane::Workspace => self.selected_work_addr == Some(addr),
                                };
                                let in_selected_range = selected_range
                                    .map(|(start, end)| addr >= start && addr <= end)
                                    .unwrap_or(false);
                                let in_active_sector = active_sector_from_input
                                    .map(|sector_idx| addr / sector_size == sector_idx)
                                    .unwrap_or(false);

                                let unknown_inspector_cell = matches!(pane, Pane::Inspector)
                                    && !self.data.ro_known.get(addr).copied().unwrap_or(false);
                                let draw_cell_content = byte.is_some() && !unknown_inspector_cell;

                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(ascii_cell_width, row_height),
                                    egui::Sense::click_and_drag(),
                                );

                                if draw_cell_content {
                                    if selected {
                                        ui.painter().rect_filled(rect, 0.0, cursor_color);
                                    } else if in_selected_range {
                                        ui.painter().rect_filled(rect, 0.0, selection_color);
                                    } else if in_active_sector {
                                        ui.painter().rect_filled(rect, 0.0, sector_label_active_bg);
                                    }
                                }
                                if draw_cell_content {
                                    if let Some(value) = byte {
                                        Self::paint_ascii_cell_text(ui, rect, value, color, selected);
                                    }
                                }
                                if self.drag_select_pane.is_none()
                                    && ui.ctx().input(|i| i.pointer.primary_pressed())
                                    && ui
                                        .ctx()
                                        .input(|i| i.pointer.interact_pos())
                                        .map(|pos| rect.contains(pos))
                                        .unwrap_or(false)
                                {
                                    let shift_pressed = ui.ctx().input(|i| i.modifiers.shift);
                                    self.drag_select_pane = Some(pane);

                                    if shift_pressed {
                                        let anchor = match pane {
                                            Pane::Inspector => self.selected_ro_addr,
                                            Pane::Workspace => self.selected_work_addr,
                                        }
                                        .unwrap_or(addr);
                                        self.drag_select_anchor = Some(anchor);
                                    } else {
                                        self.drag_select_anchor = Some(addr);
                                        match pane {
                                            Pane::Inspector => {
                                                self.selected_ro_addr = Some(addr);
                                                self.active_pane = Pane::Inspector;
                                                self.set_pane_input_mode(Pane::Inspector, CharacterMode::Ascii);
                                            }
                                            Pane::Workspace => {
                                                self.selected_work_addr = Some(addr);
                                                self.active_pane = Pane::Workspace;
                                                self.set_pane_input_mode(Pane::Workspace, CharacterMode::Ascii);
                                            }
                                        }
                                        self.range_start_input = format!("{addr:05X}");
                                        self.range_len_input = "1".to_string();
                                        self.pending_hex_high_nibble = None;
                                    }
                                    self.sector_input = (addr / sector_size).to_string();
                                }

                                if ui.ctx().input(|i| i.pointer.primary_down())
                                    && self.drag_select_pane == Some(pane)
                                    && ui
                                        .ctx()
                                        .input(|i| i.pointer.interact_pos())
                                        .map(|pos| rect.contains(pos))
                                        .unwrap_or(false)
                                {
                                    if let Some(start_anchor) = self.drag_select_anchor {
                                        let start = start_anchor.min(addr);
                                        let end = start_anchor.max(addr);
                                        let len = end - start + 1;
                                        self.range_start_input = format!("{start:05X}");
                                        self.range_len_input = len.to_string();
                                        self.status = format!(
                                            "Range selected: 0x{start:05X}..0x{end:05X} ({len} byte(s))"
                                        );
                                        self.sector_input = (addr / sector_size).to_string();
                                    }
                                }

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
                                            self.set_pane_input_mode(Pane::Inspector, CharacterMode::Ascii);
                                        }
                                        Pane::Workspace => {
                                            self.selected_work_addr = Some(addr);
                                            self.active_pane = Pane::Workspace;
                                            self.set_pane_input_mode(Pane::Workspace, CharacterMode::Ascii);
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

                self.hex_scroll_y = scroll_output.state.offset.y;

                ui.spacing_mut().item_spacing = old_item_spacing;
                ui.spacing_mut().button_padding = old_button_padding;
                ui.spacing_mut().interact_size = old_interact_size;
    }
}

impl eframe::App for FlashBangGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ensure_solid_scrollbars(ctx);

        if !ctx.input(|i| i.pointer.primary_down()) {
            self.drag_select_pane = None;
            self.drag_select_anchor = None;
        }

        let mut do_refresh = false;
        let mut do_connect = false;
        let mut do_disconnect = false;
        let mut do_query_fw = false;
        let mut do_upload_driver = false;

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
                if self.is_busy {
                    ui.colored_label(egui::Color32::from_rgb(255, 170, 40), status_line);
                } else {
                    ui.label(status_line);
                }
            });

            ui.horizontal(|ui| {
                ui.add_enabled_ui(self.serial_handle.is_none() && !self.is_busy, |ui| {
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

                ui.separator();
                ui.label("Treiber:");
                let selected_driver = self
                    .available_drivers
                    .get(self.selected_driver_index)
                    .map(|d| d.id.as_str())
                    .unwrap_or("<none>");
                egui::ComboBox::from_id_source("driver_combo")
                    .selected_text(selected_driver)
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for (idx, d) in self.available_drivers.iter().enumerate() {
                            ui.selectable_value(&mut self.selected_driver_index, idx, d.id.clone());
                        }
                    });
                if ui.button("Refresh Driver").clicked() {
                    self.available_drivers = driver_catalog::list_drivers();
                    if self.selected_driver_index >= self.available_drivers.len() {
                        self.selected_driver_index = 0;
                    }
                }
                if self.serial_handle.is_some() && !self.is_busy && ui.button("Upload Driver + ID").clicked() {
                    self.log_action("Button: Upload Driver + ID");
                    do_upload_driver = true;
                }

                if self.serial_handle.is_some() {
                    if !self.is_busy && ui.button("Firmware abfragen").clicked() {
                        self.log_action("Button: Firmware abfragen");
                        do_query_fw = true;
                    }
                    if !self.is_busy && ui.button("Disconnect").clicked() {
                        self.log_action("Button: Disconnect");
                        do_disconnect = true;
                    }
                } else if !self.is_busy && ui.button("Connect").clicked() {
                    self.log_action("Button: Connect");
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
            self.pending_connect_auto_fetch = false;
            self.status = "Serial port disconnected".to_string();
        }

        if do_connect {
            self.queue_action(ctx, "Connect", DeferredAction::Connect);
        }

        if do_query_fw {
            self.queue_action(ctx, "Firmware abfragen", DeferredAction::QueryFirmware);
        }

        if do_upload_driver {
            self.queue_action(ctx, "Upload Driver + ID", DeferredAction::UploadDriverAndId);
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

        if let Some(dialog) = self.warning_dialog.clone() {
            let mut close_warn = false;
            let mut do_action = false;
            egui::Window::new("Warnung")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.colored_label(egui::Color32::from_rgb(230, 180, 40), &dialog.message);
                    ui.add_space(8.0);
                    if let Some(action) = dialog.action.as_ref() {
                        if ui.button(Self::warning_action_label(action)).clicked() {
                            do_action = true;
                        }
                    }
                    if ui.button("OK").clicked() {
                        close_warn = true;
                    }
                });

            if do_action {
                // Close the current dialog first so follow-up warnings raised by the action
                // are not immediately cleared by this dialog's close path.
                self.warning_dialog = None;
                if let Some(action) = dialog.action {
                    self.execute_warning_action(action);
                }
            }

            if close_warn {
                self.warning_dialog = None;
            }
        }

        if let Some(dialog) = self.save_format_dialog {
            let mut cancel = false;
            let mut selected_image: Option<ImageSaveFormat> = None;
            let mut selected_sector: Option<SectorSaveFormat> = None;
            egui::Window::new("Save Format")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Format auswaehlen:");
                    ui.add_space(6.0);
                    match dialog {
                        SaveFormatDialogState::Image => {
                            ui.horizontal_wrapped(|ui| {
                                for fmt in ImageSaveFormat::ALL {
                                    if ui.button(fmt.label()).clicked() {
                                        selected_image = Some(fmt);
                                    }
                                }
                            });
                        }
                        SaveFormatDialogState::Sector { .. } => {
                            ui.horizontal_wrapped(|ui| {
                                for fmt in SectorSaveFormat::ALL {
                                    if ui.button(fmt.label()).clicked() {
                                        selected_sector = Some(fmt);
                                    }
                                }
                            });
                        }
                    }
                    ui.add_space(8.0);
                    if ui.button("Abbrechen").clicked() {
                        cancel = true;
                    }
                });

            if cancel {
                self.save_format_dialog = None;
                self.status = "Save cancelled".to_string();
            }

            if let Some(fmt) = selected_image {
                self.save_format_dialog = None;
                self.image_save_format = fmt;
                self.save_image_with_format(fmt);
            }

            if let Some(fmt) = selected_sector {
                if let SaveFormatDialogState::Sector { start, size } = dialog {
                    self.save_format_dialog = None;
                    self.sector_save_format = fmt;
                    self.save_sector_with_format(start, size, fmt);
                }
            }
        }

        if self.preview_window_open {
            let mut open = self.preview_window_open;
            egui::Window::new("Workbench Preview")
                .open(&mut open)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Pixels/Row:");
                        let pixels_resp = ui.add(
                            egui::DragValue::new(&mut self.preview_pixels_per_row)
                                .clamp_range(1..=4096),
                        );
                        ui.label("Zoom:");
                        let zoom_resp = ui.add(
                            egui::DragValue::new(&mut self.preview_zoom)
                                .clamp_range(1..=32),
                        );
                        if pixels_resp.changed() || zoom_resp.changed() {
                            self.preview_dirty = true;
                        }
                    });

                    self.rebuild_preview_texture(ctx);

                    if let Some(texture) = &self.preview_texture {
                        let zoom = self.preview_zoom.max(1) as f32;
                        let size = egui::vec2(
                            self.preview_texture_size[0] as f32 * zoom,
                            self.preview_texture_size[1] as f32 * zoom,
                        );
                        egui::ScrollArea::both().show(ui, |ui| {
                            ui.add(egui::Image::new((texture.id(), size)));
                        });
                    } else {
                        ui.label("No preview data available.");
                    }
                });
            self.preview_window_open = open;
        }

        if self.png_import_window_open {
            let mut open = self.png_import_window_open;
            egui::Window::new("PNG Import")
                .open(&mut open)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("PNG laden").clicked() {
                            if self.choose_open_png_file() {
                                let path = PathBuf::from(self.png_import_path.trim());
                                match self.load_png_into_import_buffer(&path) {
                                    Ok(()) => {
                                        self.status = format!(
                                            "PNG geladen: {}x{} px",
                                            self.png_import_width, self.png_import_height
                                        );
                                    }
                                    Err(e) => {
                                        self.status = e;
                                    }
                                }
                            } else {
                                self.status = "PNG-Import abgebrochen".to_string();
                            }
                        }
                        if !self.png_import_path.trim().is_empty() {
                            ui.monospace(self.png_import_path.as_str());
                        }
                    });

                    if !self.png_import_quantized.is_empty() {
                        let tile_width = self.bytes_per_row.max(1);
                        let mut rows_per_slice = self.png_import_rows_per_slice.max(1);
                        let counts = self.png_tile_counts(tile_width, rows_per_slice);
                        let max_tile_x = counts.0.saturating_sub(1);
                        let max_tile_y = counts.1.saturating_sub(1);
                        self.png_import_tile_x = self.png_import_tile_x.min(max_tile_x);
                        self.png_import_tile_y = self.png_import_tile_y.min(max_tile_y);

                        ui.separator();
                        ui.label(format!(
                            "Quelle: {}x{} px | Slice-Breite: {} px (Cells/Row)",
                            self.png_import_width, self.png_import_height, tile_width
                        ));

                        ui.horizontal(|ui| {
                            ui.label("Rows pro Slice:");
                            if ui
                                .add(egui::DragValue::new(&mut rows_per_slice).clamp_range(1..=1024))
                                .changed()
                            {
                                self.png_import_rows_per_slice = rows_per_slice;
                                self.png_import_tile_y = 0;
                            }
                            ui.separator();
                            ui.label("Zoom:");
                            ui.add(
                                egui::DragValue::new(&mut self.png_import_zoom)
                                    .clamp_range(1..=64),
                            );
                            ui.separator();
                            ui.label("Tile X:");
                            ui.add(
                                egui::DragValue::new(&mut self.png_import_tile_x)
                                    .clamp_range(0..=max_tile_x),
                            );
                            ui.label("Tile Y:");
                            ui.add(
                                egui::DragValue::new(&mut self.png_import_tile_y)
                                    .clamp_range(0..=max_tile_y),
                            );
                        });

                        let slice = self.extract_png_slice(
                            self.png_import_tile_x,
                            self.png_import_tile_y,
                            tile_width,
                            self.png_import_rows_per_slice,
                        );

                        self.rebuild_png_import_texture(ctx);
                        if let Some(texture) = &self.png_import_texture {
                            let zoom = self.png_import_zoom.max(1) as f32;
                            let render_size = egui::vec2(
                                self.png_import_width as f32 * zoom,
                                self.png_import_height as f32 * zoom,
                            );

                            ui.add_space(4.0);
                            egui::ScrollArea::both().max_height(320.0).show(ui, |ui| {
                                let image_response = ui.add(
                                    egui::Image::new((texture.id(), render_size))
                                        .sense(egui::Sense::click_and_drag()),
                                );

                                if (image_response.clicked() || image_response.dragged())
                                    && image_response.hovered()
                                {
                                    if let Some(pos) = image_response.interact_pointer_pos() {
                                        let local = pos - image_response.rect.min;
                                        let px = (local.x / zoom).floor().max(0.0) as usize;
                                        let py = (local.y / zoom).floor().max(0.0) as usize;
                                        self.png_import_tile_x = (px / tile_width).min(max_tile_x);
                                        self.png_import_tile_y = (py / self.png_import_rows_per_slice.max(1))
                                            .min(max_tile_y);
                                    }
                                }

                                let sx = self.png_import_tile_x * tile_width;
                                let sy = self.png_import_tile_y * self.png_import_rows_per_slice.max(1);
                                let ex = (sx + tile_width).min(self.png_import_width);
                                let ey = (sy + self.png_import_rows_per_slice.max(1)).min(self.png_import_height);

                                let sel_rect = egui::Rect::from_min_max(
                                    image_response.rect.min + egui::vec2(sx as f32 * zoom, sy as f32 * zoom),
                                    image_response.rect.min + egui::vec2(ex as f32 * zoom, ey as f32 * zoom),
                                );
                                ui.painter().rect_stroke(
                                    sel_rect,
                                    0.0,
                                    egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                );
                            });
                        }

                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Slice kopieren (HEX)").clicked() {
                                self.clipboard = slice.clone();
                                let hex = Self::clipboard_hex(&slice);
                                self.clipboard_desc = format!(
                                    "PNG slice tx={} ty={} {}x{}",
                                    self.png_import_tile_x,
                                    self.png_import_tile_y,
                                    tile_width,
                                    self.png_import_rows_per_slice
                                );
                                ui.ctx().output_mut(|o| o.copied_text = hex.clone());
                                self.copy_to_linux_primary_selection(&hex);
                                self.status = format!("PNG-Slice kopiert: {} byte(s)", slice.len());
                            }

                            let can_paste_inspector = self
                                .selected_ro_addr
                                .map(|start| start + slice.len() <= self.data.ro_data.len())
                                .unwrap_or(false);
                            if ui
                                .add_enabled(can_paste_inspector, egui::Button::new("In Inspector einfuegen"))
                                .clicked()
                            {
                                if let Some(start) = self.selected_ro_addr {
                                    if let Err(e) = self.paste_bytes_into_inspector(start, &slice) {
                                        self.status = format!("Inspector-Paste fehlgeschlagen: {e}");
                                    }
                                }
                            }

                            let can_paste_work = self
                                .selected_work_addr
                                .map(|start| start + slice.len() <= self.data.work_data.len())
                                .unwrap_or(false);
                            if ui
                                .add_enabled(can_paste_work, egui::Button::new("In Workbench einfuegen"))
                                .clicked()
                            {
                                if let Some(start) = self.selected_work_addr {
                                    if let Err(e) = self.paste_bytes_into_work(start, &slice) {
                                        self.status = format!("Workbench-Paste fehlgeschlagen: {e}");
                                    }
                                }
                            }
                        });
                    } else {
                        ui.separator();
                        ui.label("Noch kein PNG geladen.");
                    }
                });
            self.png_import_window_open = open;
        }

        if self.pending_action.is_some() {
            if self.pending_action_armed {
                self.pending_action_armed = false;
                ctx.request_repaint();
            } else {
                if let Some(label) = self.busy_action.clone() {
                    self.log_action(format!("Action execute: {label}"));
                }
                if let Err(e) = self.execute_deferred_action() {
                    self.log_action(format!("Action error: {e}"));
                    if self.status.starts_with("Laufend:") {
                        self.status = format!("Aktion fehlgeschlagen: {e}");
                    }
                }
                self.is_busy = false;
                self.busy_action = None;
                ctx.request_repaint();
            }
        }
    }
}

impl FlashBangGuiApp {
    fn draw_hex_dump(&mut self, ui: &mut egui::Ui) {
        self.ensure_chip_buffers();

        ui.horizontal_wrapped(|ui| {
            ui.label("Color:");
            if ui
                .selectable_label(self.diff_foreground_enabled, "Diff")
                .clicked()
            {
                self.diff_foreground_enabled = !self.diff_foreground_enabled;
            }
            if ui
                .selectable_label(self.palette_background_enabled, "Palette")
                .clicked()
            {
                self.palette_background_enabled = !self.palette_background_enabled;
            }
            ui.separator();
            ui.label("Input Mode:");
            let active_input_mode = self.pane_input_mode(self.active_pane);
            let mode_label = match active_input_mode {
                CharacterMode::Hex => "Hex (Cursor im Hex-Bereich)",
                CharacterMode::Ascii => "ASCII (Cursor im ASCII-Bereich)",
            };
            ui.monospace(mode_label);
            ui.separator();
            ui.checkbox(&mut self.show_sector_boundaries, "Show Sector Boundaries");
            ui.checkbox(&mut self.allow_flash_gray, "Allow Flash on gray");
            ui.checkbox(&mut self.auto_fetch, "Auto-Fetch");
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Range Start:");
            ui.add(egui::TextEdit::singleline(&mut self.range_start_input).desired_width(58.0));
            ui.label("Len:");
            ui.add(egui::TextEdit::singleline(&mut self.range_len_input).desired_width(58.0));
            ui.label("Sector:");
            ui.add(egui::TextEdit::singleline(&mut self.sector_input).desired_width(40.0));
            ui.label("Cells/Row:");
            let row_resp = ui.add(
                egui::DragValue::new(&mut self.bytes_per_row).clamp_range(1..=256),
            );
            if row_resp.changed() {
                self.hex_scroll_y = 0.0;
            }
            ui.separator();
            ui.checkbox(&mut self.preview_window_open, "Preview Window");
            ui.checkbox(&mut self.png_import_window_open, "PNG Import");
            if ui.button("New Workbench").clicked() {
                self.log_action("Button: New Workbench");
                self.prompt_new_workbench();
            }
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
            if self.workspace_input_mode == CharacterMode::Hex {
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
        let connected = self.serial_handle.is_some();
        let chip_known = self.data.chip.is_some();
        let chip_known_size = self.data.chip.as_ref().map(|c| c.size_bytes as usize);
        let valid_range = self.parse_range_input().ok();
        let valid_sector = self.parse_sector_input().ok();

        let can_fetch_image = connected && chip_known_size.is_some();
        let can_fetch_range = connected && chip_known_size.is_some() && valid_range.is_some();
        let can_fetch_sector = connected && chip_known && valid_sector.is_some();
        let can_erase_image = connected && chip_known_size.is_some();
        let can_erase_sector = connected && chip_known && valid_sector.is_some();

        let can_copy_image = chip_known_size
            .map(|size| self.is_ro_known_range(0, size))
            .unwrap_or(false);
        let can_copy_range = valid_range
            .map(|(start, len)| self.is_ro_known_range(start, len))
            .unwrap_or(false);
        let can_copy_sector = valid_sector
            .map(|(_, start, size)| self.is_ro_known_range(start, size))
            .unwrap_or(false);

        let can_flash_image = chip_known_size
            .map(|size| self.can_flash_range(0, size))
            .unwrap_or(false);
        let can_flash_range = valid_range
            .map(|(start, len)| self.can_flash_range(start, len))
            .unwrap_or(false);
        let can_flash_sector = valid_sector
            .map(|(_, start, size)| self.can_flash_range(start, size))
            .unwrap_or(false);

        let can_load_image = !self.data.work_data.is_empty();
        let can_load_sector = !self.data.work_data.is_empty();
        let can_save_image = !self.data.work_data.is_empty() && self.workbench_dirty;
        let can_save_sector = valid_sector.is_some() && self.workbench_dirty;

        let reason_join = |reasons: Vec<&str>| -> Option<String> {
            if reasons.is_empty() {
                None
            } else {
                Some(reasons.join(" | "))
            }
        };

        let reason_fetch_image = if can_fetch_image {
            None
        } else {
            let mut r = Vec::new();
            if !connected {
                r.push("Not Connected");
            }
            if chip_known_size.is_none() {
                r.push("Kein erkannter Chip");
            }
            reason_join(r)
        };

        let reason_fetch_range = if can_fetch_range {
            None
        } else {
            let mut r = Vec::new();
            if !connected {
                r.push("Not Connected");
            }
            if chip_known_size.is_none() {
                r.push("Kein erkannter Chip");
            }
            if valid_range.is_none() {
                r.push("Ungueltige Range-Eingabe");
            }
            reason_join(r)
        };

        let reason_fetch_sector = if can_fetch_sector {
            None
        } else {
            let mut r = Vec::new();
            if !connected {
                r.push("Not Connected");
            }
            if !chip_known {
                r.push("Kein erkannter Chip");
            }
            if valid_sector.is_none() {
                r.push("Ungueltige Sektor-Eingabe");
            }
            reason_join(r)
        };

        let reason_erase_image = if can_erase_image {
            None
        } else {
            let mut r = Vec::new();
            if !connected {
                r.push("Not Connected");
            }
            if chip_known_size.is_none() {
                r.push("Kein erkannter Chip");
            }
            reason_join(r)
        };

        let reason_erase_sector = if can_erase_sector {
            None
        } else {
            let mut r = Vec::new();
            if !connected {
                r.push("Not Connected");
            }
            if !chip_known {
                r.push("Kein erkannter Chip");
            }
            if valid_sector.is_none() {
                r.push("Ungueltige Sektor-Eingabe");
            }
            reason_join(r)
        };

        let reason_copy_image = if can_copy_image {
            None
        } else {
            let mut r = Vec::new();
            if chip_known_size.is_none() {
                r.push("Kein erkannter Chip");
            }
            r.push("Inspector-Daten nicht vollstaendig (erst Fetch ausfuehren)");
            reason_join(r)
        };

        let reason_copy_range = if can_copy_range {
            None
        } else {
            let mut r = Vec::new();
            if valid_range.is_none() {
                r.push("Ungueltige Range-Eingabe");
            }
            r.push("Inspector-Range nicht gelesen (erst Fetch Range)");
            reason_join(r)
        };

        let reason_copy_sector = if can_copy_sector {
            None
        } else {
            let mut r = Vec::new();
            if valid_sector.is_none() {
                r.push("Ungueltige Sektor-Eingabe");
            }
            r.push("Inspector-Sektor nicht gelesen (erst Fetch Sector)");
            reason_join(r)
        };

        let reason_flash_image = chip_known_size
            .and_then(|size| self.flash_disable_reason(0, size))
            .or_else(|| if can_flash_image { None } else { Some("Kein erkannter Chip".to_string()) });
        let reason_flash_range = valid_range
            .and_then(|(start, len)| self.flash_disable_reason(start, len))
            .or_else(|| if can_flash_range { None } else { Some("Ungueltige Range-Eingabe".to_string()) });
        let reason_flash_sector = valid_sector
            .and_then(|(_, start, size)| self.flash_disable_reason(start, size))
            .or_else(|| if can_flash_sector { None } else { Some("Ungueltige Sektor-Eingabe".to_string()) });

        let reason_load_image = if can_load_image { None } else { Some("Workspace nicht verfuegbar".to_string()) };
        let reason_load_sector = if can_load_sector {
            None
        } else {
            Some("Workspace nicht verfuegbar".to_string())
        };
        let reason_save_image = if can_save_image {
            None
        } else if self.data.work_data.is_empty() {
            Some("Workspace nicht verfuegbar".to_string())
        } else {
            Some("Keine ungespeicherten Workbench-Aenderungen".to_string())
        };
        let reason_save_sector = if can_save_sector {
            None
        } else if !self.workbench_dirty {
            Some("Keine ungespeicherten Workbench-Aenderungen".to_string())
        } else {
            Some("Ungueltige Sektor-Eingabe".to_string())
        };
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
                                    ui.label("Image format:");
                                    egui::ComboBox::from_id_source("save_image_format")
                                        .selected_text(self.image_save_format.label())
                                        .show_ui(ui, |ui| {
                                            for fmt in ImageSaveFormat::ALL {
                                                ui.selectable_value(&mut self.image_save_format, fmt, fmt.label());
                                            }
                                        });
                                    ui.separator();
                                    ui.label("Sector format:");
                                    egui::ComboBox::from_id_source("save_sector_format")
                                        .selected_text(self.sector_save_format.label())
                                        .show_ui(ui, |ui| {
                                            for fmt in SectorSaveFormat::ALL {
                                                ui.selectable_value(&mut self.sector_save_format, fmt, fmt.label());
                                            }
                                        });
                                });
                                ui.separator();
                                ui.horizontal_wrapped(|ui| {
                                    if self.operation_button_enabled(
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
                                        can_fetch_image,
                                        reason_fetch_image.as_deref(),
                                        "Fetch Image (Chip -> Inspector)",
                                    ).clicked() {
                                        self.log_action("Button: Fetch Image");
                                        self.queue_action(&ctx, "Fetch Image", DeferredAction::FetchImage);
                                    }
                                    if self.operation_button_enabled(
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
                                        can_fetch_range,
                                        reason_fetch_range.as_deref(),
                                        "Fetch Range (Chip+R -> Inspector+R)",
                                    ).clicked() {
                                        self.log_action(format!(
                                            "Button: Fetch Range (start={} len={})",
                                            self.range_start_input, self.range_len_input
                                        ));
                                        match self.parse_range_input() {
                                            Ok((start, len)) => {
                                                self.queue_action(
                                                    &ctx,
                                                    "Fetch Range",
                                                    DeferredAction::FetchRange { start, len },
                                                );
                                            }
                                            Err(e) => self.status = format!("Invalid range: {e}"),
                                        }
                                    }
                                    if self.operation_button_enabled(
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
                                        can_fetch_sector,
                                        reason_fetch_sector.as_deref(),
                                        "Fetch Sector (Chip+S -> Inspector+S)",
                                    ).clicked() {
                                        self.log_action(format!("Button: Fetch Sector (sector={})", self.sector_input));
                                        if let Some((_idx, start, size)) = valid_sector {
                                            self.queue_action(
                                                &ctx,
                                                "Fetch Sector",
                                                DeferredAction::FetchSector { start, size },
                                            );
                                        } else {
                                            self.status = "Invalid sector: no valid sector selected".to_string();
                                        }
                                    }
                                    if self.operation_button_enabled(
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
                                        can_erase_image,
                                        reason_erase_image.as_deref(),
                                        "Erase Image (Chip -> Trash)",
                                    ).clicked() {
                                        self.log_action("Button: Erase Image");
                                        self.queue_action(&ctx, "Erase Image", DeferredAction::EraseImage);
                                    }
                                    if self.operation_button_enabled(
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
                                        can_erase_sector,
                                        reason_erase_sector.as_deref(),
                                        "Erase Sector (Chip+S -> Trash)",
                                    ).clicked() {
                                        self.log_action(format!("Button: Erase Sector (sector={})", self.sector_input));
                                        if let Some((_idx, start, _size)) = valid_sector {
                                            self.queue_action(
                                                &ctx,
                                                "Erase Sector",
                                                DeferredAction::EraseSector { start },
                                            );
                                        } else {
                                            self.status = "Invalid sector: no valid sector selected".to_string();
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
                                if self.operation_button_enabled(
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
                                    can_copy_image,
                                    reason_copy_image.as_deref(),
                                    "Copy Image (Inspector -> Workbench)",
                                ).clicked() {
                                    self.log_action("Button: Copy Image");
                                    if let Some(size) = self.chip_size() {
                                        if let Err(e) = self.copy_ro_into_work(0, size) {
                                            self.status = format!("Copy all failed: {e}");
                                        }
                                    }
                                }
                                if self.operation_button_enabled(
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
                                    can_copy_sector,
                                    reason_copy_sector.as_deref(),
                                    "Copy Sector (Inspector+S -> Workbench+S)",
                                ).clicked() {
                                    self.log_action(format!("Button: Copy Sector (sector={})", self.sector_input));
                                    if let Some((_idx, start, size)) = valid_sector {
                                        if let Err(e) = self.copy_ro_into_work(start, size) {
                                            self.status = format!("Copy sector failed: {e}");
                                        }
                                    } else {
                                        self.status = "Invalid sector: no valid sector selected".to_string();
                                    }
                                }
                                if self.operation_button_enabled(
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
                                    can_copy_range,
                                    reason_copy_range.as_deref(),
                                    "Copy Range (Inspector+R -> Workbench+R)",
                                ).clicked() {
                                    self.log_action(format!(
                                        "Button: Copy Range (start={} len={})",
                                        self.range_start_input, self.range_len_input
                                    ));
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

                                if self.operation_button_enabled(
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
                                    can_flash_image,
                                    reason_flash_image.as_deref(),
                                    "Flash Image (Chip <- Workbench)",
                                ).clicked() {
                                    self.log_action("Button: Flash Image");
                                    if self.chip_size().is_some() {
                                        self.queue_action(&ctx, "Flash Image", DeferredAction::FlashImage);
                                    }
                                }
                                if self.operation_button_enabled(
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
                                    can_flash_sector,
                                    reason_flash_sector.as_deref(),
                                    "Flash Sector (Chip+S <- Workbench+S)",
                                ).clicked() {
                                    self.log_action(format!("Button: Flash Sector (sector={})", self.sector_input));
                                    if let Some((_idx, start, size)) = valid_sector {
                                        self.queue_action(
                                            &ctx,
                                            "Flash Sector",
                                            DeferredAction::FlashSector { start, size },
                                        );
                                    } else {
                                        self.status = "Invalid sector: no valid sector selected".to_string();
                                    }
                                }
                                if self.operation_button_enabled(
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
                                    can_flash_range,
                                    reason_flash_range.as_deref(),
                                    "Flash Range (Chip+R <- Workbench+R)",
                                ).clicked() {
                                    self.log_action(format!(
                                        "Button: Flash Range (start={} len={})",
                                        self.range_start_input, self.range_len_input
                                    ));
                                    match self.parse_range_input() {
                                        Ok((start, len)) => {
                                            self.queue_action(
                                                &ctx,
                                                "Flash Range",
                                                DeferredAction::FlashRange { start, len },
                                            );
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
                                    if self.operation_button_enabled(
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
                                        can_load_image,
                                        reason_load_image.as_deref(),
                                        "Load Image (Disk -> Workbench)",
                                    ).clicked() {
                                            self.log_action("Button: Load Image");
                                        if self.choose_open_file() {
                                            if let Err(e) = self.load_file_into_work(0, None) {
                                                self.status = e;
                                            }
                                        } else {
                                            self.status = "Load cancelled".to_string();
                                        }
                                    }
                                    if self.operation_button_enabled(
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
                                        can_load_sector,
                                        reason_load_sector.as_deref(),
                                        "Load Sector (Disk+S -> Workbench+S)",
                                    ).clicked() {
                                            self.log_action(format!("Button: Load Sector (sector={})", self.sector_input));
                                        if self.choose_open_file() {
                                            let path = PathBuf::from(self.file_path_input.trim());
                                            let default_start = Self::infer_start_from_filename(&path)
                                                .or_else(|| self.parse_sector_input().ok().map(|(_, start, _)| start))
                                                .unwrap_or(0);
                                            let default_start_text = format!("0x{default_start:05X}");
                                            if let Some(start_text) = input_box(
                                                "Load Sector Position",
                                                "Startadresse fuer das Laden dieses Files (Hex oder Dezimal)",
                                                &default_start_text,
                                            ) {
                                                match Self::parse_int_input(&start_text) {
                                                    Ok(start) => {
                                                        if let Err(e) = self.load_file_into_work(start as usize, None) {
                                                            self.status = e;
                                                        }
                                                    }
                                                    Err(e) => {
                                                        self.status = format!("Invalid load position: {e}");
                                                    }
                                                }
                                            } else {
                                                self.status = "Load cancelled".to_string();
                                            }
                                        } else {
                                            self.status = "Load cancelled".to_string();
                                        }
                                    }
                                    if self.operation_button_enabled(
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
                                        can_save_image,
                                        reason_save_image.as_deref(),
                                        "Save Image (Workbench -> Disk)",
                                    ).clicked() {
                                            self.log_action("Button: Save Image");
                                        self.save_format_dialog = Some(SaveFormatDialogState::Image);
                                    }
                                    if self.operation_button_enabled(
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
                                        can_save_sector,
                                        reason_save_sector.as_deref(),
                                        "Save Sector (Workbench+S -> Disk+S)",
                                    ).clicked() {
                                            self.log_action(format!("Button: Save Sector (sector={})", self.sector_input));
                                        if let Some((_idx, start, size)) = valid_sector {
                                            self.save_format_dialog =
                                                Some(SaveFormatDialogState::Sector { start, size });
                                        } else {
                                            self.status = "Invalid sector: no valid sector selected".to_string();
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
            name: "SST39SF040".to_string(),
            size_bytes: 512 * 1024,
            sector_size: 4096,
            driver_id: "sst39-default".to_string(),
        });
        app.ensure_chip_buffers();
        app.init_workbench(512 * 1024);
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
    fn decodes_clipboard_hex_with_prefixes_and_separators() {
        let bytes = FlashBangGuiApp::decode_clipboard_hex("0xDE, 0xAD 0xBE\n0xEF")
            .expect("clipboard hex should decode");
        assert_eq!(bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }
}
