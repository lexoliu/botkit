use botkit_core::BotError;
use zenwave::Client;

const API_BASE: &str = "https://discord.com/api/v10";

/// Discord REST API client
#[derive(Clone)]
pub struct DiscordClient {
    token: String,
    application_id: String,
}

impl DiscordClient {
    /// Create a new Discord client
    pub fn new(token: impl Into<String>, application_id: impl Into<String>) -> Self {
        Self {
            token: token.into(),
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
        let url = format!("{}/channels/{}/messages", API_BASE, channel_id);

        let body = serde_json::json!({
            "content": content
        });

        let mut client = zenwave::client();
        let response = client
            .post(&url)
            .bearer_auth(&self.token)
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

    /// Respond to an interaction
    pub async fn respond_interaction(
        &self,
        interaction_id: &str,
        interaction_token: &str,
        response_type: u8,
        data: serde_json::Value,
    ) -> Result<(), BotError> {
        let url = format!(
            "{}/interactions/{}/{}/callback",
            API_BASE, interaction_id, interaction_token
        );

        let body = serde_json::json!({
            "type": response_type,
            "data": data
        });

        let mut client = zenwave::client();
        let response = client
            .post(&url)
            .json_body(&body)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Failed to respond to interaction: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Edit the original interaction response
    pub async fn edit_original_response(
        &self,
        interaction_token: &str,
        data: serde_json::Value,
    ) -> Result<(), BotError> {
        let url = format!(
            "{}/webhooks/{}/{}/messages/@original",
            API_BASE, self.application_id, interaction_token
        );

        let mut client = zenwave::client();
        let response = client
            .method(http_kit::Method::PATCH, &url)
            .bearer_auth(&self.token)
            .json_body(&data)
            .await
            .map_err(|e| BotError::Api(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BotError::Api(format!(
                "Failed to edit response: {}",
                response.status()
            )));
        }

        Ok(())
    }
}
