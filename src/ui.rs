use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tui_textarea::TextArea;
use std::cell::Cell;
use std::io::{self, Stdout};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen
};
use crossterm::event::{EnableBracketedPaste, DisableBracketedPaste};
use ratatui::{
    buffer::Buffer,
    layout::Rect,  
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget, Borders, Wrap},
    DefaultTerminal, Frame,
};
use ratatui::layout::{Layout, Constraint};
use tui_textarea::{Input, Key};

#[derive(Debug)]
pub struct UI {
    terminal: Option<Terminal<CrosstermBackend<Stdout>>>,
    lines: Vec<String>,
    textarea: TextArea<'static>,
    /// How many wrapped lines we've scrolled up from the bottom (0 = pinned to newest).
    scroll: u16,
    /// Largest valid `scroll`, recomputed each render so key handlers can clamp.
    max_scroll: Cell<u16>,
}

impl UI {
    pub fn new() -> Self {
        enable_raw_mode().unwrap();
        crossterm::execute!(
            io::stdout(), 
            EnterAlternateScreen, 
            EnableMouseCapture, 
            EnableBracketedPaste
        ).unwrap();
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout())).unwrap();
        let mut textarea = TextArea::default();
        textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .title("Input"),
        );
        Self { terminal: Some(terminal), lines: vec![], textarea, scroll: 0, max_scroll: Cell::new(0) }
    }

    pub fn exit(&self) {
        disable_raw_mode().unwrap();
        crossterm::execute!(
            io::stdout(), 
            LeaveAlternateScreen, 
            DisableMouseCapture, 
            DisableBracketedPaste
        ).unwrap();
    }

    pub fn draw(&mut self) {
        // Move the terminal out so the draw closure can immutably borrow `self`
        // (for `render`) without overlapping the terminal's `&mut` borrow.
        let mut terminal = self.terminal.take().expect("terminal already taken");
        let _ = terminal.draw(|frame| self.render(frame.area(), frame.buffer_mut()));
        self.terminal = Some(terminal); // todo:
    }

    /// Scroll toward older messages (clamped to the available history).
    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll = (self.scroll + amount).min(self.max_scroll.get());
    }

    /// Scroll back toward the newest message.
    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub fn send_message(&mut self) -> String {
        let prompt = self.textarea.lines().join("\n");
        self.textarea.select_all();
        self.textarea.cut();
        self.lines.push(">> ".to_string() + &prompt);
        self.scroll = 0; // jump back to the newest line
        return prompt;
    }

    pub fn handle_paste_event(&mut self, paste_text: String){
        self.textarea.insert_str(paste_text);
    }
    
    pub fn handle_input(&mut self, input: Input){
        self.textarea.input(input);
    }

    pub fn handle_reply(&mut self, reply: String){
        self.lines.push(reply);
    }
}

impl Widget for &UI {
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
            .render(input_area, buf);
    }
}