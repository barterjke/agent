use ollama_rs::Ollama;
use ollama_rs::generation::chat::ChatMessage;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use tokio::sync::mpsc;
use crate::handler::AppEvent;

#[derive(Debug)]
pub struct Agent {
    model: String,
    ollama: Ollama,
    history: Vec<ChatMessage>,
    tx: mpsc::Sender<AppEvent>,
}

impl Agent {
    pub fn new(tx: mpsc::Sender<AppEvent>) -> Self {
        let ollama = Ollama::default();
        let model = "gemma4:e2b".to_string();
        let history = vec![];   
        Self { model, ollama, history, tx }
    }

    pub fn send_message(&mut self, message: String) {
        self.history.push(ChatMessage::user(message));
        let request = ChatMessageRequest::new(self.model.clone(), self.history.clone());
        let ollama = self.ollama.clone();
        // let model = self.model.clone();
        let tx = self.tx.clone();
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

    pub fn handle_reply(&mut self, reply: String) {
        self.history.push(ChatMessage::assistant(reply));
    }
}