use std::io;
use std::time::Duration;
use std::env;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Paragraph, Wrap, Gauge,
    },
    Frame, Terminal,
};

use hyprsechy::{config::Config, session::SessionManager, types::SessionState};

#[derive(Debug, Clone, PartialEq)]
enum AppState {
    Loading,
    ShowSession,
    Restoring,
    Complete,
    Cancelled,
}

#[derive(Debug, Clone)]
struct App {
    state: AppState,
    session: Option<SessionState>,
    selected: bool,
    progress: f64,
    status_message: String,
    error_message: Option<String>,
}

impl App {
    fn new() -> Self {
        Self {
            state: AppState::Loading,
            session: None,
            selected: true,
            progress: 0.0,
            status_message: "Loading previous session...".to_string(),
            error_message: None,
        }
    }

    fn toggle_selection(&mut self) {
        if self.state == AppState::ShowSession {
            self.selected = !self.selected;
        }
    }

    fn confirm_selection(&mut self) -> bool {
        match self.state {
            AppState::ShowSession => {
                if self.selected {
                    self.state = AppState::Restoring;
                    self.status_message = "Restoring session...".to_string();
                    true
                } else {
                    self.state = AppState::Cancelled;
                    self.status_message = "Session restore cancelled".to_string();
                    false
                }
            }
            _ => false
        }
    }

    fn set_session(&mut self, session: SessionState) {
        self.session = Some(session);
        self.state = AppState::ShowSession;
        self.status_message = "Previous session found. Do you want to restore it?".to_string();
    }

    fn set_no_session(&mut self) {
        self.state = AppState::Complete;
        self.status_message = "No previous session found. Starting fresh.".to_string();
    }

    fn set_error(&mut self, error: String) {
        self.error_message = Some(error);
        self.state = AppState::Complete;
        self.status_message = "Error loading session. Starting fresh.".to_string();
    }

    fn update_progress(&mut self, progress: f64) {
        self.progress = progress;
        if progress >= 100.0 {
            self.state = AppState::Complete;
            self.status_message = "Session restored successfully!".to_string();
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check if we're running in Hyprland
    if env::var("HYPRLAND_INSTANCE_SIGNATURE").is_err() {
        eprintln!("Not running in Hyprland environment");
        std::process::exit(1);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load session in background
    let config = Config::load().unwrap_or_default();
    let session_manager = SessionManager::new(config).await?;

    // Load session data
    match session_manager.load_session().await {
        Ok(Some(session)) => {
            app.set_session(session);
        }
        Ok(None) => {
            app.set_no_session();
        }
        Err(e) => {
            app.set_error(format!("Failed to load session: {}", e));
        }
    }

    loop {
        terminal.draw(|f| ui(f, app))?;

        // Auto-close after completion
        if matches!(app.state, AppState::Complete | AppState::Cancelled) {
            tokio::time::sleep(Duration::from_secs(2)).await;
            break;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match app.state {
                        AppState::ShowSession => {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => {
                                    app.state = AppState::Cancelled;
                                    break;
                                }
                                KeyCode::Up | KeyCode::Down | KeyCode::Tab => {
                                    app.toggle_selection();
                                }
                                KeyCode::Enter | KeyCode::Char(' ') => {
                                    let should_restore = app.confirm_selection();
                                    if should_restore {
                                        if let Some(ref session) = app.session.clone() {
                                            // Trigger actual restoration
                                            eprintln!("TUI: Starting restoration via ENTER...");
                                            if let Err(e) = session_manager.restore_session(session).await {
                                                eprintln!("TUI: Restoration failed: {}", e);
                                                app.set_error(format!("Restore failed: {}", e));
                                            } else {
                                                eprintln!("TUI: Restoration completed successfully");
                                                app.update_progress(100.0);
                                            }
                                        } else {
                                            eprintln!("TUI: No session data found");
                                        }
                                    }
                                    if !should_restore {
                                        break;
                                    }
                                }
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    app.selected = true;
                                    let should_restore = app.confirm_selection();
                                    if should_restore {
                                        if let Some(ref session) = app.session.clone() {
                                            // Trigger actual restoration
                                            eprintln!("TUI: Starting restoration...");
                                            if let Err(e) = session_manager.restore_session(session).await {
                                                eprintln!("TUI: Restoration failed: {}", e);
                                                app.set_error(format!("Restore failed: {}", e));
                                            } else {
                                                eprintln!("TUI: Restoration completed successfully");
                                                app.update_progress(100.0);
                                            }
                                        } else {
                                            eprintln!("TUI: No session data found");
                                        }
                                    } else {
                                        eprintln!("TUI: Restoration not confirmed");
                                    }
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') => {
                                    app.selected = false;
                                    app.confirm_selection();
                                    break;
                                }
                                _ => {}
                            }
                        }
                        AppState::Loading | AppState::Restoring => {
                            // Allow escape during these states
                            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                                app.state = AppState::Cancelled;
                                break;
                            }
                        }
                        _ => {
                            break;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Title
    render_title(f, chunks[0]);

    // Main content
    match app.state {
        AppState::Loading => render_loading(f, chunks[1]),
        AppState::ShowSession => render_session_details(f, app, chunks[1]),
        AppState::Restoring => render_restoring(f, app, chunks[1]),
        AppState::Complete | AppState::Cancelled => render_complete(f, app, chunks[1]),
    }

    // Status bar
    render_status(f, app, chunks[2]);
}

fn render_title(f: &mut Frame, area: ratatui::layout::Rect) {
    let title = Paragraph::new("Hyprsechy - Session Restore")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        );
    f.render_widget(title, area);
}

fn render_loading(f: &mut Frame, area: ratatui::layout::Rect) {
    let loading = Paragraph::new("Loading previous session data...")
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Please wait"),
        );
    f.render_widget(loading, area);
}

fn render_session_details(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if let Some(ref session) = app.session {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);

        // Session details
        let mut details = Vec::new();
        details.push(Line::from(vec![
            Span::styled("Session Timestamp: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{}", session.timestamp),
                Style::default().fg(Color::White),
            ),
        ]));
        details.push(Line::from(""));

        details.push(Line::from(vec![
            Span::styled("Workspaces:", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]));

        for workspace in &session.workspaces {
            details.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("Workspace {}: ", workspace.id), Style::default().fg(Color::Yellow)),
                Span::styled(format!("{} windows", workspace.windows.len()), Style::default().fg(Color::White)),
            ]));

            for window in &workspace.windows {
                details.push(Line::from(vec![
                    Span::raw("    • "),
                    Span::styled(&window.class, Style::default().fg(Color::Magenta)),
                    Span::raw(" - "),
                    Span::styled(&window.title, Style::default().fg(Color::Gray)),
                ]));
            }
        }

        details.push(Line::from(""));
        details.push(Line::from(vec![
            Span::styled("Applications:", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]));

        for app in &session.uwsm_apps {
            details.push(Line::from(vec![
                Span::raw("  • "),
                Span::styled(&app.class, Style::default().fg(Color::Magenta)),
                Span::raw(" on workspace "),
                Span::styled(app.workspace_id.to_string(), Style::default().fg(Color::Yellow)),
            ]));
        }

        let details_widget = Paragraph::new(details)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Session Details"),
            )
            .wrap(Wrap { trim: true });

        f.render_widget(details_widget, chunks[0]);

        // Choice buttons
        render_choice_buttons(f, app, chunks[1]);
    }
}

