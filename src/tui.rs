use std::io;
use std::path::Path;

use anyhow::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};
use ratatui::Frame;
use time::Date;

use screenhistory::viz::{load_weekly_stats, WeeklyStats};

/// UI state for the activity history visualizer.
pub struct VisualizerState {
    /// Current week start date (Monday)
    pub current_week: Date,
    /// Loaded weekly statistics
    pub stats: WeeklyStats,
    /// Index of selected app in the sorted list (for potential future feature)
    pub scroll_offset: usize,
    /// Whether to exit the application
    pub should_exit: bool,
}

impl VisualizerState {
    /// Create a new visualizer state for the given week.
    pub async fn new(local_db_path: &Path, week_date: Date) -> Result<Self> {
        let stats = load_weekly_stats(local_db_path, week_date).await?;
        Ok(Self {
            current_week: stats.week_start,
            stats,
            scroll_offset: 0,
            should_exit: false,
        })
    }

    /// Move to the previous week.
    pub async fn prev_week(&mut self, local_db_path: &Path) -> Result<()> {
        use time::Duration;
        let new_week = self.current_week - Duration::days(7);
        self.stats = load_weekly_stats(local_db_path, new_week).await?;
        self.current_week = self.stats.week_start;
        self.scroll_offset = 0;
        Ok(())
    }

    /// Move to the next week.
    pub async fn next_week(&mut self, local_db_path: &Path) -> Result<()> {
        use time::Duration;
        let new_week = self.current_week + Duration::days(7);
        self.stats = load_weekly_stats(local_db_path, new_week).await?;
        self.current_week = self.stats.week_start;
        self.scroll_offset = 0;
        Ok(())
    }

    /// Handle keyboard input.
    pub fn handle_input(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_exit = true;
            }
            KeyCode::Up => {
                if self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                }
            }
            KeyCode::Down => {
                let max_offset = self.stats.sorted_app_names().len().saturating_sub(1);
                if self.scroll_offset < max_offset {
                    self.scroll_offset += 1;
                }
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
            }
            KeyCode::PageDown => {
                let max_offset = self.stats.sorted_app_names().len().saturating_sub(1);
                self.scroll_offset = (self.scroll_offset + 5).min(max_offset);
            }
            _ => {}
        }
    }
}

/// Main TUI application runner.
pub async fn run_tui(local_db_path: &Path, week_date: Date) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Create app state
    let mut state = VisualizerState::new(local_db_path, week_date).await?;

    // Run event loop
    loop {
        terminal.draw(|f| draw_ui(f, &state))?;

        if event::poll(std::time::Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Left => {
                        if let Err(e) = state.prev_week(local_db_path).await {
                            eprintln!("Failed to load previous week: {e}");
                        }
                    }
                    KeyCode::Right => {
                        if let Err(e) = state.next_week(local_db_path).await {
                            eprintln!("Failed to load next week: {e}");
                        }
                    }
                    _ => state.handle_input(key.code),
                }
            }
        }

        if state.should_exit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

/// Draw the UI frame.
fn draw_ui(f: &mut Frame, state: &VisualizerState) {
    let size = f.area();

    // Split into header, body, and footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Length(3), // Header
                Constraint::Min(5),    // Body
                Constraint::Length(2), // Footer
            ]
            .as_ref(),
        )
        .split(size);

    // Draw header
    draw_header(f, chunks[0], state);

    // Draw main content
    draw_body(f, chunks[1], state);

    // Draw footer
    draw_footer(f, chunks[2], state);
}

/// Draw the header with week info.
fn draw_header(f: &mut Frame, area: Rect, state: &VisualizerState) {
    use time::format_description::well_known::Rfc2822;

    let week_str = format!(
        "Week of {} - {}",
        state.current_week.format(&Rfc2822).unwrap_or_default(),
        (state.current_week + time::Duration::days(6))
            .format(&Rfc2822)
            .unwrap_or_default()
    );

    let header = Paragraph::new(week_str)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::BOTTOM));

    f.render_widget(header, area);
}

