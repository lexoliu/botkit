use botkit_core::BotError;
use futures_lite::io::AsyncReadExt;
use serde::de::DeserializeOwned;
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

    async fn post_json<T>(&self, method: &str, body: &serde_json::Value) -> Result<T, BotError>
    where
        T: DeserializeOwned,
    {
        let mut client = zenwave::client();
        let response = client
            .post(&self.api_url(method))
            .json_body(body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        self.decode_response(method, response).await
    }

    async fn post_multipart<T>(
        &self,
        method: &str,
        content_type: String,
        body: Vec<u8>,
    ) -> Result<T, BotError>
    where
        T: DeserializeOwned,
    {
        let mut client = zenwave::client();
        let response = client
            .post(&self.api_url(method))
            .header("Content-Type", content_type)
            .bytes_body(body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        self.decode_response(method, response).await
    }

    async fn decode_response<T>(
        &self,
        method: &str,
        response: http_kit::Response,
    ) -> Result<T, BotError>
    where
        T: DeserializeOwned,
    {
        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Telegram {method} failed with HTTP {}",
                response.status()
            )));
        }

        let body = response
            .into_body()
            .into_string()
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        parse_api_response(method, &body)
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
            body["reply_markup"] = serde_json::to_value(markup)
                .map_err(|e| BotError::Other(format!("failed to serialize reply markup: {e}")))?;
        }

        let _: serde_json::Value = self.post_json("sendMessage", &body).await?;
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
            body["reply_markup"] = serde_json::to_value(markup)
                .map_err(|e| BotError::Other(format!("failed to serialize reply markup: {e}")))?;
        }

        let _: serde_json::Value = self.post_json("editMessageText", &body).await?;
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

        let _: serde_json::Value = self.post_json("answerCallbackQuery", &body).await?;
        Ok(())
    }

    /// Set webhook URL
    pub async fn set_webhook(&self, url: &str) -> Result<(), BotError> {
        let body = serde_json::json!({
            "url": url,
        });

        let _: serde_json::Value = self.post_json("setWebhook", &body).await?;
        Ok(())
    }

    /// Delete webhook
    pub async fn delete_webhook(&self) -> Result<(), BotError> {
        let body = serde_json::json!({});

        let _: serde_json::Value = self.post_json("deleteWebhook", &body).await?;
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

        self.post_json("getUpdates", &body).await
    }

    /// Send a chat action (typing, uploading, etc.)
    pub async fn send_chat_action(&self, chat_id: i64, action: &str) -> Result<(), BotError> {
        let body = serde_json::json!({
            "chat_id": chat_id,
            "action": action,
        });

        let _: serde_json::Value = self.post_json("sendChatAction", &body).await?;
        Ok(())
    }

    /// Set bot commands for the menu
    ///
    /// Registers commands with Telegram so they appear in the command menu.
    pub async fn set_my_commands(&self, commands: &[BotCommand]) -> Result<(), BotError> {
        let body = serde_json::json!({
            "commands": commands,
        });

        let _: serde_json::Value = self.post_json("setMyCommands", &body).await?;
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

        let mut contents = Vec::new();
        file.read_to_end(&mut contents)
            .await
            .map_err(|e| BotError::Other(e.to_string()))?;

        let filename = filename.unwrap_or("file");

        let mut multipart = Multipart::new();
        multipart.push(MultipartPart::text("chat_id", chat_id.to_string()));

        if let Some(caption) = caption {
            multipart.push(MultipartPart::text("caption", caption));
        }

        multipart.push(MultipartPart::binary(
            "document",
            filename.to_owned(),
            "application/octet-stream",
            contents,
        ));

        let (boundary, body) = multipart.encode();
        let content_type = format!("multipart/form-data; boundary={}", boundary);

        let _: serde_json::Value = self
            .post_multipart("sendDocument", content_type, body)
            .await?;
        Ok(())
    }
}

#[derive(Debug, serde::Deserialize)]
struct TelegramApiResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

fn parse_api_response<T>(method: &str, body: &str) -> Result<T, BotError>
where
    T: DeserializeOwned,
{
    let response: TelegramApiResponse<T> =
        serde_json::from_str(body).map_err(|e| BotError::Api(e.to_string()))?;

    if !response.ok {
        let description = response
            .description
            .unwrap_or_else(|| format!("Telegram {method} failed without description"));
        return Err(BotError::Api(description));
    }

    response
        .result
        .ok_or_else(|| BotError::Api(format!("Telegram {method} succeeded without result")))
}

#[cfg(test)]
mod tests {
    use super::parse_api_response;

    #[test]
    fn parses_successful_api_response() {
        let updates: Vec<serde_json::Value> =
            parse_api_response("getUpdates", r#"{"ok":true,"result":[{"update_id":1}]}"#).unwrap();
        assert_eq!(updates.len(), 1);
    }

    #[test]
    fn rejects_api_error_response() {
        let err = parse_api_response::<serde_json::Value>(
            "sendMessage",
            r#"{"ok":false,"description":"chat not found"}"#,
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "API request failed: chat not found");
    }
}
