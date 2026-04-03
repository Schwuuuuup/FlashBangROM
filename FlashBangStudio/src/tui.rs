use std::io;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Cell, Gauge, List, ListItem, Paragraph, Row, Table, Tabs},
    Frame, Terminal,
};

use crate::{
    report::{build_report, group_mismatches, DiffReport},
    session::{ChipId, DeviceSession, HelloInfo, MockSession},
    verify::compute_diff,
};

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct AppState {
    pub active_tab: usize,
    pub hello_info: Option<HelloInfo>,
    pub chip_id: Option<ChipId>,
    /// Raw bytes read from the device (demo: first 4 KiB).
    pub chip_data: Vec<u8>,
    /// Row offset for the hex-dump view (one row = 16 bytes).
    pub hex_scroll: usize,
    pub diff_report: Option<DiffReport>,
    pub log: Vec<String>,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn run_tui() -> io::Result<()> {
    // ---- Terminal setup ----
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal);

    // ---- Restore terminal even on error ----
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut state = AppState {
        active_tab: 0,
        hello_info: None,
        chip_id: None,
        chip_data: Vec::new(),
        hex_scroll: 0,
        diff_report: None,
        log: Vec::new(),
        status: String::new(),
    };

    // ---- Pre-populate via mock session (no hardware needed) ----
    let mut session = MockSession::new();

    match session.handshake() {
        Ok(hello) => {
            state.log.push(format!(
                "HELLO: fw={} proto={}",
                hello.fw_version, hello.protocol_version
            ));
            state.hello_info = Some(hello);
        }
        Err(e) => state.log.push(format!("HELLO error: {e}")),
    }

    match session.identify() {
        Ok(chip) => {
            state.log.push(format!(
                "ID: {} ({} KiB, {} × {} B sectors)",
                chip.name,
                chip.size_bytes / 1024,
                chip.sector_count(),
                chip.sector_size,
            ));
            state.chip_id = Some(chip);
        }
        Err(e) => state.log.push(format!("ID error: {e}")),
    }

    // Read first 4 KiB for hex-dump display (full 512 KiB would be slow in
    // a purely in-memory demo; actual serial read is bounded by line rate).
    let demo_read_len = 4096u32;
    match session.read_range(0, demo_read_len, &mut |_, _| {}) {
        Ok(data) => {
            state.log.push(format!("Read {} bytes OK (demo: first 4 KiB)", data.len()));

            // Build a reference with a few deliberate differences so the
            // diff tab shows something interesting.
            let mut reference = data.clone();
            if reference.len() > 0x100B {
                reference[0x1000] = 0xDE;
                reference[0x1001] = 0xAD;
                reference[0x1002] = 0xBE;
                reference[0x1003] = 0xEF;
                reference[0x100A] = 0xFF; // was 0x0A in actual data
            }

            let diff = compute_diff(0x0000, &reference, &data);
            let region_count = group_mismatches(&diff.mismatches).len();
            state.log.push(format!(
                "Diff vs reference: {} mismatch(es) in {} region(s)",
                diff.mismatch_count, region_count
            ));
            state.diff_report = Some(build_report(&diff));
            state.chip_data = data;
        }
        Err(e) => state.log.push(format!("Read error: {e}")),
    }

    state.status =
        " [1-3] / Tab: switch view   [↑/↓/PgUp/PgDn]: scroll hex   [q]: quit ".to_string();

    // ---- Event loop ----
    loop {
        terminal.draw(|frame| draw_ui(frame, &state))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    KeyCode::Char('1') => state.active_tab = 0,
                    KeyCode::Char('2') => state.active_tab = 1,
                    KeyCode::Char('3') => state.active_tab = 2,
                    KeyCode::Tab => state.active_tab = (state.active_tab + 1) % 3,
                    KeyCode::Up => {
                        if state.active_tab == 1 && state.hex_scroll > 0 {
                            state.hex_scroll -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if state.active_tab == 1 {
                            let max = (state.chip_data.len() / 16).saturating_sub(1);
                            if state.hex_scroll < max {
                                state.hex_scroll += 1;
                            }
                        }
                    }
                    KeyCode::PageUp => {
                        if state.active_tab == 1 {
                            state.hex_scroll = state.hex_scroll.saturating_sub(10);
                        }
                    }
                    KeyCode::PageDown => {
                        if state.active_tab == 1 {
                            let max = (state.chip_data.len() / 16).saturating_sub(1);
                            state.hex_scroll = (state.hex_scroll + 10).min(max);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn draw_ui(frame: &mut Frame, state: &AppState) {
    let area = frame.size();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tab bar
            Constraint::Min(0),    // content
            Constraint::Length(1), // status line
        ])
        .split(area);

    draw_tabs_bar(frame, chunks[0], state.active_tab);

    match state.active_tab {
        0 => draw_info_tab(frame, chunks[1], state),
        1 => draw_hex_tab(frame, chunks[1], state),
        2 => draw_diff_tab(frame, chunks[1], state),
        _ => {}
    }

    let status = Paragraph::new(state.status.as_str())
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(status, chunks[2]);
}

fn draw_tabs_bar(frame: &mut Frame, area: Rect, active: usize) {
    let titles: Vec<Line> = vec![
        Line::from("1: Chip Info"),
        Line::from("2: Hex Dump"),
        Line::from("3: Diff View"),
    ];
    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" FlashBang Studio — Demo Mode "),
        )
        .select(active)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, area);
}

