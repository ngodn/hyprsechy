use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
    },
    Frame, Terminal,
};

use hyprsechy::config::Config;

#[derive(Debug, Clone)]
struct App {
    config: Config,
    selected_section: usize,
    list_state: ListState,
    editing: Option<String>,
    edit_value: String,
    sections: Vec<ConfigSection>,
    show_help: bool,
    status_message: String,
    modified: bool,
}

#[derive(Debug, Clone)]
struct ConfigSection {
    name: String,
    items: Vec<ConfigItem>,
}

#[derive(Debug, Clone)]
struct ConfigItem {
    key: String,
    value: String,
    description: String,
    editable: bool,
}

impl App {
    fn new() -> io::Result<Self> {
        let config = Config::load().unwrap_or_default();

        let sections = vec![
            ConfigSection {
                name: "General".to_string(),
                items: vec![
                    ConfigItem {
                        key: "auto_restore".to_string(),
                        value: config.general.auto_restore.to_string(),
                        description: "Automatically restore session on startup".to_string(),
                        editable: true,
                    },
                    ConfigItem {
                        key: "save_interval".to_string(),
                        value: config.general.save_interval.to_string(),
                        description: "Interval between automatic saves (seconds)".to_string(),
                        editable: true,
                    },
                    ConfigItem {
                        key: "backup_count".to_string(),
                        value: config.general.backup_count.to_string(),
                        description: "Number of session backups to keep".to_string(),
                        editable: true,
                    },
                    ConfigItem {
                        key: "verbose_logging".to_string(),
                        value: config.general.verbose_logging.to_string(),
                        description: "Enable verbose logging".to_string(),
                        editable: true,
                    },
                ],
            },
            ConfigSection {
                name: "UWSM".to_string(),
                items: vec![
                    ConfigItem {
                        key: "enabled".to_string(),
                        value: config.uwsm.enabled.to_string(),
                        description: "Enable UWSM integration".to_string(),
                        editable: true,
                    },
                    ConfigItem {
                        key: "track_systemd_units".to_string(),
                        value: config.uwsm.track_systemd_units.to_string(),
                        description: "Track systemd units for UWSM apps".to_string(),
                        editable: true,
                    },
                    ConfigItem {
                        key: "launch_via_uwsm".to_string(),
                        value: config.uwsm.launch_via_uwsm.to_string(),
                        description: "Launch applications via UWSM when possible".to_string(),
                        editable: true,
                    },
                ],
            },
            ConfigSection {
                name: "Walker".to_string(),
                items: vec![
                    ConfigItem {
                        key: "enabled".to_string(),
                        value: config.walker.enabled.to_string(),
                        description: "Enable Walker desktop entry integration".to_string(),
                        editable: true,
                    },
                    ConfigItem {
                        key: "desktop_dirs".to_string(),
                        value: config.walker.desktop_entry_dirs.join(", "),
                        description: "Desktop entry directories (comma-separated)".to_string(),
                        editable: true,
                    },
                ],
            },
            ConfigSection {
                name: "Filters".to_string(),
                items: vec![
                    ConfigItem {
                        key: "ignore_classes".to_string(),
                        value: config.filters.ignore_classes.join(", "),
                        description: "Window classes to ignore (comma-separated)".to_string(),
                        editable: true,
                    },
                    ConfigItem {
                        key: "ignore_workspaces".to_string(),
                        value: config.filters.ignore_workspaces.iter()
                            .map(|w| w.to_string()).collect::<Vec<_>>().join(", "),
                        description: "Workspace IDs to ignore (comma-separated)".to_string(),
                        editable: true,
                    },
                ],
            },
        ];

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Ok(Self {
            config,
            selected_section: 0,
            list_state,
            editing: None,
            edit_value: String::new(),
            sections,
            show_help: false,
            status_message: "Use ↑/↓ to navigate, Enter to edit, 's' to save, 'q' to quit, '?' for help".to_string(),
            modified: false,
        })
    }

    fn next_item(&mut self) {
        let total_items = self.get_total_items();
        if total_items > 0 {
            let current = self.list_state.selected().unwrap_or(0);
            let next = if current >= total_items - 1 { 0 } else { current + 1 };
            self.list_state.select(Some(next));
        }
    }

