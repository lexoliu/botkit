use botkit_core::{BotError, FileSource};
use http_kit::Method;
use serde::de::DeserializeOwned;
use zenwave::Client;
use zenwave::multipart::{Multipart, MultipartPart};

use crate::types::{Message, User};

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

    /// The bot's own user record (`GET /users/@me`) — its id and username,
    /// needed to tell mentions of the bot from ordinary guild chatter.
    pub async fn current_user(&self) -> Result<User, BotError> {
        let response = self.request(Method::GET, "/users/@me", None).await?;
        decode("fetch bot identity", response).await
    }

    /// Send a message to a channel; returns the created message.
    pub async fn send_message(&self, channel_id: &str, content: &str) -> Result<Message, BotError> {
        let payload = serde_json::json!({ "content": content });
        self.send_message_payload(channel_id, &payload).await
    }

    /// Send a message with a full payload (embeds, components,
    /// `message_reference`, ...); returns the created message.
    pub async fn send_message_payload(
        &self,
        channel_id: &str,
        payload: &serde_json::Value,
    ) -> Result<Message, BotError> {
        let path = format!("/channels/{channel_id}/messages");
        let response = self.request(Method::POST, &path, Some(payload)).await?;
        decode("send message", response).await
    }

    /// A channel message by id; `Ok(None)` means it no longer exists —
    /// Discord pushes no deletion notice to bots, so existence checks are
    /// how deletions are learned after the fact.
    pub async fn get_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> Result<Option<Message>, BotError> {
        let path = format!("/channels/{channel_id}/messages/{message_id}");
        let response = self.request(Method::GET, &path, None).await?;
        if response.status() == http_kit::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        decode("fetch message", response).await.map(Some)
    }

    /// Edit the text of a message the bot sent.
    pub async fn edit_message(
        &self,
        channel_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<Message, BotError> {
        let path = format!("/channels/{channel_id}/messages/{message_id}");
        let payload = serde_json::json!({ "content": content });
        let response = self.request(Method::PATCH, &path, Some(&payload)).await?;
        decode("edit message", response).await
    }

    /// Delete a message — the bot's own anywhere, others' where it has
    /// `MANAGE_MESSAGES`.
    pub async fn delete_message(&self, channel_id: &str, message_id: &str) -> Result<(), BotError> {
        let path = format!("/channels/{channel_id}/messages/{message_id}");
        let response = self.request(Method::DELETE, &path, None).await?;
        check_status("delete message", response).await
    }

    /// Set the bot's own emoji reaction on a message. `emoji` is a Unicode
    /// emoji or a `name:id` custom-emoji pair.
    pub async fn react(
        &self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<(), BotError> {
        let path = format!(
            "/channels/{channel_id}/messages/{message_id}/reactions/{}/@me",
            encode_emoji(emoji),
        );
        let response = self.request(Method::PUT, &path, None).await?;
        check_status("react", response).await
    }

    /// Remove the bot's own reaction from a message.
    pub async fn clear_reaction(
        &self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<(), BotError> {
        let path = format!(
            "/channels/{channel_id}/messages/{message_id}/reactions/{}/@me",
            encode_emoji(emoji),
        );
        let response = self.request(Method::DELETE, &path, None).await?;
        check_status("clear reaction", response).await
    }

    /// Pin a message in its channel (or unpin with `unpin`). Needs pin
    /// rights in guild channels.
    pub async fn pin_message(
        &self,
        channel_id: &str,
        message_id: &str,
        unpin: bool,
    ) -> Result<(), BotError> {
        let path = format!("/channels/{channel_id}/pins/{message_id}");
        let method = if unpin { Method::DELETE } else { Method::PUT };
        let response = self.request(method, &path, None).await?;
        check_status("pin message", response).await
    }

    /// Fetch the bytes behind an attachment's CDN `url`, refusing payloads
    /// over `limit` bytes. CDN links need no authorization.
    pub async fn download(&self, url: &str, limit: usize) -> Result<Vec<u8>, BotError> {
        let response = zenwave::client()
            .method(Method::GET, url)
            .map_err(api_error)?
            .await
            .map_err(api_error)?;
        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Discord: attachment download failed (HTTP {})",
                response.status()
            )));
        }
        let bytes = response
            .into_body()
            .into_bytes()
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;
        if bytes.len() > limit {
            return Err(BotError::Api(format!(
                "Discord: attachment is {} bytes, over the {limit}-byte limit",
                bytes.len()
            )));
        }
        Ok(bytes.to_vec())
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

        check_status("respond to interaction", response).await
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

        check_status("edit response", response).await
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

        check_status("send follow-up file", response).await
    }

    /// Send a file to a channel; returns the created message.
    pub async fn send_file(
        &self,
        channel_id: &str,
        file: FileSource,
        filename: &str,
        content: Option<&str>,
    ) -> Result<Message, BotError> {
        let path = format!("/channels/{channel_id}/messages");

        let (content_type, body) = attachment_form(filename, content, file).await?;
        let mut client = zenwave::client();
        let mut request = client
            .post(format!("{API_BASE}{path}"))
            .map_err(api_error)?
            .header("Authorization", self.auth_header.as_str())
            .map_err(api_error)?
            .header("Content-Type", content_type)
            .map_err(api_error)?;
        request = request.bytes_body(body);
        let response = request.await.map_err(api_error)?;

        decode("send file", response).await
    }

    /// Trigger typing indicator in a channel
    pub async fn trigger_typing(&self, channel_id: &str) -> Result<(), BotError> {
        let path = format!("/channels/{channel_id}/typing");
        let response = self.request(Method::POST, &path, None).await?;
        check_status("trigger typing", response).await
    }

    /// Register the bot's global slash commands, replacing the existing set
    ///
    /// Discord only offers a command in its UI once it is registered;
    /// [`crate::DiscordBot`] does this on startup from the handlers you declared.
    pub async fn set_global_commands(
        &self,
        commands: &[serde_json::Value],
    ) -> Result<(), BotError> {
        let path = format!("/applications/{}/commands", self.application_id);
        let response = self
            .request(
                Method::PUT,
                &path,
                Some(&serde_json::Value::Array(commands.to_vec())),
            )
            .await?;
        check_status("register commands", response).await
    }

    /// One authenticated request against the REST API. `path` is the
    /// `API_BASE`-relative route (`/channels/…`); `body` goes out as JSON.
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<http_kit::Response, BotError> {
        let url = format!("{API_BASE}{path}");
        let mut client = zenwave::client();
        let mut request = client
            .method(method, &url)
            .map_err(api_error)?
            .header("Authorization", self.auth_header.as_str())
            .map_err(api_error)?;
        if let Some(body) = body {
            request = request.json_body(body).map_err(api_error)?;
        }
        request.await.map_err(api_error)
    }
}

