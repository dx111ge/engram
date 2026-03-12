use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChatRole {
    User,
    Assistant,
    System,
    ToolResult,
}