    fn previous_item(&mut self) {
        let total_items = self.get_total_items();
        if total_items > 0 {
            let current = self.list_state.selected().unwrap_or(0);
            let previous = if current == 0 { total_items - 1 } else { current - 1 };
            self.list_state.select(Some(previous));
        }
    }

    fn get_total_items(&self) -> usize {
        self.sections.iter().map(|s| s.items.len() + 1).sum() // +1 for section headers
    }

    fn get_current_item(&self) -> Option<(usize, usize)> {
        if let Some(selected) = self.list_state.selected() {
            let mut current_index = 0;
            for (section_idx, section) in self.sections.iter().enumerate() {
                if current_index == selected {
                    return None; // This is a section header
                }
                current_index += 1;

                for (item_idx, _) in section.items.iter().enumerate() {
                    if current_index == selected {
                        return Some((section_idx, item_idx));
                    }
                    current_index += 1;
                }
            }
        }
        None
    }

    fn start_editing(&mut self) {
        if let Some((section_idx, item_idx)) = self.get_current_item() {
            let item = &self.sections[section_idx].items[item_idx];
            if item.editable {
                self.editing = Some(format!("{}:{}", section_idx, item_idx));
                self.edit_value = item.value.clone();
                self.status_message = format!("Editing {} - Press Enter to save, Escape to cancel", item.key);
            }
        }
    }

    fn save_edit(&mut self) {
        if let Some(ref editing_key) = self.editing.clone() {
            let parts: Vec<&str> = editing_key.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(section_idx), Ok(item_idx)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                    if section_idx < self.sections.len() && item_idx < self.sections[section_idx].items.len() {
                        self.sections[section_idx].items[item_idx].value = self.edit_value.clone();
                        self.modified = true;
                        self.status_message = "Value updated. Press 's' to save to file.".to_string();
                    }
                }
            }
        }
        self.editing = None;
        self.edit_value.clear();
    }

    fn cancel_edit(&mut self) {
        self.editing = None;
        self.edit_value.clear();
        self.status_message = "Edit cancelled".to_string();
    }

    fn save_config(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Update config from sections
        for section in &self.sections {
            match section.name.as_str() {
                "General" => {
                    for item in &section.items {
                        match item.key.as_str() {
                            "auto_restore" => self.config.general.auto_restore = item.value.parse().unwrap_or(false),
                            "save_interval" => self.config.general.save_interval = item.value.parse().unwrap_or(60),
                            "backup_count" => self.config.general.backup_count = item.value.parse().unwrap_or(5),
                            "verbose_logging" => self.config.general.verbose_logging = item.value.parse().unwrap_or(false),
                            _ => {}
                        }
                    }
                }
                "UWSM" => {
                    for item in &section.items {
                        match item.key.as_str() {
                            "enabled" => self.config.uwsm.enabled = item.value.parse().unwrap_or(true),
                            "track_systemd_units" => self.config.uwsm.track_systemd_units = item.value.parse().unwrap_or(true),
                            "launch_via_uwsm" => self.config.uwsm.launch_via_uwsm = item.value.parse().unwrap_or(true),
                            _ => {}
                        }
                    }
                }
                "Walker" => {
                    for item in &section.items {
                        match item.key.as_str() {
                            "enabled" => self.config.walker.enabled = item.value.parse().unwrap_or(false),
                            "desktop_dirs" => {
                                self.config.walker.desktop_entry_dirs = item.value
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                            }
                            _ => {}
                        }
                    }
                }
                "Filters" => {
                    for item in &section.items {
                        match item.key.as_str() {
                            "ignore_classes" => {
                                self.config.filters.ignore_classes = item.value
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                            }
                            "ignore_workspaces" => {
                                self.config.filters.ignore_workspaces = item.value
                                    .split(',')
                                    .filter_map(|s| s.trim().parse().ok())
                                    .collect();
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        self.config.save()?;
        self.modified = false;
        self.status_message = "Configuration saved successfully!".to_string();
        Ok(())
    }

    fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new()?;
    let res = run_app(&mut terminal, &mut app);

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

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if app.editing.is_some() {
                        match key.code {
                            KeyCode::Char(c) => {
                                app.edit_value.push(c);
                            }
                            KeyCode::Backspace => {
                                app.edit_value.pop();
                            }
                            KeyCode::Enter => {
                                app.save_edit();
                            }
                            KeyCode::Esc => {
                                app.cancel_edit();
                            }
                            _ => {}
                        }
                    } else if app.show_help {
                        match key.code {
                            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                                app.toggle_help();
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Char('?') => app.toggle_help(),
                            KeyCode::Char('s') => {
                                if let Err(e) = app.save_config() {
                                    app.status_message = format!("Save failed: {}", e);
                                }
                            }
                            KeyCode::Down => app.next_item(),
                            KeyCode::Up => app.previous_item(),
                            KeyCode::Enter => app.start_editing(),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(f.area());

    // Main content
    if app.show_help {
        render_help(f, chunks[0]);
    } else {
        render_config(f, app, chunks[0]);
    }

    // Status bar
    render_status(f, app, chunks[1]);

    // Edit popup
    if app.editing.is_some() {
        render_edit_popup(f, app);
    }
}

fn render_config(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let mut list_items = Vec::new();
    let mut item_index = 0;

    for section in &app.sections {
        // Section header
        list_items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!(" {} ", section.name),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ])));
        item_index += 1;

        // Section items
        for item in &section.items {
            let style = if Some(item_index) == app.list_state.selected() {
                Style::default().bg(Color::Blue).fg(Color::White)
            } else {
                Style::default()
            };

            let display_value = if item.value.len() > 40 {
                format!("{}...", &item.value[..37])
            } else {
                item.value.clone()
            };

            list_items.push(ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{}: ", item.key), Style::default().fg(Color::Cyan)),
                Span::styled(display_value, Style::default().fg(Color::Green)),
            ])).style(style));
            item_index += 1;
        }
    }

    let list = List::new(list_items)
        .block(
            Block::default()
                .title("Hyprsechy Configuration")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        )
        .highlight_style(Style::default().bg(Color::Blue));

    f.render_stateful_widget(list, area, &mut app.list_state.clone());
}

