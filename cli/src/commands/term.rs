use std::io;
use std::io::Stdout;

use clap::ArgMatches;
use crossterm::{event, execute};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use tui::{Frame, Terminal};
use tui::backend::CrosstermBackend;
use tui::layout::{Constraint, Direction, Layout};
use tui::widgets::{Block, Borders};

pub fn command(_matches: Option<&ArgMatches>) {
    enable_raw_mode().expect("could not enable terminal raw mode");
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    let res = run_term(&mut terminal);

    disable_raw_mode().expect("could not disable terminal raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture).unwrap();
    terminal.show_cursor().unwrap();
}

fn run_term(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f))?;

        if let Event::Key(key) = event::read()? {
            if let KeyCode::Esc = key.code {
                return Ok(());
            }
        }
    }
}

fn ui(f: &mut Frame<CrosstermBackend<Stdout>>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage(10),
                Constraint::Percentage(80),
                Constraint::Percentage(10),
            ]
                .as_ref(),
        )
        .split(f.size());

    let block = Block::default().title("Block").borders(Borders::ALL);
    f.render_widget(block, chunks[0]);
    let block = Block::default().title("Block 2").borders(Borders::ALL);
    f.render_widget(block, chunks[2]);
}
