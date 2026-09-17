use botkit_core::{BotError, FileSource};
use serde::de::DeserializeOwned;
use zenwave::{Client, ResponseExt};

use crate::types::{BotCommand, InlineKeyboardMarkup, ReplyMarkup, StickerSet};

const API_BASE: &str = "https://api.telegram.org";

/// Which `sendX` endpoint a media payload goes through — the endpoint name
/// and the form field Telegram expects the file under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// `sendPhoto` — images rendered inline.
    Photo,
    /// `sendAnimation` — GIFs and looping video without sound.
    Animation,
    /// `sendVideo` — video files.
    Video,
    /// `sendAudio` — music/podcast files with player UI.
    Audio,
    /// `sendVoice` — voice-note bubble (`.ogg` OPUS).
    Voice,
    /// `sendDocument` — everything else, as a file attachment.
    Document,
    /// `sendSticker` — native sticker formats or a sticker `file_id`.
    Sticker,
}

impl MediaKind {
    /// The Bot API method and its file/form field name.
    fn spec(self) -> (&'static str, &'static str) {
        match self {
            Self::Photo => ("sendPhoto", "photo"),
            Self::Animation => ("sendAnimation", "animation"),
            Self::Video => ("sendVideo", "video"),
            Self::Audio => ("sendAudio", "audio"),
            Self::Voice => ("sendVoice", "voice"),
            Self::Document => ("sendDocument", "document"),
            Self::Sticker => ("sendSticker", "sticker"),
        }
    }
}

/// The payload of a media upload — everything `send_upload` needs past the
/// endpoint and chat id.
struct Upload<'a> {
    /// The form field Telegram expects the file under.
    field: &'a str,
    /// The file bytes or path.
    file: FileSource,
    /// File name hint for the upload part.
    filename: &'a str,
    /// Optional caption under the media.
    caption: Option<&'a str>,
    /// Forum topic to post into.
    thread_id: Option<i64>,
}

/// A sticker being added to a set — the file plus its `InputSticker`
/// metadata.
pub struct NewSticker {
    /// The sticker image/animation bytes (`.png`/`.webp` static, `.tgs`
    /// animated, `.webm` video).
    pub file: FileSource,
    /// File name hint for the upload part.
    pub filename: String,
    /// Telegram sticker format: `static`, `animated`, or `video`.
    pub format: &'static str,
    /// The emoji the sticker is associated with.
    pub emoji: String,
    /// `replaceStickerInSet` only: `file_id` of the sticker being replaced.
    old_file_id: Option<String>,
}

impl NewSticker {
    /// A sticker file with its set metadata.
    pub fn new(
        file: FileSource,
        filename: impl Into<String>,
        format: &'static str,
        emoji: impl Into<String>,
    ) -> Self {
        Self {
            file,
            filename: filename.into(),
            format,
            emoji: emoji.into(),
            old_file_id: None,
        }
    }

    /// Mark this as replacing `file_id` (for `replaceStickerInSet`).
    fn with_old_file_id(mut self, file_id: &str) -> Self {
        self.old_file_id = Some(file_id.to_string());
        self
    }
}

/// A chat the Bot API can address: a numeric id or a public `@username`
/// — the API accepts either form wherever a `chat_id` goes.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum ChatRef {
    /// Numeric chat id (`-100…` for channels and supergroups).
    Id(i64),
    /// Public username, with the `@` prefix the API expects.
    Username(String),
}

impl From<i64> for ChatRef {
    fn from(id: i64) -> Self {
        Self::Id(id)
    }
}

impl From<String> for ChatRef {
    fn from(username: String) -> Self {
        Self::Username(username)
    }
}

impl From<&str> for ChatRef {
    fn from(username: &str) -> Self {
        Self::Username(username.to_string())
    }
}

/// Telegram REST API client
#[derive(Clone)]
pub struct TelegramClient {
    token: String,
}

