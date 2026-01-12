use crate::types::component::Component;
use crate::types::embed::Embed;

/// Unified bot response builder
///
/// Represents a response that can be sent back to any platform.
/// Each platform adapter converts this to platform-specific format.
pub struct Response {
    kind: ResponseKind,
}

enum ResponseKind {
    /// Empty response (no reply)
    Empty,
    /// Text message
    Text(TextResponse),
    /// Acknowledge without visible response (for deferred responses)
    Acknowledge,
    /// File attachment
    File(FileResponse),
}

struct TextResponse {
    content: String,
    embeds: Vec<Embed>,
    components: Vec<Component>,
    ephemeral: bool,
}

/// File response data
pub struct FileResponse {
    /// The file to send
    pub file: async_fs::File,
    /// Optional filename (used for content-disposition)
    pub filename: Option<String>,
    /// Optional caption to accompany the file
    pub caption: Option<String>,
}

impl Response {
    /// Create an empty response (no reply)
    pub fn empty() -> Self {
        Self {
            kind: ResponseKind::Empty,
        }
    }

    /// Create a text response
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            kind: ResponseKind::Text(TextResponse {
                content: content.into(),
                embeds: Vec::new(),
                components: Vec::new(),
                ephemeral: false,
            }),
        }
    }

    /// Create an acknowledgement response (deferred)
    pub fn acknowledge() -> Self {
        Self {
            kind: ResponseKind::Acknowledge,
        }
    }

    /// Create a response with an embed
    pub fn embed(embed: Embed) -> Self {
        Self {
            kind: ResponseKind::Text(TextResponse {
                content: String::new(),
                embeds: vec![embed],
                components: Vec::new(),
                ephemeral: false,
            }),
        }
    }

    /// Add an embed to this response
    pub fn with_embed(mut self, embed: Embed) -> Self {
        if let ResponseKind::Text(ref mut text) = self.kind {
            text.embeds.push(embed);
        }
        self
    }

    /// Add components (buttons, select menus) to this response
    pub fn with_components(mut self, components: Vec<Component>) -> Self {
        if let ResponseKind::Text(ref mut text) = self.kind {
            text.components = components;
        }
        self
    }

    /// Make this response ephemeral (only visible to the user)
    pub fn ephemeral(mut self) -> Self {
        if let ResponseKind::Text(ref mut text) = self.kind {
            text.ephemeral = true;
        }
        self
    }

    /// Create a file response
    pub fn file(file: async_fs::File) -> Self {
        Self {
            kind: ResponseKind::File(FileResponse {
                file,
                filename: None,
                caption: None,
            }),
        }
    }

    /// Set the filename for a file response
    pub fn with_filename(mut self, name: impl Into<String>) -> Self {
        if let ResponseKind::File(ref mut f) = self.kind {
            f.filename = Some(name.into());
        }
        self
    }

    /// Set the caption for a file response
    pub fn with_caption(mut self, caption: impl Into<String>) -> Self {
        if let ResponseKind::File(ref mut f) = self.kind {
            f.caption = Some(caption.into());
        }
        self
    }

    /// Check if this is an empty response
    pub fn is_empty(&self) -> bool {
        matches!(self.kind, ResponseKind::Empty)
    }

    /// Check if this is an acknowledge response
    pub fn is_acknowledge(&self) -> bool {
        matches!(self.kind, ResponseKind::Acknowledge)
    }

    /// Get response content if this is a text response
    pub fn content(&self) -> Option<&str> {
        match &self.kind {
            ResponseKind::Text(t) => Some(&t.content),
            _ => None,
        }
    }

    /// Get embeds if this is a text response
    pub fn embeds(&self) -> &[Embed] {
        match &self.kind {
            ResponseKind::Text(t) => &t.embeds,
            _ => &[],
        }
    }

    /// Get components if this is a text response
    pub fn components(&self) -> &[Component] {
        match &self.kind {
            ResponseKind::Text(t) => &t.components,
            _ => &[],
        }
    }

    /// Check if this response is ephemeral
    pub fn is_ephemeral(&self) -> bool {
        match &self.kind {
            ResponseKind::Text(t) => t.ephemeral,
            _ => false,
        }
    }

    /// Check if this is a file response
    pub fn is_file(&self) -> bool {
        matches!(self.kind, ResponseKind::File(_))
    }

    /// Take the file response data, leaving Empty in its place
    ///
    /// This consumes the file data, so it can only be called once.
    pub fn take_file(&mut self) -> Option<FileResponse> {
        match std::mem::replace(&mut self.kind, ResponseKind::Empty) {
            ResponseKind::File(f) => Some(f),
            other => {
                self.kind = other;
                None
            }
        }
    }
}
