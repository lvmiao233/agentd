use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

type DynError = Box<dyn std::error::Error>;

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct StatusBar {
    pub mode: String,
    pub hint: String,
}

#[derive(Debug, Clone, Default)]
pub struct EventPanel {
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PendingApproval {
    pub id: String,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct AgentShellApp {
    input_buffer: String,
    messages: Vec<ChatMessage>,
    status_bar: StatusBar,
    event_panel: EventPanel,
    approval_queue: Vec<PendingApproval>,
}

impl AgentShellApp {
    pub fn new() -> Self {
        Self {
            input_buffer: String::new(),
            messages: Vec::new(),
            status_bar: StatusBar {
                mode: "idle".to_string(),
                hint: "enter=submit | q/esc/ctrl-c=quit".to_string(),
            },
            event_panel: EventPanel::default(),
            approval_queue: Vec::new(),
        }
    }

    fn submit_input(&mut self) {
        let input = self.input_buffer.trim().to_string();
        if !input.is_empty() {
            self.messages.push(ChatMessage {
                role: "user".to_string(),
                content: input,
            });
        }
        self.input_buffer.clear();
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> bool {
        if key_event.kind != KeyEventKind::Press {
            return true;
        }

        match key_event.code {
            KeyCode::Char('q') | KeyCode::Esc => false,
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => false,
            KeyCode::Enter => {
                self.submit_input();
                true
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
                true
            }
            KeyCode::Char(ch) => {
                self.input_buffer.push(ch);
                true
            }
            _ => true,
        }
    }

    pub fn render(&self, frame: &mut Frame<'_>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(frame.size());

        let message_lines: Vec<Line<'_>> = if self.messages.is_empty() {
            vec![Line::from("No messages yet. Type and press Enter.")]
        } else {
            self.messages
                .iter()
                .map(|message| Line::from(format!("{}: {}", message.role, message.content)))
                .collect()
        };

        let message_panel = Paragraph::new(Text::from(message_lines))
            .block(Block::default().borders(Borders::ALL).title("Messages"))
            .wrap(Wrap { trim: false });

        let input_panel = Paragraph::new(format!("> {}", self.input_buffer))
            .block(Block::default().borders(Borders::ALL).title("Input"));

        let approval_preview = self
            .approval_queue
            .first()
            .map(|approval| format!("next_approval={} ({})", approval.id, approval.summary))
            .unwrap_or_else(|| "next_approval=none".to_string());
        let status_line = format!(
            "mode={} | pending_events={} | pending_approvals={} | {} | {}",
            self.status_bar.mode,
            self.event_panel.entries.len(),
            self.approval_queue.len(),
            approval_preview,
            self.status_bar.hint
        );
        let status_bar = Paragraph::new(status_line);

        frame.render_widget(message_panel, chunks[0]);
        frame.render_widget(input_panel, chunks[1]);
        frame.render_widget(status_bar, chunks[2]);
    }
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut AgentShellApp,
) -> Result<(), DynError> {
    loop {
        terminal.draw(|frame| app.render(frame))?;

        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            if let Event::Key(key_event) = event {
                if !app.handle_key_event(key_event) {
                    return Ok(());
                }
            }
        }
    }
}

pub fn run() -> Result<(), DynError> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = AgentShellApp::new();
    let event_loop_result = run_event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    event_loop_result
}

#[cfg(test)]
#[test]
fn tui_app_handles_quit_key() {
    let mut app = AgentShellApp::new();
    let should_continue =
        app.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(!should_continue);
}