impl TelegramClient {
    /// Create a new Telegram client
    pub fn new(token: impl Into<String>) -> Self {
        install_crypto_provider();

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

    /// Map a transport error, keeping the bot token out of the message.
    ///
    /// Telegram authenticates by putting the token in the request path, so any
    /// error that echoes the URL would otherwise leak it into logs.
    fn api_error(&self, error: impl std::fmt::Display) -> BotError {
        BotError::Api(error.to_string().replace(&self.token, "<token>"))
    }

    async fn post_json<T>(&self, method: &str, body: &serde_json::Value) -> Result<T, BotError>
    where
        T: DeserializeOwned,
    {
        let mut client = zenwave::client();
        let response = client
            .post(self.api_url(method))
            .map_err(|e| self.api_error(e))?
            .json_body(body)
            .map_err(|e| self.api_error(e))?
            .await
            .map_err(|e| self.api_error(e))?;

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
            .post(self.api_url(method))
            .map_err(|e| self.api_error(e))?
            .header("Content-Type", content_type)
            .map_err(|e| self.api_error(e))?
            .bytes_body(body)
            .await
            .map_err(|e| self.api_error(e))?;

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
        // Telegram reports most failures as a 4xx whose body carries the real
        // reason, so read the body before deciding what to report.
        let status = response.status();
        let body = response
            .into_body()
            .into_string()
            .await
            .map_err(|e| self.api_error(e))?;

        parse_api_response(method, &body).map_err(|e| {
            if status.is_success() {
                e
            } else {
                BotError::Api(format!("Telegram {method} failed with HTTP {status}: {e}"))
            }
        })
    }

    /// Send a text message
    ///
    /// `thread_id` targets a forum topic. Returns the id of the message that
    /// was sent.
    pub async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        thread_id: Option<i64>,
        reply_markup: Option<ReplyMarkup>,
    ) -> Result<i64, BotError> {
        self.send_message_inner(chat_id, text, None, thread_id, reply_markup)
            .await
    }

    /// Send a text message as a reply to another message
    ///
    /// `reply_to` is the id of the message to quote. Telegram drops the reply
    /// reference rather than failing when the target no longer exists
    /// (`allow_sending_without_reply`), and posts the reply into the
    /// referenced message's forum topic. Returns the sent message's id.
    pub async fn send_reply(
        &self,
        chat_id: i64,
        reply_to: i64,
        text: &str,
    ) -> Result<i64, BotError> {
        self.send_reply_markup(chat_id, reply_to, text, None).await
    }

    /// [`Self::send_reply`] with an inline keyboard attached.
    pub async fn send_reply_markup(
        &self,
        chat_id: i64,
        reply_to: i64,
        text: &str,
        markup: Option<InlineKeyboardMarkup>,
    ) -> Result<i64, BotError> {
        self.send_message_inner(
            chat_id,
            text,
            Some(reply_to),
            None,
            markup.map(ReplyMarkup::InlineKeyboard),
        )
        .await
    }

