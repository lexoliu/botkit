use std::fmt;

use crate::types::component::Component;
use crate::types::embed::Embed;

/// Unified bot response builder
///
/// Represents a response that can be sent back to any platform.
/// Each platform adapter converts this to platform-specific format.
///
/// The builder methods (`with_embed`, `ephemeral`, `with_caption`, ...) apply
/// to whichever kind of response they make sense for. Calling a message builder
/// on an empty response promotes it to a message rather than silently dropping
/// the value, so `Response::empty().with_embed(e)` sends the embed.
#[derive(Default)]
pub struct Response {
    kind: ResponseKind,
}

#[derive(Default)]
enum ResponseKind {
    /// Empty response (no reply)
    #[default]
    Empty,
    /// Text message, optionally with embeds and components
    Message(Message),
    /// Acknowledge without visible response (for deferred responses)
    Acknowledge,
    /// File attachment
    File(FileResponse),
}

#[derive(Default)]
struct Message {
    content: String,
    embeds: Vec<Embed>,
    components: Vec<Component>,
    ephemeral: bool,
}

/// Bytes to upload, plus how to present them
pub struct FileResponse {
    /// The file contents
    pub file: FileSource,
    /// Optional filename (used for content-disposition)
    pub filename: Option<String>,
    /// Optional caption to accompany the file
    pub caption: Option<String>,
}

/// Where a file response's bytes come from
///
/// Adapters read this to completion before upload, so a `Path` is opened lazily
/// and only once.
pub enum FileSource {
    /// An already-open file handle
    File(async_fs::File),
    /// Bytes held in memory
    Bytes(Vec<u8>),
    /// A path to read at send time
    Path(std::path::PathBuf),
}

impl FileSource {
    /// Read the whole source into memory
    pub async fn read(self) -> std::io::Result<Vec<u8>> {
        use futures_lite::io::AsyncReadExt;

        match self {
            Self::Bytes(bytes) => Ok(bytes),
            Self::Path(path) => async_fs::read(path).await,
            Self::File(mut file) => {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes).await?;
                Ok(bytes)
            }
        }
    }

    /// The filename implied by the source, if it has one
    fn implied_filename(&self) -> Option<String> {
        match self {
            Self::Path(path) => Some(path.file_name()?.to_string_lossy().into_owned()),
            _ => None,
        }
    }
}

