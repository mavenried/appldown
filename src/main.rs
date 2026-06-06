mod app;
mod download;
mod itunes;
mod ui;

use std::io;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, AppMsg};

fn main() -> Result<()> {
    let (tx, rx) = mpsc::channel::<AppMsg>();
    let mut app = App::new(tx);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, &mut app, rx);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    rx: mpsc::Receiver<AppMsg>,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        while let Ok(msg) = rx.try_recv() {
            app.handle_msg(msg);
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if app.handle_key(key) {
                    break;
                }
            }
        }

        app.tick = app.tick.wrapping_add(1);
    }
    Ok(())
}
