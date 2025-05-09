use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::{
    io::{self, stdout},
    path::{Path, PathBuf},
    time::Duration,
};

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = stdout().execute(LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

pub struct FilePicker {
    root: PathBuf,
    max_size: u64,
    show_hidden: bool,
    respect_ignore: bool,
    selected_files: Vec<PathBuf>,
    current_dir: PathBuf,
    files: Vec<PathBuf>,
    list_state: ListState,
    selected_list_state: ListState,
    show_help: bool,
}

impl FilePicker {
    pub fn new(root: PathBuf, max_size: u64, show_hidden: bool, respect_ignore: bool) -> Self {
        let mut picker = Self {
            root: root.clone(),
            max_size,
            show_hidden,
            respect_ignore,
            selected_files: Vec::new(),
            current_dir: root,
            files: Vec::new(),
            list_state: ListState::default(),
            selected_list_state: ListState::default(),
            show_help: false,
        };
        picker.refresh_files().unwrap();
        picker
    }

    pub fn run(&mut self) -> Result<Vec<PathBuf>> {
        let _guard = TerminalGuard::new()?;
        let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
        terminal.clear()?;

        loop {
            terminal.draw(|f| self.ui(f))?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('?') => self.show_help = !self.show_help,
                        KeyCode::Char('x')
                            if !self.show_help && !self.selected_files.is_empty() =>
                        {
                            self.unpick_selected()
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                                self.next_selected()
                            } else {
                                self.next()
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                                self.previous_selected()
                            } else {
                                self.previous()
                            }
                        }
                        KeyCode::Enter => self.select_item()?,
                        KeyCode::Backspace => self.go_up()?,
                        _ => {}
                    }
                }
            }
        }

        terminal.clear()?;
        Ok(self.selected_files.clone())
    }

    fn ui(&self, f: &mut ratatui::Frame) {
        if self.show_help {
            self.draw_help(f);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(1),      // Current folder
                    Constraint::Percentage(80), // File list
                    Constraint::Percentage(20), // Selected files
                ]
                .as_ref(),
            )
            .split(f.area());

        // Current folder
        let current_path = self.get_relative_path(&self.current_dir);
        let folder = Paragraph::new(format!("📁 {}", current_path.display())).block(
            Block::default()
                .borders(Borders::NONE)
                .style(Style::default().fg(Color::Blue)),
        );

        f.render_widget(folder, chunks[0]);

        // File list
        let items: Vec<ListItem> = self
            .files
            .iter()
            .map(|p| {
                let style = if p.is_dir() {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default()
                };
                let icon = if p.is_dir() { "📁" } else { "📄" };
                ListItem::new(Line::from(vec![
                    Span::styled(icon, style),
                    Span::raw(" "),
                    Span::styled(p.file_name().unwrap().to_string_lossy(), style),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Files"))
            .highlight_style(Style::default().bg(Color::DarkGray));

        f.render_stateful_widget(list, chunks[1], &mut self.list_state.clone());

        // Selected files
        let selected_items: Vec<ListItem> = self
            .selected_files
            .iter()
            .map(|p| {
                let relative_path = self.get_relative_path(p);
                ListItem::new(Line::from(format!("✅ {}", relative_path.display())))
            })
            .collect();

        let selected_widget = List::new(selected_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Selected Files"),
            )
            .highlight_style(Style::default().bg(Color::DarkGray));

        f.render_stateful_widget(
            selected_widget,
            chunks[2],
            &mut self.selected_list_state.clone(),
        );
    }

    fn refresh_files(&mut self) -> Result<()> {
        self.files = self.get_files(&self.current_dir)?;
        self.list_state.select(Some(0));
        Ok(())
    }

    fn get_relative_path(&self, path: &Path) -> PathBuf {
        path.strip_prefix(&self.root).unwrap_or(path).to_path_buf()
    }

    fn get_files(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for entry in path.read_dir()? {
            let entry = entry?;
            let path = entry.path();

            // skip hidden files if not showing them
            if !self.show_hidden
                && path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }

            // skip ignored files if respecting ignore
            if self.respect_ignore {
                let path_clone = path.clone();
                let mut builder = ignore::gitignore::GitignoreBuilder::new(path_clone);
                if let Some(parent) = path.parent() {
                    builder.add(parent);
                }
                let gitignore = builder.build()?;
                if gitignore.matched(&path, false).is_ignore() {
                    continue;
                }
            }

            // skip files larger than max size
            if path.is_file() && entry.metadata()?.len() > self.max_size {
                continue;
            }

            files.push(path);
        }

        // sort directories first, then files
        files.sort_by(|a, b| {
            if a.is_dir() && !b.is_dir() {
                std::cmp::Ordering::Less
            } else if !a.is_dir() && b.is_dir() {
                std::cmp::Ordering::Greater
            } else {
                a.file_name().cmp(&b.file_name())
            }
        });

        Ok(files)
    }

    fn next_selected(&mut self) {
        let i = match self.selected_list_state.selected() {
            Some(i) => {
                if i >= self.selected_files.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.selected_list_state.select(Some(i));
    }

    fn previous_selected(&mut self) {
        let i = match self.selected_list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.selected_files.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.selected_list_state.select(Some(i));
    }

    fn next(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.files.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.files.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn unpick_selected(&mut self) {
        if let Some(selected) = self.selected_list_state.selected() {
            if selected < self.selected_files.len() {
                self.selected_files.remove(selected);
                // Adjust the selection after removal
                if self.selected_files.is_empty() {
                    self.selected_list_state.select(None);
                } else {
                    self.selected_list_state
                        .select(Some(selected.min(self.selected_files.len() - 1)));
                }
            }
        }
    }

    fn select_item(&mut self) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            let path: &PathBuf = &self.files[selected];
            if path.is_dir() {
                self.current_dir = path.clone();
                self.refresh_files()?;
            } else {
                self.selected_files.push(path.clone());
            }
        }
        Ok(())
    }

    fn go_up(&mut self) -> Result<()> {
        if let Some(parent) = self.current_dir.parent() {
            self.current_dir = parent.to_path_buf();
            self.refresh_files()?;
        }
        Ok(())
    }

    fn draw_help(&self, f: &mut ratatui::Frame) {
        let text = vec![
            Line::from("Keybindings:"),
            Line::from(""),
            Line::from(Span::styled("q", Style::default().fg(Color::Yellow))),
            Line::from("  Quit"),
            Line::from(Span::styled(
                "↓ / ↑ or j / k",
                Style::default().fg(Color::Yellow),
            )),
            Line::from("  Navigate files"),
            Line::from(Span::styled(
                "Ctrl + ↓ / ↑ or j / k",
                Style::default().fg(Color::Yellow),
            )),
            Line::from("  Navigate selected files"),
            Line::from(Span::styled("Enter", Style::default().fg(Color::Yellow))),
            Line::from("  Select/open directory"),
            Line::from(Span::styled("x", Style::default().fg(Color::Yellow))),
            Line::from("  Unselect file"),
            Line::from(Span::styled(
                "Backspace",
                Style::default().fg(Color::Yellow),
            )),
            Line::from("  Go up a directory"),
            Line::from(Span::styled("?", Style::default().fg(Color::Yellow))),
            Line::from("  Toggle this help"),
            Line::from(""),
            Line::from(format!("Root Directory: {}", self.root.display())),
        ];

        let help = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .alignment(ratatui::layout::Alignment::Left);

        let area = ratatui::layout::Rect {
            x: f.area().width / 4,
            y: f.area().height / 4,
            width: f.area().width / 2,
            height: f.area().height / 2,
        };

        f.render_widget(help, area);
    }
}