    async fn send_message_inner(
        &self,
        chat_id: i64,
        text: &str,
        reply_to: Option<i64>,
        thread_id: Option<i64>,
        reply_markup: Option<ReplyMarkup>,
    ) -> Result<i64, BotError> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
        });

        if let Some(thread) = thread_id {
            body["message_thread_id"] = serde_json::json!(thread);
        }

        if let Some(message_id) = reply_to {
            body["reply_parameters"] = serde_json::json!({
                "message_id": message_id,
                "allow_sending_without_reply": true,
            });
        }

        if let Some(markup) = reply_markup {
            body["reply_markup"] = serde_json::to_value(markup)
                .map_err(|e| BotError::Other(format!("failed to serialize reply markup: {e}")))?;
        }

        let message: crate::types::Message = self.post_json("sendMessage", &body).await?;
        Ok(message.message_id)
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

    /// Edit only a message's inline keyboard. Called with `None` it is the
    /// existence probe: Telegram replies "message is not modified" on a
    /// live message and "message to edit not found" on a dead one.
    pub async fn edit_message_reply_markup(
        &self,
        chat_id: i64,
        message_id: i64,
        reply_markup: Option<InlineKeyboardMarkup>,
    ) -> Result<(), BotError> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "message_id": message_id,
        });
        if let Some(markup) = reply_markup {
            body["reply_markup"] = serde_json::to_value(markup)
                .map_err(|e| BotError::Other(format!("failed to serialize reply markup: {e}")))?;
        }
        let _: serde_json::Value = self.post_json("editMessageReplyMarkup", &body).await?;
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
            "allowed_updates": [
                "message",
                "edited_message",
                "callback_query",
                "message_reaction"
            ],
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
        // `message_reaction` (and a few others) are only delivered when
        // explicitly opted into — list every kind we model.
        body["allowed_updates"] = serde_json::json!([
            "message",
            "edited_message",
            "callback_query",
            "message_reaction"
        ]);

        self.post_json("getUpdates", &body).await
    }

    /// Send a chat action (typing, uploading, etc.). `thread_id` targets a
    /// forum topic.
    pub async fn send_chat_action(
        &self,
        chat_id: i64,
        action: &str,
        thread_id: Option<i64>,
    ) -> Result<(), BotError> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "action": action,
        });
        if let Some(thread) = thread_id {
            body["message_thread_id"] = serde_json::json!(thread);
        }

        let _: serde_json::Value = self.post_json("sendChatAction", &body).await?;
        Ok(())
    }

    /// Set the bot's emoji reaction on a message. `None` removes the bot's
    /// reaction; `is_big` plays the large animation.
    pub async fn set_message_reaction(
        &self,
        chat_id: i64,
        message_id: i64,
        emoji: Option<&str>,
        is_big: bool,
    ) -> Result<(), BotError> {
        let reactions: Vec<_> = emoji
            .into_iter()
            .map(|emoji| serde_json::json!({"type": "emoji", "emoji": emoji}))
            .collect();
        let body = serde_json::json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "reaction": reactions,
            "is_big": is_big,
        });
        let _: serde_json::Value = self.post_json("setMessageReaction", &body).await?;
        Ok(())
    }

    /// Delete a message — the bot's own, or any message in a chat where it
    /// has delete rights.
    pub async fn delete_message(&self, chat_id: i64, message_id: i64) -> Result<(), BotError> {
        let _: serde_json::Value = self
            .post_json(
                "deleteMessage",
                &serde_json::json!({"chat_id": chat_id, "message_id": message_id}),
            )
            .await?;
        Ok(())
    }

    /// Pin a message (`notify = false` pins silently); `unpin` removes the
    /// pin. Needs pin rights in groups.
    pub async fn pin_message(
        &self,
        chat_id: i64,
        message_id: i64,
        notify: bool,
    ) -> Result<(), BotError> {
        let _: serde_json::Value = self
            .post_json(
                "pinChatMessage",
                &serde_json::json!({
                    "chat_id": chat_id,
                    "message_id": message_id,
                    "disable_notification": !notify,
                }),
            )
            .await?;
        Ok(())
    }

    /// See [`Self::pin_message`].
    pub async fn unpin_message(&self, chat_id: i64, message_id: i64) -> Result<(), BotError> {
        let _: serde_json::Value = self
            .post_json(
                "unpinChatMessage",
                &serde_json::json!({"chat_id": chat_id, "message_id": message_id}),
            )
            .await?;
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

    /// The bot's own identity. `id` is the `user_id` owner argument the
    /// sticker-set methods take; `username` is the mandatory
    /// `_by_<bot>` suffix for sets the bot creates.
    pub async fn get_me(&self) -> Result<crate::types::User, BotError> {
        self.post_json("getMe", &serde_json::json!({})).await
    }

    /// `getChat` — a chat's metadata. Works for any public chat by
    /// `@username` and for any chat the bot can see by id; a chat the bot
    /// has no access to answers "chat not found".
    pub async fn get_chat(&self, chat: ChatRef) -> Result<crate::types::Chat, BotError> {
        self.post_json("getChat", &serde_json::json!({"chat_id": chat}))
            .await
    }

    /// `getChatMemberCount` — the chat's member count, under the same
    /// reachability rules as [`Self::get_chat`].
    pub async fn get_chat_member_count(&self, chat: ChatRef) -> Result<i64, BotError> {
        self.post_json("getChatMemberCount", &serde_json::json!({"chat_id": chat}))
            .await
    }

    /// `getChatMember` — a user's status in a chat. Only chats the bot
    /// belongs to answer (channels additionally require the bot to be an
    /// administrator), so an error here is itself the "not a member"
    /// signal.
    pub async fn get_chat_member(
        &self,
        chat: ChatRef,
        user_id: i64,
    ) -> Result<crate::types::ChatMember, BotError> {
        self.post_json(
            "getChatMember",
            &serde_json::json!({"chat_id": chat, "user_id": user_id}),
        )
        .await
    }

    /// `forwardMessage` — copies `message_id` from `from` into `to`,
    /// attributing it to the original sender. The bot must be able to see
    /// the source message (membership) and write to the destination.
    /// Returns the new message.
    pub async fn forward_message(
        &self,
        to: i64,
        from: ChatRef,
        message_id: i64,
    ) -> Result<crate::types::Message, BotError> {
        self.post_json(
            "forwardMessage",
            &serde_json::json!({
                "chat_id": to,
                "from_chat_id": from,
                "message_id": message_id,
            }),
        )
        .await
    }

    /// `getFile` — resolve a `file_id` to its server-side `file_path`.
    pub async fn get_file(&self, file_id: &str) -> Result<crate::types::File, BotError> {
        self.post_json("getFile", &serde_json::json!({"file_id": file_id}))
            .await
    }

    /// Download a file's bytes using the `file_path` `getFile` returned.
    ///
    /// Telegram serves file content from a separate URL shape
    /// (`/file/bot<token>/<path>`) and returns raw bytes rather than the
    /// usual JSON envelope. `limit` caps the buffered size; files larger
    /// than it error instead of truncating.
    pub async fn download_file(&self, file_path: &str, limit: usize) -> Result<Vec<u8>, BotError> {
        let url = format!("{}/file/bot{}/{}", API_BASE, self.token, file_path);
        let response = zenwave::get(&url).await.map_err(|e| self.api_error(e))?;
        let bytes = response
            .error_for_status()
            .await
            .map_err(|e| self.api_error(e))?
            .into_bytes_with_limit(limit)
            .await
            .map_err(|e| self.api_error(e))?;
        Ok(bytes.to_vec())
    }

    /// Send a document/file.
    ///
    /// Returns the id of the message that was sent.
    pub async fn send_document(
        &self,
        chat_id: i64,
        file: FileSource,
        filename: Option<&str>,
        caption: Option<&str>,
        thread_id: Option<i64>,
    ) -> Result<i64, BotError> {
        self.send_media(
            chat_id,
            MediaKind::Document,
            file,
            filename.unwrap_or("file"),
            caption,
            thread_id,
        )
        .await
    }

    /// Send a photo (JPEG, PNG, WebP, GIF, …)
    ///
    /// Returns the id of the message that was sent.
    pub async fn send_photo(
        &self,
        chat_id: i64,
        file: FileSource,
        filename: &str,
        caption: Option<&str>,
        thread_id: Option<i64>,
    ) -> Result<i64, BotError> {
        self.send_media(
            chat_id,
            MediaKind::Photo,
            file,
            filename,
            caption,
            thread_id,
        )
        .await
    }

    /// Send a native sticker by upload (WebP, TGS, or WebM)
    ///
    /// Returns the id of the message that was sent.
    pub async fn send_sticker(
        &self,
        chat_id: i64,
        file: FileSource,
        filename: &str,
        thread_id: Option<i64>,
    ) -> Result<i64, BotError> {
        self.send_media(chat_id, MediaKind::Sticker, file, filename, None, thread_id)
            .await
    }

    /// Upload `file` as the given media kind; returns the sent message's id.
    /// `thread_id` targets a forum topic.
    pub async fn send_media(
        &self,
        chat_id: i64,
        kind: MediaKind,
        file: FileSource,
        filename: &str,
        caption: Option<&str>,
        thread_id: Option<i64>,
    ) -> Result<i64, BotError> {
        let (method, field) = kind.spec();
        self.send_upload(
            method,
            chat_id,
            Upload {
                field,
                file,
                filename,
                caption,
                thread_id,
            },
        )
        .await
    }

    /// Send media Telegram already hosts by `file_id` — anything the bot has
    /// seen in a message or holds in a sticker set. `thread_id` targets a
    /// forum topic. Returns the message id.
    pub async fn send_media_id(
        &self,
        chat_id: i64,
        kind: MediaKind,
        file_id: &str,
        caption: Option<&str>,
        thread_id: Option<i64>,
    ) -> Result<i64, BotError> {
        let (method, field) = kind.spec();
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            field: file_id,
        });
        if let Some(caption) = caption {
            body["caption"] = serde_json::json!(caption);
        }
        if let Some(thread) = thread_id {
            body["message_thread_id"] = serde_json::json!(thread);
        }
        let message: crate::types::Message = self.post_json(method, &body).await?;
        Ok(message.message_id)
    }

    /// Fetch a sticker set by short name.
    ///
    /// A missing set fails with a `BotError::Api` carrying Telegram's
    /// `STICKERSET_INVALID` description.
    pub async fn get_sticker_set(&self, name: &str) -> Result<StickerSet, BotError> {
        self.post_json("getStickerSet", &serde_json::json!({"name": name}))
            .await
    }

    /// Create a sticker set owned by `user_id` (the bot itself works) with
    /// `sticker` as its first entry. `name` must end in `_by_<bot username>`.
    pub async fn create_sticker_set(
        &self,
        user_id: i64,
        name: &str,
        title: &str,
        sticker: NewSticker,
    ) -> Result<(), BotError> {
        self.sticker_set_edit("createNewStickerSet", user_id, name, Some(title), sticker)
            .await
    }

    /// Append `sticker` to a set the bot owns.
    pub async fn add_sticker_to_set(
        &self,
        user_id: i64,
        name: &str,
        sticker: NewSticker,
    ) -> Result<(), BotError> {
        self.sticker_set_edit("addStickerToSet", user_id, name, None, sticker)
            .await
    }

    /// Replace the sticker `old_file_id` points at with `sticker`, keeping
    /// its position in the set.
    pub async fn replace_sticker_in_set(
        &self,
        user_id: i64,
        name: &str,
        old_file_id: &str,
        sticker: NewSticker,
    ) -> Result<(), BotError> {
        self.sticker_set_edit(
            "replaceStickerInSet",
            user_id,
            name,
            None,
            sticker.with_old_file_id(old_file_id),
        )
        .await
    }

    /// Shared multipart body for the sticker-set edit methods: the sticker
    /// metadata goes as an `InputSticker` JSON referencing the uploaded
    /// bytes through `attach://s0`.
    async fn sticker_set_edit(
        &self,
        method: &str,
        user_id: i64,
        name: &str,
        title: Option<&str>,
        sticker: NewSticker,
    ) -> Result<(), BotError> {
        use zenwave::multipart::{Multipart, MultipartPart};

        let contents = sticker
            .file
            .read()
            .await
            .map_err(|e| BotError::Other(format!("failed to read sticker file: {e}")))?;

        let input = serde_json::json!({
            "sticker": "attach://s0",
            "format": sticker.format,
            "emoji_list": [sticker.emoji],
        });

        let mut multipart = Multipart::new();
        multipart.push(MultipartPart::text("user_id", user_id.to_string()));
        multipart.push(MultipartPart::text("name", name));
        if let Some(title) = title {
            multipart.push(MultipartPart::text("title", title));
            // createNewStickerSet wraps the InputSticker in a `stickers`
            // array and wants the set's type; addStickerToSet takes the
            // single object under `sticker`.
            multipart.push(MultipartPart::text("sticker_type", "regular"));
            multipart.push(MultipartPart::text(
                "stickers",
                serde_json::json!([input]).to_string(),
            ));
        } else {
            multipart.push(MultipartPart::text("sticker", input.to_string()));
            if let Some(old) = sticker.old_file_id {
                multipart.push(MultipartPart::text("old_sticker", old));
            }
        }
        let mime = mime_guess::from_path(&sticker.filename)
            .first_or_octet_stream()
            .to_string();
        multipart.push(MultipartPart::binary(
            "s0".to_owned(),
            sticker.filename,
            mime,
            contents,
        ));

        let (boundary, body) = multipart.encode();
        let content_type = format!("multipart/form-data; boundary={}", boundary);
        let _: serde_json::Value = self.post_multipart(method, content_type, body).await?;
        Ok(())
    }

    /// Upload a file with a multipart request to `method`. Returns the sent
    /// message's id.
    async fn send_upload(
        &self,
        method: &str,
        chat_id: i64,
        upload: Upload<'_>,
    ) -> Result<i64, BotError> {
        use zenwave::multipart::{Multipart, MultipartPart};

        let contents = upload
            .file
            .read()
            .await
            .map_err(|e| BotError::Other(format!("failed to read attachment: {e}")))?;

        let mut multipart = Multipart::new();
        multipart.push(MultipartPart::text("chat_id", chat_id.to_string()));

        if let Some(thread) = upload.thread_id {
            multipart.push(MultipartPart::text("message_thread_id", thread.to_string()));
        }

        if let Some(caption) = upload.caption {
            multipart.push(MultipartPart::text("caption", caption));
        }

        multipart.push(MultipartPart::binary(
            upload.field.to_owned(),
            upload.filename.to_owned(),
            mime_guess::from_path(upload.filename)
                .first_or_octet_stream()
                .to_string(),
            contents,
        ));

        let (boundary, body) = multipart.encode();
        let content_type = format!("multipart/form-data; boundary={}", boundary);

        let message: crate::types::Message =
            self.post_multipart(method, content_type, body).await?;
        Ok(message.message_id)
    }
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
    use super::{TelegramClient, parse_api_response};

    #[test]
    fn building_a_client_installs_a_crypto_provider() {
        // Without this, a bot that also talks to Matrix panics on its first TLS
        // handshake: both `ring` and `aws-lc-rs` end up compiled in and rustls
        // refuses to pick one for you.
        let _client = TelegramClient::new("token");
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    #[test]
    fn redacts_the_token_from_error_messages() {
        // The token lives in every request path, so it must never reach a log.
        let client = TelegramClient::new("123456:SECRET");
        let error = client.api_error("connect to https://api.telegram.org/bot123456:SECRET/x");
        assert!(!error.to_string().contains("SECRET"), "{error}");
        assert!(error.to_string().contains("<token>"), "{error}");
    }

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