// ---- Tab 1: Chip Info ----

fn draw_info_tab(frame: &mut Frame, area: Rect, state: &AppState) {
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(area);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(horiz[0]);

    // Device / chip info text
    let mut lines: Vec<Line> = vec![Line::from("")];
    if let Some(h) = &state.hello_info {
        lines.push(Line::from(format!("  FW Version   : {}", h.fw_version)));
        lines.push(Line::from(format!("  Protocol     : {}", h.protocol_version)));
        lines.push(Line::from(format!(
            "  Capabilities : {}",
            h.capabilities.join(", ")
        )));
        lines.push(Line::from(""));
    }
    if let Some(c) = &state.chip_id {
        lines.push(Line::from(format!("  Chip Model   : {}", c.name)));
        lines.push(Line::from(format!(
            "  Manufacturer : 0x{:02X}",
            c.manufacturer_id
        )));
        lines.push(Line::from(format!("  Device ID    : 0x{:02X}", c.device_id)));
        lines.push(Line::from(format!("  Driver       : {}", c.driver_id)));
        lines.push(Line::from(format!(
            "  Total Size   : {} KiB  ({} bytes)",
            c.size_bytes / 1024,
            c.size_bytes
        )));
        lines.push(Line::from(format!("  Sector Size  : {} bytes", c.sector_size)));
        lines.push(Line::from(format!("  Sector Count : {}", c.sector_count())));
    } else {
        lines.push(Line::from("  No chip identified."));
    }

    let info = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Device "));
    frame.render_widget(info, left[0]);

    // Read-progress gauge
    let pct: u16 = if state.chip_data.is_empty() { 0 } else { 100 };
    let label = if pct == 100 {
        "Complete (demo: 4 KiB)"
    } else {
        "Idle"
    };
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" Read Progress "))
        .gauge_style(Style::default().fg(Color::Green))
        .percent(pct)
        .label(label);
    frame.render_widget(gauge, left[1]);

    // Operation log
    let items: Vec<ListItem> = state.log.iter().map(|l| ListItem::new(l.as_str())).collect();
    let log = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Operation Log "))
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(log, horiz[1]);
}

// ---- Tab 2: Hex Dump ----

fn draw_hex_tab(frame: &mut Frame, area: Rect, state: &AppState) {
    let data = &state.chip_data;

    if data.is_empty() {
        let p = Paragraph::new("No data available. Run a read operation first.")
            .block(Block::default().borders(Borders::ALL).title(" Hex Dump "));
        frame.render_widget(p, area);
        return;
    }

    const BYTES_PER_ROW: usize = 16;
    let visible_rows = area.height.saturating_sub(2) as usize;
    let start_row = state.hex_scroll;
    let start_offset = start_row * BYTES_PER_ROW;

    let mut lines: Vec<Line> = Vec::with_capacity(visible_rows);
    for row in 0..visible_rows {
        let offset = start_offset + row * BYTES_PER_ROW;
        if offset >= data.len() {
            break;
        }
        let end = (offset + BYTES_PER_ROW).min(data.len());
        let slice = &data[offset..end];

        let hex_part: String = slice
            .chunks(8)
            .map(|c| {
                c.iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("  ");

        let ascii_part: String = slice
            .iter()
            .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
            .collect();

        lines.push(Line::from(format!(
            " {offset:06X}:  {hex_part:<49}  {ascii_part}"
        )));
    }

    let title = format!(
        " Hex Dump — 0x{:06X}  [↑/↓/PgUp/PgDn to scroll] ",
        start_offset
    );
    let para =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(para, area);
}

// ---- Tab 3: Diff View ----

fn draw_diff_tab(frame: &mut Frame, area: Rect, state: &AppState) {
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(32), Constraint::Min(0)])
        .split(area);

    // Summary panel
    let summary_lines = if let Some(r) = &state.diff_report {
        vec![
            Line::from(""),
            Line::from(format!("  Start Addr  : 0x{:05X}", r.start_address)),
            Line::from(format!("  Compared    : {} bytes", r.compared_len)),
            Line::from(format!("  Mismatches  : {}", r.mismatch_count)),
            Line::from(format!("  Regions     : {}", r.ranges.len())),
            Line::from(""),
            Line::from("  (reference = chip data"),
            Line::from("   with 5 bytes changed)"),
        ]
    } else {
        vec![Line::from("  No diff data.")]
    };

    let summary = Paragraph::new(summary_lines)
        .block(Block::default().borders(Borders::ALL).title(" Summary "));
    frame.render_widget(summary, horiz[0]);

    // Mismatch-regions table
    let bold_underline = Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let header = Row::new(vec![
        Cell::from("Start").style(bold_underline),
        Cell::from("End").style(bold_underline),
        Cell::from("Bytes").style(bold_underline),
    ]);

    let rows: Vec<Row> = if let Some(r) = &state.diff_report {
        r.ranges
            .iter()
            .map(|range| {
                Row::new(vec![
                    Cell::from(format!("0x{:05X}", range.start_address)),
                    Cell::from(format!("0x{:05X}", range.end_address)),
                    Cell::from(format!("{}", range.mismatch_count)),
                ])
            })
            .collect()
    } else {
        vec![]
    };

    let widths = [
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(8),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Mismatch Regions "),
        );
    frame.render_widget(table, horiz[1]);
}
