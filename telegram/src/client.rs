use botkit_core::BotError;
use futures_lite::io::AsyncReadExt;
use zenwave::Client;

use crate::types::{BotCommand, ReplyMarkup};

const API_BASE: &str = "https://api.telegram.org";

/// Telegram REST API client
#[derive(Clone)]
pub struct TelegramClient {
    token: String,
}

impl TelegramClient {
    /// Create a new Telegram client
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }

    /// Get the bot token
    pub fn token(&self) -> &str {
        &self.token
    }

    fn api_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", API_BASE, self.token, method)
    }

    /// Send a text message
    pub async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        reply_markup: Option<ReplyMarkup>,
    ) -> Result<(), BotError> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
        });

        if let Some(markup) = reply_markup {
            body["reply_markup"] = serde_json::to_value(markup).unwrap_or_default();
        }

        let mut client = zenwave::client();
        let response = client
            .post(&self.api_url("sendMessage"))
            .json_body(&body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Failed to send message: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Edit a message
    pub async fn edit_message_text(
        &self,
        chat_id: i64,
        message_id: i64,
        text: &str,
        reply_markup: Option<ReplyMarkup>,
    ) -> Result<(), BotError> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "text": text,
        });

        if let Some(markup) = reply_markup {
            body["reply_markup"] = serde_json::to_value(markup).unwrap_or_default();
        }

        let mut client = zenwave::client();
        let response = client
            .post(&self.api_url("editMessageText"))
            .json_body(&body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Failed to edit message: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Answer a callback query
    pub async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
        show_alert: bool,
    ) -> Result<(), BotError> {
        let mut body = serde_json::json!({
            "callback_query_id": callback_query_id,
            "show_alert": show_alert,
        });

        if let Some(text) = text {
            body["text"] = serde_json::json!(text);
        }

        let mut client = zenwave::client();
        let response = client
            .post(&self.api_url("answerCallbackQuery"))
            .json_body(&body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Failed to answer callback query: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Set webhook URL
    pub async fn set_webhook(&self, url: &str) -> Result<(), BotError> {
        let body = serde_json::json!({
            "url": url,
        });

        let mut client = zenwave::client();
        let response = client
            .post(&self.api_url("setWebhook"))
            .json_body(&body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Failed to set webhook: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Delete webhook
    pub async fn delete_webhook(&self) -> Result<(), BotError> {
        let body = serde_json::json!({});

        let mut client = zenwave::client();
        let response = client
            .post(&self.api_url("deleteWebhook"))
            .json_body(&body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Failed to delete webhook: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Get updates using long polling
    pub async fn get_updates(
        &self,
        offset: Option<i64>,
        timeout: Option<u32>,
    ) -> Result<Vec<crate::types::Update>, BotError> {
        let mut body = serde_json::json!({});

        if let Some(offset) = offset {
            body["offset"] = serde_json::json!(offset);
        }
        if let Some(timeout) = timeout {
            body["timeout"] = serde_json::json!(timeout);
        }

        let mut client = zenwave::client();
        let response = client
            .post(&self.api_url("getUpdates"))
            .json_body(&body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Failed to get updates: {}",
                response.status()
            )));
        }

        #[derive(serde::Deserialize)]
        struct Response {
            result: Vec<crate::types::Update>,
        }

        let body_str = response
            .into_body()
            .into_string()
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        let parsed: Response =
            serde_json::from_str(&body_str).map_err(|e| BotError::Api(e.to_string()))?;

        Ok(parsed.result)
    }

    /// Send a chat action (typing, uploading, etc.)
    pub async fn send_chat_action(&self, chat_id: i64, action: &str) -> Result<(), BotError> {
        let body = serde_json::json!({
            "chat_id": chat_id,
            "action": action,
        });

        let mut client = zenwave::client();
        let response = client
            .post(&self.api_url("sendChatAction"))
            .json_body(&body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Failed to send chat action: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Set bot commands for the menu
    ///
    /// Registers commands with Telegram so they appear in the command menu.
    pub async fn set_my_commands(&self, commands: &[BotCommand]) -> Result<(), BotError> {
        let body = serde_json::json!({
            "commands": commands,
        });

        let mut client = zenwave::client();
        let response = client
            .post(&self.api_url("setMyCommands"))
            .json_body(&body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Failed to set commands: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Send a document/file
    pub async fn send_document(
        &self,
        chat_id: i64,
        mut file: async_fs::File,
        filename: Option<&str>,
        caption: Option<&str>,
    ) -> Result<(), BotError> {
        use zenwave::multipart::{Multipart, MultipartPart};

        // Read file contents
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)
            .await
            .map_err(|e| BotError::Other(e.to_string()))?;

        let filename = filename.unwrap_or("file");

        // Build multipart form
        let mut multipart = Multipart::new();

        // chat_id field
        multipart.push(MultipartPart::text("chat_id", chat_id.to_string()));

        // caption field (if provided)
        if let Some(caption) = caption {
            multipart.push(MultipartPart::text("caption", caption));
        }

        // document field (file)
        multipart.push(MultipartPart::binary(
            "document",
            filename.to_owned(),
            "application/octet-stream",
            contents,
        ));

        let (boundary, body) = multipart.encode();
        let content_type = format!("multipart/form-data; boundary={}", boundary);

        let mut client = zenwave::client();
        let response = client
            .post(&self.api_url("sendDocument"))
            .header("Content-Type", content_type)
            .bytes_body(body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Failed to send document: {}",
                response.status()
            )));
        }

        Ok(())
    }
}
