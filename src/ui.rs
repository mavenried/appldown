use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Gauge, Paragraph, Row, Table, TableState,
    },
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, LogEntry, TrackStatus};

const SPINNER: &[&str] = &["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];

fn spinner_frame(tick: u8) -> &'static str {
    SPINNER[(tick as usize / 2) % SPINNER.len()]
}

const C_ACCENT:   Color = Color::Magenta;
const C_SUCCESS:  Color = Color::Green;
const C_WARN:     Color = Color::Yellow;
const C_ERROR:    Color = Color::Red;
const C_DIM:      Color = Color::DarkGray;
const C_EMBED:    Color = Color::Cyan;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(C_ACCENT))
        .title(Line::from(vec![
            Span::styled(" appldown ", Style::default().fg(C_ACCENT).bold()),
        ]))
        .title_bottom(Line::from(vec![
            Span::styled(
                " Enter",
                Style::default().fg(C_ACCENT).bold(),
            ),
            Span::raw(":add  "),
            Span::styled("F5", Style::default().fg(C_ACCENT).bold()),
            Span::raw(":fetch  "),
            Span::styled("F6", Style::default().fg(C_ACCENT).bold()),
            Span::raw(":download  "),
            Span::styled("^L", Style::default().fg(C_ACCENT).bold()),
            Span::raw(":clear  "),
            Span::styled("q", Style::default().fg(C_ACCENT).bold()),
            Span::raw(":quit "),
        ]));

    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(8),
        ])
        .split(inner);

    draw_url_input(f, app, rows[0]);
    draw_main(f, app, rows[1]);
    draw_bottom(f, app, rows[2]);
}

fn draw_url_input(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(C_DIM))
        .title(Span::styled(" URL ", Style::default().fg(C_ACCENT)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let (text, style) = if app.input.is_empty() {
        (
            "Paste an Apple Music URL and press Enter…".to_string(),
            Style::default().fg(C_DIM).italic(),
        )
    } else {
        (app.input.clone(), Style::default())
    };

    let visible_width = inner.width as usize;
    let _char_count = app.input.chars().count();
    let scroll_start = if app.cursor >= visible_width {
        app.cursor - visible_width + 1
    } else {
        0
    };
    let displayed: String = app.input.chars().skip(scroll_start).take(visible_width).collect();
    let displayed = if app.input.is_empty() { text } else { displayed };

    f.render_widget(Paragraph::new(displayed).style(style), inner);

    if !app.input.is_empty() {
        let cursor_offset = app.input
            .chars()
            .skip(scroll_start)
            .take(app.cursor.saturating_sub(scroll_start))
            .collect::<String>()
            .width() as u16;
        f.set_cursor_position((inner.x + cursor_offset, inner.y));
    }
}

fn draw_main(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28),
            Constraint::Min(40),
        ])
        .split(area);

    draw_url_list(f, app, cols[0]);
    draw_track_table(f, app, cols[1]);
}

fn draw_url_list(f: &mut Frame, app: &App, area: Rect) {
    let title = format!(" URLs ({}) ", app.urls.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(C_DIM))
        .title(Span::styled(title, Style::default().fg(C_ACCENT)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible = inner.height as usize;
    let start = app.urls.len().saturating_sub(visible);
    let lines: Vec<Line> = app.urls[start..]
        .iter()
        .map(|u| {
            let short = shorten(u.trim_start_matches("https://"), inner.width as usize - 2);
            Line::from(vec![
                Span::styled("● ", Style::default().fg(C_DIM)),
                Span::raw(short),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_track_table(f: &mut Frame, app: &App, area: Rect) {
    let title = format!(" Tracks ({}) ", app.tracks.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(C_DIM))
        .title(Span::styled(title, Style::default().fg(C_ACCENT)));

    let inner = block.inner(area);

    let header = Row::new(vec!["#", "Artist", "Title", "Album", "Status"])
        .style(Style::default().bold().underlined())
        .height(1);

    let rows: Vec<Row> = app
        .tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let (sym, style) = status_style(&track.status, app.tick);
            Row::new(vec![
                Cell::from(format!("{}", i + 1)),
                Cell::from(shorten(&track.data.artist, 18)),
                Cell::from(shorten(&track.data.title, 26)),
                Cell::from(shorten(&track.data.album, 18)),
                Cell::from(Span::styled(sym, style)),
            ])
            .height(1)
        })
        .collect();

    let widths = [
        Constraint::Length(4),
        Constraint::Length(20),
        Constraint::Min(20),
        Constraint::Length(20),
        Constraint::Length(14),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .row_highlight_style(Style::default().reversed());

    let mut state = TableState::default();
    state.select(app.track_selected);

    f.render_stateful_widget(table, area, &mut state);
    let _ = inner;
}

fn draw_bottom(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
        ])
        .split(area);

    draw_progress(f, app, rows[0]);
    draw_log(f, app, rows[1]);
}

fn draw_progress(f: &mut Frame, app: &App, area: Rect) {
    let (done, total) = app.progress;
    let ratio = if total == 0 {
        0.0
    } else {
        done as f64 / total as f64
    };

    let label = if total == 0 {
        "idle".to_string()
    } else {
        format!("{done} / {total} tracks")
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Thick)
                .border_style(Style::default().fg(C_DIM)),
        )
        .gauge_style(Style::default().fg(C_SUCCESS).bg(Color::DarkGray))
        .ratio(ratio)
        .label(label);

    f.render_widget(gauge, area);
}

fn draw_log(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(C_DIM))
        .title(Span::styled(" Log ", Style::default().fg(C_ACCENT)));

    let inner = block.inner(area);
    let visible = inner.height as usize;

    let start = app
        .log
        .len()
        .saturating_sub(visible + app.log_scroll as usize);
    let end = app.log.len().saturating_sub(app.log_scroll as usize);

    let lines: Vec<Line> = app.log[start..end]
        .iter()
        .map(|entry| log_line(entry))
        .collect();

    f.render_widget(Paragraph::new(lines).block(block), area);
    let _ = inner;
}

fn status_style(status: &TrackStatus, tick: u8) -> (String, Style) {
    match status {
        TrackStatus::Pending => (
            "○ pending".to_string(),
            Style::default().fg(C_DIM),
        ),
        TrackStatus::Downloading => (
            format!("{} downloading", spinner_frame(tick)),
            Style::default().fg(C_WARN).bold(),
        ),
        TrackStatus::Embedding => (
            format!("{} embedding", spinner_frame(tick)),
            Style::default().fg(C_EMBED).bold(),
        ),
        TrackStatus::Done => (
            "✓ done".to_string(),
            Style::default().fg(C_SUCCESS).bold(),
        ),
        TrackStatus::Failed(_) => (
            "✗ failed".to_string(),
            Style::default().fg(C_ERROR).bold(),
        ),
    }
}

fn log_line(entry: &LogEntry) -> Line<'static> {
    match entry {
        LogEntry::Info(s) => Line::from(s.clone()),
        LogEntry::Success(s) => {
            Line::from(Span::styled(s.clone(), Style::default().fg(C_SUCCESS)))
        }
        LogEntry::Error(s) => {
            Line::from(Span::styled(s.clone(), Style::default().fg(C_ERROR)))
        }
        LogEntry::Dim(s) => {
            Line::from(Span::styled(s.clone(), Style::default().fg(C_DIM)))
        }
    }
}

fn shorten(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = chars[..max_chars.saturating_sub(1)].iter().collect();
        format!("{truncated}…")
    }
}