impl fmt::Debug for FileSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(_) => f.write_str("FileSource::File(..)"),
            Self::Bytes(bytes) => write!(f, "FileSource::Bytes({} bytes)", bytes.len()),
            Self::Path(path) => write!(f, "FileSource::Path({})", path.display()),
        }
    }
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
            kind: ResponseKind::Message(Message {
                content: content.into(),
                ..Message::default()
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
        Self::empty().with_embed(embed)
    }

    /// Create a file response from an open file handle
    pub fn file(file: async_fs::File) -> Self {
        Self::from_source(FileSource::File(file))
    }

    /// Create a file response from bytes already in memory
    pub fn bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self::from_source(FileSource::Bytes(bytes.into()))
    }

    /// Create a file response that reads `path` when the response is sent
    ///
    /// The filename defaults to the path's final component.
    pub fn path(path: impl Into<std::path::PathBuf>) -> Self {
        Self::from_source(FileSource::Path(path.into()))
    }

    fn from_source(file: FileSource) -> Self {
        Self {
            kind: ResponseKind::File(FileResponse {
                filename: file.implied_filename(),
                file,
                caption: None,
            }),
        }
    }

    /// Message parts of this response, promoting `Empty` to a message
    ///
    /// Returns `None` for acknowledge and file responses, which have no
    /// embeds or components to carry.
    fn message_mut(&mut self) -> Option<&mut Message> {
        if matches!(self.kind, ResponseKind::Empty) {
            self.kind = ResponseKind::Message(Message::default());
        }
        match &mut self.kind {
            ResponseKind::Message(message) => Some(message),
            _ => None,
        }
    }

    /// Add an embed to this response
    pub fn with_embed(mut self, embed: Embed) -> Self {
        if let Some(message) = self.message_mut() {
            message.embeds.push(embed);
        }
        self
    }

    /// Add components (buttons, select menus) to this response
    pub fn with_components(mut self, components: Vec<Component>) -> Self {
        if let Some(message) = self.message_mut() {
            message.components = components;
        }
        self
    }

    /// Make this response ephemeral (only visible to the user)
    ///
    /// Platforms without ephemeral messages (Telegram, Matrix) ignore this.
    pub fn ephemeral(mut self) -> Self {
        if let Some(message) = self.message_mut() {
            message.ephemeral = true;
        }
        self
    }

    /// Set the filename for a file response
    pub fn with_filename(mut self, name: impl Into<String>) -> Self {
        if let ResponseKind::File(file) = &mut self.kind {
            file.filename = Some(name.into());
        }
        self
    }

    /// Set the caption for a file response
    pub fn with_caption(mut self, caption: impl Into<String>) -> Self {
        if let ResponseKind::File(file) = &mut self.kind {
            file.caption = Some(caption.into());
        }
        self
    }

    /// Check if this response carries nothing to send
    pub fn is_empty(&self) -> bool {
        match &self.kind {
            ResponseKind::Empty => true,
            ResponseKind::Message(message) => {
                message.content.is_empty()
                    && message.embeds.is_empty()
                    && message.components.is_empty()
            }
            _ => false,
        }
    }

    /// Check if this is an acknowledge response
    pub fn is_acknowledge(&self) -> bool {
        matches!(self.kind, ResponseKind::Acknowledge)
    }

    /// Get response content if this is a message response
    pub fn content(&self) -> Option<&str> {
        match &self.kind {
            ResponseKind::Message(message) => Some(&message.content),
            _ => None,
        }
    }

    /// Get embeds if this is a message response
    pub fn embeds(&self) -> &[Embed] {
        match &self.kind {
            ResponseKind::Message(message) => &message.embeds,
            _ => &[],
        }
    }

    /// Get components if this is a message response
    pub fn components(&self) -> &[Component] {
        match &self.kind {
            ResponseKind::Message(message) => &message.components,
            _ => &[],
        }
    }

    /// Check if this response is ephemeral
    pub fn is_ephemeral(&self) -> bool {
        match &self.kind {
            ResponseKind::Message(message) => message.ephemeral,
            _ => false,
        }
    }

    /// Check if this is a file response
    pub fn is_file(&self) -> bool {
        matches!(self.kind, ResponseKind::File(_))
    }

    /// Take the file response data, leaving an empty response in its place
    ///
    /// This consumes the file data, so it can only be called once.
    pub fn take_file(&mut self) -> Option<FileResponse> {
        match std::mem::take(&mut self.kind) {
            ResponseKind::File(file) => Some(file),
            other => {
                self.kind = other;
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_lite::future::block_on;

    #[test]
    fn text_carries_content() {
        let response = Response::text("hi");
        assert_eq!(response.content(), Some("hi"));
        assert!(!response.is_empty());
        assert!(!response.is_file());
    }

    #[test]
    fn empty_response_is_empty() {
        assert!(Response::empty().is_empty());
        assert!(Response::text("").is_empty());
        assert!(!Response::acknowledge().is_empty());
    }

    #[test]
    fn builders_promote_empty_instead_of_dropping_values() {
        let response = Response::empty()
            .with_embed(Embed::new().title("t"))
            .ephemeral();
        assert_eq!(response.embeds().len(), 1);
        assert!(response.is_ephemeral());
        assert!(!response.is_empty());
    }

    #[test]
    fn embed_constructor_matches_builder() {
        let response = Response::embed(Embed::new().title("t"));
        assert_eq!(response.embeds().len(), 1);
        assert_eq!(response.embeds()[0].title.as_deref(), Some("t"));
    }

    #[test]
    fn message_builders_leave_file_responses_alone() {
        let response = Response::bytes(b"data".to_vec())
            .with_embed(Embed::new())
            .ephemeral();
        assert!(response.is_file());
        assert!(response.embeds().is_empty());
        assert!(!response.is_ephemeral());
    }

    #[test]
    fn path_responses_default_their_filename() {
        let response = Response::path("/tmp/report.pdf");
        let mut response = response;
        let file = response.take_file().unwrap();
        assert_eq!(file.filename.as_deref(), Some("report.pdf"));
    }

    #[test]
    fn take_file_yields_the_payload_once() {
        let mut response = Response::bytes(b"hello".to_vec()).with_caption("cap");

        let file = response.take_file().expect("first take yields the file");
        assert_eq!(file.caption.as_deref(), Some("cap"));
        assert_eq!(block_on(file.file.read()).unwrap(), b"hello");

        assert!(response.take_file().is_none());
        assert!(response.is_empty());
    }

    #[test]
    fn take_file_preserves_non_file_responses() {
        let mut response = Response::text("hi");
        assert!(response.take_file().is_none());
        assert_eq!(response.content(), Some("hi"));
    }
}
