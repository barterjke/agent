
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen
};
use crossterm::event::EnableBracketedPaste;
use crossterm::event::DisableBracketedPaste;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use tui_textarea::{Input, Key, TextArea};
use ratatui::layout::{Layout, Constraint};

use crossterm::event::KeyEvent;
use ratatui::{
    buffer::Buffer,
    layout::Rect,  
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget, Borders, Wrap},
    DefaultTerminal, Frame,
};

use tokio::sync::mpsc;
use ollama_rs::Ollama;
use ollama_rs::generation::chat::{ChatMessage, request::ChatMessageRequest};
use std::cell::Cell;

#[derive(Debug)]
pub struct App {
    exit: bool,
    lines: Vec<String>,
    tx: Option<mpsc::Sender<AppEvent>>,
    textarea: TextArea<'static>,
    model: String,
    ollama: Ollama,
    history: Vec<ChatMessage>,
    /// How many wrapped lines we've scrolled up from the bottom (0 = pinned to newest).
    scroll: u16,
    /// Largest valid `scroll`, recomputed each render so key handlers can clamp.
    max_scroll: Cell<u16>,
}

enum AppEvent {
    OllamaResponse(String),
    OllamaError(String),
    KeyEvent(KeyEvent),
    PasteEvent(String), // 👈 Add this to capture the bulk text
    ScrollUp,
    ScrollDown,
}

impl App {
    pub fn new() -> Self {
        let ollama = Ollama::default();
        let model = "gemma4:e2b".to_string();
        let mut textarea = TextArea::default();
        textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .title("Input"),
        );
        Self {
            exit: false,
            lines: vec![],
            tx: None,
            textarea,
            model,
            ollama,
            history: vec![],
            scroll: 0,
            max_scroll: Cell::new(0),
        }
    }

    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        let (tx, mut rx) = mpsc::channel::<AppEvent>(32);
        self.tx = Some(tx.clone());
        let key_tx = tx.clone();
        tokio::spawn(async move {
            loop {
                let read_result = tokio::task::spawn_blocking(crossterm::event::read).await;
                if let Ok(Ok(event)) = read_result {
                    match event {
                        // Handle normal typing
                        crossterm::event::Event::Key(key_event) => {
                            if key_tx.send(AppEvent::KeyEvent(key_event)).await.is_err() {
                                break;
                            }
                        }
                        // Handle big clipboard pastes instantly!
                        crossterm::event::Event::Paste(paste_text) => {
                            if key_tx.send(AppEvent::PasteEvent(paste_text)).await.is_err() {
                                break;
                            }
                        }
                        // Mouse wheel scrolls the chat history
                        crossterm::event::Event::Mouse(mouse_event) => {
                            let scroll = match mouse_event.kind {
                                crossterm::event::MouseEventKind::ScrollUp => Some(AppEvent::ScrollUp),
                                crossterm::event::MouseEventKind::ScrollDown => Some(AppEvent::ScrollDown),
                                _ => None,
                            };
                            if let Some(scroll) = scroll {
                                if key_tx.send(scroll).await.is_err() {
                                    break;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        });
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            tokio::select! {
                Some(event) = rx.recv() => {
                    match event {
                        AppEvent::OllamaResponse(reply) => {
                            self.lines.push(reply.clone());
                            self.history.push(ChatMessage::assistant(reply));
                        }
                        AppEvent::OllamaError(err) => self.lines.push(format!("Error: {}", err)),
                        AppEvent::KeyEvent(key_event) => self.handle_key_event(key_event),
                        AppEvent::PasteEvent(paste_text) => self.handle_paste_event(paste_text),
                        AppEvent::ScrollUp => self.scroll_up(3),
                        AppEvent::ScrollDown => self.scroll_down(3),
                    }
                }
            }
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    fn start_ollama_task(&mut self, prompt: String){
        self.history.push(ChatMessage::user(prompt));
        let request = ChatMessageRequest::new(self.model.clone(), self.history.clone());
        let ollama = self.ollama.clone();
        // let model = self.model.clone();
        let Some(tx) = self.tx.clone() else {
            return;
        };
        tokio::spawn(async move {
            let res = ollama.send_chat_messages(request).await;
            match res {
                Ok(res) => {
                    let reply = res.message.content;
                    let _ = tx.send(AppEvent::OllamaResponse(reply)).await;
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::OllamaError(err.to_string())).await;
                }
            }
        });
    }

    fn handle_paste_event(&mut self, paste_text: String){
        self.textarea.insert_str(paste_text);
    }

    fn send_message(&mut self){
        let prompt = self.textarea.lines().join("\n");
        self.textarea.select_all();
        self.textarea.cut();
        self.lines.push(">> ".to_string() + &prompt);
        self.scroll = 0; // jump back to the newest line
        self.start_ollama_task(prompt);
    }

    /// Scroll toward older messages (clamped to the available history).
    fn scroll_up(&mut self, amount: u16) {
        self.scroll = (self.scroll + amount).min(self.max_scroll.get());
    }

    /// Scroll back toward the newest message.
    fn scroll_down(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    fn handle_key_event(&mut self, key_event: KeyEvent){
        match key_event.into() {
            Input { key: Key::Enter, .. } => {
                self.send_message();
            }
            Input { key: Key::Esc, .. } => self.exit(),
            Input { key: Key::PageUp, .. } => self.scroll_up(5),
            Input { key: Key::PageDown, .. } => self.scroll_down(5),
            input => {
                self.textarea.input(input);
            }
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [history_area, input_area] = Layout::vertical([
            Constraint::Min(1),      // history: take all remaining space
            Constraint::Length(6),   // input: 1 text row + top/bottom border
        ])
        .areas(area);
        let block = Block::bordered()
            .title(Line::from("Chat").centered())
            .title_bottom(Line::from("press Esc to exit").centered())
            // .title_bottom(instructions.centered())
            .border_set(border::THICK);

        let mut all_lines = vec![];
        all_lines.extend(self.lines.iter().map(|s| Line::from(s.as_str())));

        let paragraph = Paragraph::new(Text::from(all_lines))
            .block(block)
            .wrap(Wrap { trim: false });

        // Inner dimensions exclude the 1-cell border on each side.
        let inner_width = history_area.width.saturating_sub(2);
        let inner_height = history_area.height.saturating_sub(2);

        // Total wrapped lines vs. what fits → how far we can scroll.
        let total_lines = paragraph.line_count(inner_width) as u16;
        let max_scroll = total_lines.saturating_sub(inner_height);
        self.max_scroll.set(max_scroll);

        // `scroll` is distance from the bottom; convert to a top offset.
        let from_bottom = self.scroll.min(max_scroll);
        let top_offset = max_scroll - from_bottom;

        paragraph
            .scroll((top_offset, 0))
            .render(history_area, buf);
        self.textarea
            // .title_bottom(Line::from("press Esc to exit"))
            .render(input_area, buf);
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;
    let mut app = App::new();

    app.run(&mut term).await?;
    disable_raw_mode()?;
    crossterm::execute!(
        term.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    term.show_cursor()?;
    Ok(())
}