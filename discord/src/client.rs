use botkit_core::{BotError, FileSource};
use zenwave::Client;
use zenwave::multipart::{Multipart, MultipartPart};

const API_BASE: &str = "https://discord.com/api/v10";

/// `PONG`, the only valid reply to a `PING` interaction.
pub(crate) const INTERACTION_PONG: u8 = 1;
/// Reply immediately with a visible message.
pub(crate) const CHANNEL_MESSAGE_WITH_SOURCE: u8 = 4;
/// Acknowledge now, send the real message as a follow-up.
pub(crate) const DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE: u8 = 5;
/// Message flag marking a response as visible only to the invoking user.
pub(crate) const EPHEMERAL_FLAG: u32 = 1 << 6;

/// Discord REST API client
#[derive(Clone)]
pub struct DiscordClient {
    /// Pre-rendered `Bot <token>` header value, so it isn't rebuilt per request.
    auth_header: String,
    token: String,
    application_id: String,
}

impl DiscordClient {
    /// Create a new Discord client
    pub fn new(token: impl Into<String>, application_id: impl Into<String>) -> Self {
        let token = token.into();
        Self {
            // Bot tokens use Discord's own `Bot` scheme, not `Bearer`.
            auth_header: format!("Bot {token}"),
            token,
            application_id: application_id.into(),
        }
    }

    /// Get the bot token
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Get the application ID
    pub fn application_id(&self) -> &str {
        &self.application_id
    }

    /// Send a message to a channel
    pub async fn send_message(&self, channel_id: &str, content: &str) -> Result<(), BotError> {
        let payload = serde_json::json!({ "content": content });
        self.send_message_payload(channel_id, &payload).await
    }

    /// Send a message with a full payload (embeds, components, ...)
    pub async fn send_message_payload(
        &self,
        channel_id: &str,
        payload: &serde_json::Value,
    ) -> Result<(), BotError> {
        let url = format!("{API_BASE}/channels/{channel_id}/messages");

        let response = zenwave::client()
            .post(&url)
            .header("Authorization", self.auth_header.as_str())
            .json_body(payload)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        check_status("send message", response)
    }

    /// Respond to an interaction
    ///
    /// Interaction callbacks are authorized by the interaction token itself, so
    /// no bot authorization header is sent.
    pub async fn respond_interaction(
        &self,
        interaction_id: &str,
        interaction_token: &str,
        response_type: u8,
        data: serde_json::Value,
    ) -> Result<(), BotError> {
        let url = format!("{API_BASE}/interactions/{interaction_id}/{interaction_token}/callback");
        let body = serde_json::json!({ "type": response_type, "data": data });

        let response = zenwave::client()
            .post(&url)
            .json_body(&body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        check_status("respond to interaction", response)
    }

    /// Edit the original interaction response
    pub async fn edit_original_response(
        &self,
        interaction_token: &str,
        data: &serde_json::Value,
    ) -> Result<(), BotError> {
        let url = format!(
            "{API_BASE}/webhooks/{}/{interaction_token}/messages/@original",
            self.application_id
        );

        let response = zenwave::client()
            .method(http_kit::Method::PATCH, &url)
            .header("Authorization", self.auth_header.as_str())
            .json_body(data)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        check_status("edit response", response)
    }

    /// Send a follow-up message with a file attachment for an interaction
    pub async fn send_followup_file(
        &self,
        interaction_token: &str,
        file: FileSource,
        filename: &str,
        content: Option<&str>,
    ) -> Result<(), BotError> {
        let url = format!(
            "{API_BASE}/webhooks/{}/{interaction_token}",
            self.application_id
        );

        // Follow-ups are authorized by the interaction token.
        let (content_type, body) = attachment_form(filename, content, file).await?;
        let response = zenwave::client()
            .post(&url)
            .header("Content-Type", content_type)
            .bytes_body(body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        check_status("send follow-up file", response)
    }

    /// Send a file to a channel
    pub async fn send_file(
        &self,
        channel_id: &str,
        file: FileSource,
        filename: &str,
        content: Option<&str>,
    ) -> Result<(), BotError> {
        let url = format!("{API_BASE}/channels/{channel_id}/messages");

        let (content_type, body) = attachment_form(filename, content, file).await?;
        let response = zenwave::client()
            .post(&url)
            .header("Authorization", self.auth_header.as_str())
            .header("Content-Type", content_type)
            .bytes_body(body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        check_status("send file", response)
    }

    /// Trigger typing indicator in a channel
    pub async fn trigger_typing(&self, channel_id: &str) -> Result<(), BotError> {
        let url = format!("{API_BASE}/channels/{channel_id}/typing");

        let response = zenwave::client()
            .post(&url)
            .header("Authorization", self.auth_header.as_str())
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        check_status("trigger typing", response)
    }

    /// Register the bot's global slash commands, replacing the existing set
    ///
    /// Discord only offers a command in its UI once it is registered;
    /// [`crate::DiscordBot`] does this on startup from the handlers you declared.
    pub async fn set_global_commands(
        &self,
        commands: &[serde_json::Value],
    ) -> Result<(), BotError> {
        let url = format!("{API_BASE}/applications/{}/commands", self.application_id);

        let response = zenwave::client()
            .method(http_kit::Method::PUT, &url)
            .header("Authorization", self.auth_header.as_str())
            .json_body(&serde_json::Value::Array(commands.to_vec()))
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        check_status("register commands", response)
    }
}

/// Build the `multipart/form-data` body Discord expects for one attachment.
async fn attachment_form(
    filename: &str,
    content: Option<&str>,
    file: FileSource,
) -> Result<(String, Vec<u8>), BotError> {
    let contents = file
        .read()
        .await
        .map_err(|e| BotError::Other(format!("failed to read attachment: {e}")))?;

    let mut payload = serde_json::json!({
        "attachments": [{ "id": 0, "filename": filename }]
    });
    if let Some(content) = content {
        payload["content"] = serde_json::json!(content);
    }

    let mut multipart = Multipart::new();
    multipart.push(MultipartPart::text("payload_json", payload.to_string()));
    multipart.push(MultipartPart::binary(
        "files[0]",
        filename.to_owned(),
        "application/octet-stream",
        contents,
    ));

    let (boundary, body) = multipart.encode();
    Ok((format!("multipart/form-data; boundary={boundary}"), body))
}

fn check_status(what: &str, response: http_kit::Response) -> Result<(), BotError> {
    if response.status().is_success() {
        return Ok(());
    }
    Err(BotError::Api(format!(
        "Discord: failed to {what} (HTTP {})",
        response.status()
    )))
}