fn render_help(f: &mut Frame, area: ratatui::layout::Rect) {
    let help_text = vec![
        Line::from("Hyprsechy Configuration TUI Help"),
        Line::from(""),
        Line::from("Navigation:"),
        Line::from("  ↑/↓     - Navigate through options"),
        Line::from("  Enter   - Edit selected value"),
        Line::from("  s       - Save configuration to file"),
        Line::from("  ?       - Toggle this help"),
        Line::from("  q       - Quit"),
        Line::from(""),
        Line::from("Editing:"),
        Line::from("  Type    - Enter new value"),
        Line::from("  Enter   - Save changes"),
        Line::from("  Escape  - Cancel editing"),
        Line::from(""),
        Line::from("Configuration Guide:"),
        Line::from("  auto_restore     - true/false"),
        Line::from("  save_interval    - seconds (number)"),
        Line::from("  backup_count     - number of backups"),
        Line::from("  verbose_logging  - true/false"),
        Line::from("  enabled options  - true/false"),
        Line::from("  lists            - comma-separated values"),
        Line::from(""),
        Line::from("Press any key to close help"),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .title("Help")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

fn render_status(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let status_style = if app.modified {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    };

    let status_text = if app.modified {
        format!("MODIFIED - {}", app.status_message)
    } else {
        app.status_message.clone()
    };

    let status = Paragraph::new(status_text)
        .style(status_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        );

    f.render_widget(status, area);
}

fn render_edit_popup(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);

    let (section_idx, item_idx) = if let Some(ref editing_key) = app.editing {
        let parts: Vec<&str> = editing_key.split(':').collect();
        if parts.len() == 2 {
            if let (Ok(s), Ok(i)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                (s, i)
            } else {
                return;
            }
        } else {
            return;
        }
    } else {
        return;
    };

    if section_idx >= app.sections.len() || item_idx >= app.sections[section_idx].items.len() {
        return;
    }

    let item = &app.sections[section_idx].items[item_idx];

    let edit_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Title
    let title = Paragraph::new(format!("Editing: {}", item.key))
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, edit_chunks[0]);

    // Input field
    let input = Paragraph::new(app.edit_value.as_str())
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Value")
                .border_style(Style::default().fg(Color::Green)),
        );
    f.render_widget(input, edit_chunks[1]);

    // Description
    let description = Paragraph::new(item.description.as_str())
        .style(Style::default().fg(Color::Gray))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Description"),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(description, edit_chunks[2]);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}