use std::io;
use std::io::Stdout;
use std::rc::Rc;

use clap::{ArgMatches, crate_version};
use crossterm::{event, execute};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use itertools::Itertools;
use tui::{Frame, Terminal};
use tui::backend::CrosstermBackend;
use tui::layout::{Alignment, Constraint, Direction, Layout};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::projectfs::Project;
use crate::term::app::App;
use crate::transpiler::toml::asml::Manifest;

mod app;

pub fn command(_matches: Option<&ArgMatches>) {
    enable_raw_mode().expect("could not enable terminal raw mode");
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut app = App::new();
    let res = run_term(&mut terminal, &mut app);

    disable_raw_mode().expect("could not disable terminal raw mode");
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .unwrap();
    terminal.show_cursor().unwrap();

    if let Err(error) = res {
        println!("AssemblyLift Terminal exited with error: {:?}", error.to_string());
    }
}

fn run_term(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if let KeyCode::Esc = key.code {
                return Ok(());
            }
        }
    }
}

fn ui(f: &mut Frame<CrosstermBackend<Stdout>>, app: &mut App) {
    let (project, manifest): (Rc<Project>, Manifest) = {
        let cwd = std::env::current_dir().unwrap();
        let mut manifest_path = cwd.clone();
        manifest_path.push("assemblylift.toml");

        let asml_manifest =
            Manifest::read(&manifest_path).expect("could not read assemblylift.toml");
        (
            Rc::new(Project::new(asml_manifest.project.name.clone(), Some(cwd))),
            asml_manifest,
        )
    };

    let services_items: Vec<ListItem> = manifest
        .services
        .iter()
        .map(|s| {
            ListItem::new(&*s.name)
        })
        .collect_vec();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Max(3),
                Constraint::Percentage(85),
                Constraint::Max(3),
            ]
            .as_ref(),
        )
        .split(f.size());

    let block = Block::default().borders(Borders::BOTTOM);
    let paragraph = Paragraph::new(format!("AssemblyLift Terminal\nv{}", crate_version!()))
        .style(Style::default().add_modifier(Modifier::BOLD))
        .block(block)
        .alignment(Alignment::Left);
    f.render_widget(paragraph, chunks[0]);

    let block = Block::default()
        .title(format!("Project: {}", project.name.clone()))
        .borders(Borders::ALL);
    f.render_widget(block, chunks[1]);

    let block = Block::default().borders(Borders::TOP | Borders::BOTTOM);
    let paragraph = Paragraph::new("ESC to exit")
        .style(Style::default())
        .block(block)
        .alignment(Alignment::Left);
    f.render_widget(paragraph, chunks[2]);

    let list = List::new(services_items)
        .block(Block::default().borders(Borders::ALL).title("Services"))
        .highlight_style(
            Style::default()
                .bg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    f.render_stateful_widget(list, chunks[1], &mut app.services.state);
}
