use crate::ui::UI;
use crate::agent::Agent;
use crossterm::event::KeyEvent;
use std::io;
use tui_textarea::{Input, Key, TextArea};
use tokio::sync::mpsc;

pub enum AppEvent {
    OllamaResponse(String),
    OllamaError(String),
    KeyEvent(KeyEvent),
    PasteEvent(String),
    ScrollUp,
    ScrollDown,
}

pub struct Handler {
    ui: UI,
    agent: Agent,
    exit: bool,
    rx: mpsc::Receiver<AppEvent>,
    tx: mpsc::Sender<AppEvent>,
}

impl Handler {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(32);
        let ui = UI::new();
        let agent = Agent::new(tx.clone());
        let exit = false;
        Self { ui, agent, exit, rx, tx }
    }

    fn start_keyboard_listener(&self) {
        let key_tx = self.tx.clone();
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
    }

    pub async fn run(&mut self) -> io::Result<()> {
        self.start_keyboard_listener();
        while !self.exit {
            self.ui.draw();
            // terminal.draw(|frame| self.draw(frame))?;
            tokio::select! {
                Some(event) = self.rx.recv() => {
                    match event {
                        AppEvent::OllamaResponse(reply) => {
                            self.ui.handle_reply(reply.clone());
                            self.agent.handle_reply(reply);
                            
                        }
                        AppEvent::OllamaError(err) => println!("Error: {}", err), // TODO:
                        // self.lines.push(format!("Error: {}", err)),
                        AppEvent::KeyEvent(key_event) => self.handle_key_event(key_event),
                        AppEvent::PasteEvent(paste_text) => self.ui.handle_paste_event(paste_text),
                        AppEvent::ScrollUp => self.ui.scroll_up(3),
                        AppEvent::ScrollDown => self.ui.scroll_down(3),
                    }
                }
            }
        }
        self.ui.exit();
        Ok(())
    }

    

    fn handle_key_event(&mut self, key_event: KeyEvent){
        match key_event.into() {
            Input { key: Key::Enter, .. } => {
                self.send_message();
            }
            Input { key: Key::Esc, .. } => self.exit(),
            Input { key: Key::PageUp, .. } => self.ui.scroll_up(5),
            Input { key: Key::PageDown, .. } => self.ui.scroll_down(5),
            input => {
                self.ui.handle_input(input);
            }
        }
    }

    fn send_message(&mut self){
        let prompt = self.ui.send_message();
        self.agent.send_message(prompt);
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}