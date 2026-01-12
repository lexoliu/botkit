use serde::{Deserialize, Serialize};

/// Rich embed for messages
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Embed {
    /// Title of the embed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Description text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// URL for the title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Color of the embed sidebar (as integer)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    /// Embed fields
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<EmbedField>,
    /// Footer information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<EmbedFooter>,
    /// Image to display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<EmbedImage>,
    /// Thumbnail image
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<EmbedImage>,
    /// Author information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<EmbedAuthor>,
    /// Timestamp (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl Embed {
    /// Create a new empty embed
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the URL
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Set the color (as hex integer, e.g., 0x5865F2)
    pub fn color(mut self, color: u32) -> Self {
        self.color = Some(color);
        self
    }

    /// Add a field
    pub fn field(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
        inline: bool,
    ) -> Self {
        self.fields.push(EmbedField {
            name: name.into(),
            value: value.into(),
            inline,
        });
        self
    }

    /// Set the footer
    pub fn footer(mut self, text: impl Into<String>) -> Self {
        self.footer = Some(EmbedFooter {
            text: text.into(),
            icon_url: None,
        });
        self
    }

    /// Set the footer with an icon
    pub fn footer_with_icon(
        mut self,
        text: impl Into<String>,
        icon_url: impl Into<String>,
    ) -> Self {
        self.footer = Some(EmbedFooter {
            text: text.into(),
            icon_url: Some(icon_url.into()),
        });
        self
    }

    /// Set the image
    pub fn image(mut self, url: impl Into<String>) -> Self {
        self.image = Some(EmbedImage { url: url.into() });
        self
    }

    /// Set the thumbnail
    pub fn thumbnail(mut self, url: impl Into<String>) -> Self {
        self.thumbnail = Some(EmbedImage { url: url.into() });
        self
    }

    /// Set the author
    pub fn author(mut self, name: impl Into<String>) -> Self {
        self.author = Some(EmbedAuthor {
            name: name.into(),
            url: None,
            icon_url: None,
        });
        self
    }

    /// Set the author with URL and icon
    pub fn author_full(
        mut self,
        name: impl Into<String>,
        url: Option<String>,
        icon_url: Option<String>,
    ) -> Self {
        self.author = Some(EmbedAuthor {
            name: name.into(),
            url,
            icon_url,
        });
        self
    }

    /// Set the timestamp
    pub fn timestamp(mut self, timestamp: impl Into<String>) -> Self {
        self.timestamp = Some(timestamp.into());
        self
    }
}

/// Field within an embed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedField {
    /// Field name/title
    pub name: String,
    /// Field value/content
    pub value: String,
    /// Whether to display inline with other fields
    #[serde(default)]
    pub inline: bool,
}

/// Footer of an embed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedFooter {
    /// Footer text
    pub text: String,
    /// URL of footer icon
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

/// Image in an embed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedImage {
    /// URL of the image
    pub url: String,
}

/// Author information in an embed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedAuthor {
    /// Author name
    pub name: String,
    /// URL to link the author name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// URL of author icon
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}