/// Draw the main body with app usage visualization.
fn draw_body(f: &mut Frame, area: Rect, state: &VisualizerState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage(20), // App list sidebar
                Constraint::Percentage(80), // Main visualization
            ]
            .as_ref(),
        )
        .split(area);

    draw_app_list(f, chunks[0], state);
    draw_usage_visualization(f, chunks[1], state);
}

/// Draw the app list sidebar.
fn draw_app_list(f: &mut Frame, area: Rect, state: &VisualizerState) {
    let apps = state.stats.sorted_app_names();
    let max_items = area.height.saturating_sub(2) as usize;

    let items: Vec<ListItem> = apps
        .iter()
        .enumerate()
        .skip(state.scroll_offset)
        .take(max_items)
        .map(|(idx, app)| {
            let short_name = if app.len() > 12 {
                format!("{}...", &app[..9])
            } else {
                app.clone()
            };
            ListItem::new(short_name).style(
                if idx == state.scroll_offset {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            )
        })
        .collect();

    let list = List::new(items).block(Block::default().title("Apps").borders(Borders::ALL));
    f.render_widget(list, area);
}

/// Draw the main usage visualization (weekly grid).
fn draw_usage_visualization(f: &mut Frame, area: Rect, state: &VisualizerState) {
    let apps = state.stats.sorted_app_names();
    let max_items = area.height.saturating_sub(4) as usize;

    // Day headers
    let day_headers = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Percentage(100 / 7); 7])
        .split(Rect {
            x: area.left() + 1,
            y: area.top() + 1,
            width: area.width.saturating_sub(2),
            height: 1,
        });

    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    for (day_rect, day_name) in day_headers.iter().zip(days.iter()) {
        let day_widget =
            Paragraph::new(*day_name)
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
        f.render_widget(day_widget, *day_rect);
    }

    // Usage bars for each app
    let content_area = Rect {
        x: area.left() + 1,
        y: area.top() + 3,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(4),
    };

    let mut row_y = content_area.top();

    for (idx, app) in apps
        .iter()
        .enumerate()
        .skip(state.scroll_offset)
        .take(max_items)
    {
        if row_y >= content_area.bottom() {
            break;
        }

        // App name label (narrow column)
        let label_area = Rect {
            x: content_area.left(),
            y: row_y,
            width: 12,
            height: 1,
        };

        let short_name = if app.len() > 10 {
            format!("{}...", &app[..7])
        } else {
            app.clone()
        };

        let label = Paragraph::new(short_name).style(
            if idx == state.scroll_offset {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(get_app_color(app, idx))
            },
        );
        f.render_widget(label, label_area);

        // Day bars
        let bar_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(100 / 7); 7])
            .split(Rect {
                x: label_area.right() + 1,
                y: row_y,
                width: content_area.width.saturating_sub(14),
                height: 1,
            });

        for (day_idx, day_rect) in bar_chunks.iter().enumerate() {
            let day_offset = state.current_week + time::Duration::days(day_idx as i64);
            let duration_secs = state.stats.get_usage(app, day_offset);

            // Normalize to 0-100 for display (max 8 hours = 28800 seconds)
            let ratio = (duration_secs as f64 / 28800.0).min(1.0);

            let bar_color = get_app_color(app, idx);
            let gauge = Gauge::default()
                .gauge_style(Style::default().fg(bar_color))
                .ratio(ratio)
                .label(format!("{}h", duration_secs / 3600));

            f.render_widget(gauge, *day_rect);
        }

        row_y += 1;
    }

    // Border
    let border_block = Block::default()
        .title("Weekly Usage (hours)")
        .borders(Borders::ALL);
    f.render_widget(border_block, area);
}

/// Draw footer with help text.
fn draw_footer(f: &mut Frame, area: Rect, _state: &VisualizerState) {
    let help_text = "← → Week Navigation | ↑ ↓ Scroll | q Quit";
    let footer = Paragraph::new(help_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(footer, area);
}

/// Get a deterministic color for an app based on its name and index.
fn get_app_color(app_name: &str, _idx: usize) -> Color {
    // Simple hash-based color selection
    let hash = app_name.bytes().fold(0u32, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(b as u32)
    });

    let colors = [
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
    ];

    colors[(hash as usize) % colors.len()]
}