/// Percent-encode an emoji for the reactions route: unreserved characters
/// plus `:` (custom emoji travel as `name:id`) pass through, everything
/// else — including every byte of a Unicode emoji — is UTF-8 encoded.
fn encode_emoji(emoji: &str) -> String {
    let mut out = String::with_capacity(emoji.len());
    for byte in emoji.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
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

/// Read a JSON response body into `T`; a non-2xx reply folds the body text
/// into the error since Discord puts the real reason there.
async fn decode<T: DeserializeOwned>(
    what: &str,
    response: http_kit::Response,
) -> Result<T, BotError> {
    let status = response.status();
    let body = response
        .into_body()
        .into_string()
        .await
        .map_err(|e| BotError::Api(e.to_string()))?;
    if !status.is_success() {
        return Err(BotError::Api(format!(
            "Discord: failed to {what} (HTTP {status}): {body}"
        )));
    }
    serde_json::from_str(&body)
        .map_err(|e| BotError::Api(format!("Discord: malformed {what} response: {e}")))
}

/// Read the response body into the error on failure; 204s carry no body.
async fn check_status(what: &str, response: http_kit::Response) -> Result<(), BotError> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let body = response
        .into_body()
        .into_string()
        .await
        .map(|b| b.to_string())
        .unwrap_or_default();
    Err(BotError::Api(format!(
        "Discord: failed to {what} (HTTP {status}){body}"
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

    #[test]
    fn emoji_route_segments_are_percent_encoded() {
        assert_eq!(super::encode_emoji("👍"), "%F0%9F%91%8D");
        // Custom emoji keep their `name:id` shape — Discord wants the colon.
        assert_eq!(super::encode_emoji("party:123"), "party:123");
    }
}
