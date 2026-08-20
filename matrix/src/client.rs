use botkit_core::BotError;
use matrix_sdk::ruma::OwnedEventId;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::{Client, Room};

/// Matrix client wrapper
///
/// Provides a simplified API for common Matrix operations.
#[derive(Clone)]
pub struct MatrixClient {
    inner: Client,
}

impl MatrixClient {
    /// Create a new Matrix client wrapper
    pub fn new(client: Client) -> Self {
        Self { inner: client }
    }

    /// Get the inner matrix-sdk Client for advanced operations
    pub fn inner(&self) -> &Client {
        &self.inner
    }

    /// Send a plain text message to a room
    pub async fn send_message(&self, room: &Room, content: &str) -> Result<OwnedEventId, BotError> {
        let msg = RoomMessageEventContent::text_plain(content);
        let response = room
            .send(msg)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;
        Ok(response.response.event_id)
    }

    /// Send a formatted (HTML) message to a room
    pub async fn send_formatted_message(
        &self,
        room: &Room,
        plain: &str,
        html: &str,
    ) -> Result<OwnedEventId, BotError> {
        let msg = RoomMessageEventContent::text_html(plain, html);
        let response = room
            .send(msg)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;
        Ok(response.response.event_id)
    }

    /// Send a typing notification
    pub async fn send_typing(&self, room: &Room, typing: bool) -> Result<(), BotError> {
        room.typing_notice(typing)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;
        Ok(())
    }

    /// Upload a file and send it to a room
    ///
    /// The MIME type is guessed from the filename; Matrix requires one, and
    /// `application/octet-stream` is the safe fallback.
    pub async fn send_file(
        &self,
        room: &Room,
        filename: &str,
        bytes: Vec<u8>,
    ) -> Result<OwnedEventId, BotError> {
        use matrix_sdk::attachment::AttachmentConfig;

        let content_type = mime_guess::from_path(filename).first_or_octet_stream();

        let response = room
            .send_attachment(filename, &content_type, bytes, AttachmentConfig::new())
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        Ok(response.event_id)
    }

    /// React to a message with an emoji
    pub async fn react(
        &self,
        room: &Room,
        event_id: &OwnedEventId,
        emoji: &str,
    ) -> Result<OwnedEventId, BotError> {
        use matrix_sdk::ruma::events::reaction::ReactionEventContent;
        use matrix_sdk::ruma::events::relation::Annotation;

        let reaction =
            ReactionEventContent::new(Annotation::new(event_id.clone(), emoji.to_string()));

        let response = room
            .send(reaction)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;
        Ok(response.response.event_id)
    }
}