fn render_choice_buttons(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Min(0),
        ])
        .split(area);

    // Yes button
    let yes_style = if app.selected {
        Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };

    let yes_button = Paragraph::new("YES - Restore Session")
        .style(yes_style)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if app.selected { Style::default().fg(Color::Green) } else { Style::default() }),
        );

    f.render_widget(yes_button, chunks[0]);

    // No button
    let no_style = if !app.selected {
        Style::default().bg(Color::Red).fg(Color::Black).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Red)
    };

    let no_button = Paragraph::new("NO - Start Fresh")
        .style(no_style)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if !app.selected { Style::default().fg(Color::Red) } else { Style::default() }),
        );

    f.render_widget(no_button, chunks[1]);

    // Instructions
    let instructions = Paragraph::new("Use ↑ ↓ or Tab to select, Enter/Space to confirm\nOr press Y for Yes, N for No")
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(instructions, chunks[2]);
}

fn render_restoring(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    let message = Paragraph::new("Restoring your session...")
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center);
    f.render_widget(message, chunks[0]);

    let progress_bar = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Progress"))
        .gauge_style(Style::default().fg(Color::Green))
        .percent(app.progress as u16);
    f.render_widget(progress_bar, chunks[1]);

    let details = Paragraph::new("Please wait while applications are being restored...")
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);
    f.render_widget(details, chunks[2]);
}

fn render_complete(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let (message, color) = match app.state {
        AppState::Complete => ("Session restore completed successfully!", Color::Green),
        AppState::Cancelled => ("Session restore cancelled. Starting fresh session.", Color::Yellow),
        _ => ("Unknown state", Color::Red),
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(message, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ]),
    ];

    if let Some(ref error) = app.error_message {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Error: ", Style::default().fg(Color::Red)),
            Span::styled(error, Style::default().fg(Color::White)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("This window will close automatically...", Style::default().fg(Color::Gray)),
    ]));

    let complete = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Complete"),
        );

    f.render_widget(complete, area);
}

fn render_status(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let status = Paragraph::new(app.status_message.as_str())
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        );

    f.render_widget(status, area);
}