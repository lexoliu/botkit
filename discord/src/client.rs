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
/// Acknowledge a component interaction without changing anything.
pub(crate) const DEFERRED_UPDATE_MESSAGE: u8 = 6;
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
        install_crypto_provider();

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
            .map_err(api_error)?
            .header("Authorization", self.auth_header.as_str())
            .map_err(api_error)?
            .json_body(payload)
            .map_err(api_error)?
            .await
            .map_err(api_error)?;

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
            .map_err(api_error)?
            .json_body(&body)
            .map_err(api_error)?
            .await
            .map_err(api_error)?;

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
            .map_err(api_error)?
            .header("Authorization", self.auth_header.as_str())
            .map_err(api_error)?
            .json_body(data)
            .map_err(api_error)?
            .await
            .map_err(api_error)?;

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
            .map_err(api_error)?
            .header("Content-Type", content_type)
            .map_err(api_error)?
            .bytes_body(body)
            .await
            .map_err(api_error)?;

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
            .map_err(api_error)?
            .header("Authorization", self.auth_header.as_str())
            .map_err(api_error)?
            .header("Content-Type", content_type)
            .map_err(api_error)?
            .bytes_body(body)
            .await
            .map_err(api_error)?;

        check_status("send file", response)
    }

    /// Trigger typing indicator in a channel
    pub async fn trigger_typing(&self, channel_id: &str) -> Result<(), BotError> {
        let url = format!("{API_BASE}/channels/{channel_id}/typing");

        let response = zenwave::client()
            .post(&url)
            .map_err(api_error)?
            .header("Authorization", self.auth_header.as_str())
            .map_err(api_error)?
            .await
            .map_err(api_error)?;

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
            .map_err(api_error)?
            .header("Authorization", self.auth_header.as_str())
            .map_err(api_error)?
            .json_body(&serde_json::Value::Array(commands.to_vec()))
            .map_err(api_error)?
            .await
            .map_err(api_error)?;

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

/// Make sure rustls has a process-wide crypto provider before any TLS happens.
///
/// rustls only auto-detects a provider when exactly one is compiled in. A bot
/// that talks to Matrix *and* Discord or Telegram pulls both `ring` and
/// `aws-lc-rs` into the build, and rustls then refuses to guess — it panics on
/// the first handshake. Installing one explicitly is what keeps a unified bot
/// working; whichever adapter gets there first wins, and the rest are no-ops.
fn install_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        // Fails only if another thread won the race, which is just as good.
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

fn api_error(error: zenwave::Error) -> BotError {
    BotError::Api(error.to_string())
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

#[cfg(test)]
mod tests {
    use super::DiscordClient;

    #[test]
    fn bot_tokens_use_discords_own_auth_scheme() {
        // Discord rejects `Bearer` for bot tokens; every REST call would 401.
        let client = DiscordClient::new("abc123", "app");
        assert_eq!(client.auth_header, "Bot abc123");
    }

    #[test]
    fn building_a_client_installs_a_crypto_provider() {
        // Without this, a bot that also talks to Matrix panics on its first TLS
        // handshake: both `ring` and `aws-lc-rs` end up compiled in and rustls
        // refuses to pick one for you.
        let _client = DiscordClient::new("token", "app");
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }
}
