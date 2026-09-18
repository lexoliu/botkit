//! Markdown → Telegram entities.
//!
//! Bots speak markdown; Telegram wants `(text, entities)` — a plain string
//! plus formatting ranges whose offsets are UTF-16 code-unit indices. This
//! module parses CommonMark with `pulldown-cmark` and emits that pair
//! directly, so no `parse_mode` string escaping is ever involved: emphasis
//! next to CJK text, full-width punctuation, or nested spans all render
//! exactly as the AST says.
//!
//! Supported: `**bold**`, `*italic*`/`_x_`, `~~strike~~`, `` `code` ``,
//! fenced ``` blocks (language → `pre`), `[label](url)`, `![alt](img)`,
//! `>` quotes (→ `blockquote`), `#`–`######` headings (→ bold), ordered and
//! bullet lists (rendered as literal `- `/`n.` markers), task-list markers,
//! rules (`---`). Anything else — tables, footnotes, inline HTML — passes
//! through as literal text.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::types::{EntityType, Formatted, MessageEntity};

/// Owned counterpart of [`Formatted`]: the plain text plus its entity list,
/// as produced by [`render`].
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    /// The message text with markdown syntax resolved away.
    pub text: String,
    /// Entities covering `text`; offsets are UTF-16 code units.
    pub entities: Vec<MessageEntity>,
}

impl Rendered {
    /// Borrow this render in the shape send/edit/caption methods take.
    pub fn formatted(&self) -> Formatted<'_> {
        Formatted {
            text: &self.text,
            entities: &self.entities,
        }
    }
}

/// Parse `markdown` and render it to the text/entity pair Telegram accepts.
pub fn render(markdown: &str) -> Rendered {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut renderer = Renderer::default();
    for event in Parser::new_ext(markdown, options) {
        renderer.event(event);
    }
    renderer.finish()
}

/// A span opened by `Tag::Start` and closed by its `TagEnd`, which becomes
/// one [`MessageEntity`] covering the text emitted in between.
struct Open {
    kind: EntityType,
    url: Option<String>,
    language: Option<String>,
    /// Byte offset into `out` where the covered text begins.
    start: usize,
    /// UTF-16 counterpart of `start`.
    start16: usize,
    /// Text emitted inside the span if it is empty when closed — images
    /// with no alt text fall back to showing the destination URL.
    empty_fill: Option<String>,
}

struct ListCtx {
    ordered: bool,
    next: u64,
}

#[derive(Default)]
struct Renderer {
    out: String,
    /// UTF-16 length of `out` — entity offsets count UTF-16 units, not
    /// bytes (an emoji is two).
    len16: usize,
    entities: Vec<MessageEntity>,
    stack: Vec<Open>,
    lists: Vec<ListCtx>,
    /// Open `Item` count — paragraphs inside list items are tight list
    /// content and get no blank-line separation.
    item_depth: usize,
}

impl Renderer {
    fn event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.push(&text),
            Event::Code(code) => {
                self.open(EntityType::Code, None, None);
                self.push(&code);
                self.close();
            }
            Event::InlineMath(math) | Event::DisplayMath(math) => self.push(&math),
            // HTML passes through literally — with entities there is no
            // parser for it to feed.
            Event::Html(html) | Event::InlineHtml(html) => self.push(&html),
            Event::FootnoteReference(name) => self.push(&format!("[^{name}]")),
            Event::SoftBreak | Event::HardBreak => self.push("\n"),
            Event::Rule => {
                self.blank_line();
                self.push("———");
            }
            Event::TaskListMarker(checked) => {
                self.push(if checked { "[x] " } else { "[ ] " });
            }
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                if self.item_depth == 0 {
                    self.blank_line();
                }
            }
            Tag::Heading { .. } => {
                self.newline();
                self.open(EntityType::Bold, None, None);
            }
            Tag::BlockQuote(..) => {
                self.newline();
                self.open(EntityType::Blockquote, None, None);
            }
            Tag::CodeBlock(kind) => {
                self.newline();
                let language = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.to_string()),
                    _ => None,
                };
                self.open(EntityType::Pre, None, language);
            }
            Tag::List(first) => {
                self.newline();
                self.lists.push(ListCtx {
                    ordered: first.is_some(),
                    next: first.unwrap_or(1),
                });
            }
            Tag::Item => {
                self.item_depth += 1;
                self.newline();
                for _ in 1..self.lists.len() {
                    self.push("  ");
                }
                let marker = match self.lists.last_mut() {
                    Some(ctx) if ctx.ordered => {
                        let marker = format!("{}. ", ctx.next);
                        ctx.next += 1;
                        marker
                    }
                    _ => "- ".to_string(),
                };
                self.push(&marker);
            }
            Tag::Emphasis => self.open(EntityType::Italic, None, None),
            Tag::Strong => self.open(EntityType::Bold, None, None),
            Tag::Strikethrough => self.open(EntityType::Strikethrough, None, None),
            Tag::Link { dest_url, .. } => {
                self.open(EntityType::TextLink, Some(dest_url.to_string()), None);
            }
            Tag::Image { dest_url, .. } => {
                let mut open = Open::new(
                    EntityType::TextLink,
                    Some(dest_url.to_string()),
                    self.out.len(),
                    self.len16,
                );
                open.empty_fill = Some(dest_url.to_string());
                self.stack.push(open);
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Link
            | TagEnd::Image
            | TagEnd::Heading(..)
            | TagEnd::BlockQuote(..)
            | TagEnd::CodeBlock => self.close(),
            TagEnd::Item => self.item_depth -= 1,
            _ => {}
        }
        // Block-level containers separate what follows.
        match tag {
            TagEnd::Heading(..) | TagEnd::BlockQuote(..) | TagEnd::CodeBlock | TagEnd::Item => {
                self.newline();
            }
            _ => {}
        }
    }

    fn push(&mut self, text: &str) {
        self.out.push_str(text);
        self.len16 += text.encode_utf16().count();
    }

    /// Ensure `out` ends with a single newline when it has content.
    fn newline(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.push("\n");
        }
    }

    /// Ensure `out` ends with a blank line when it has content.
    fn blank_line(&mut self) {
        if !self.out.is_empty() {
            if self.out.ends_with('\n') {
                self.push("\n");
            } else {
                self.push("\n\n");
            }
        }
    }

    fn open(&mut self, kind: EntityType, url: Option<String>, language: Option<String>) {
        let mut open = Open::new(kind, url, self.out.len(), self.len16);
        open.language = language;
        self.stack.push(open);
    }

    /// Pop the innermost span and emit its entity. Whitespace at the edges
    /// is excluded from the range — Telegram ignores entities that cover
    /// stray whitespace, and an empty range drops the entity entirely.
    fn close(&mut self) {
        let Some(open) = self.stack.pop() else {
            return;
        };
        if open.start == self.out.len()
            && let Some(fill) = &open.empty_fill
        {
            self.push(fill);
        }
        let mut start = open.start;
        let mut start16 = open.start16;
        let mut end = self.out.len();
        let mut end16 = self.len16;
        while let Some(c) = self.out[start..end].chars().next() {
            if !c.is_whitespace() {
                break;
            }
            start += c.len_utf8();
            start16 += c.len_utf16();
        }
        while let Some(c) = self.out[start..end].chars().next_back() {
            if !c.is_whitespace() {
                break;
            }
            end -= c.len_utf8();
            end16 -= c.len_utf16();
        }
        if end > start {
            self.entities.push(MessageEntity {
                entity_type: open.kind,
                offset: start16 as i64,
                length: (end16 - start16) as i64,
                user: None,
                url: open.url,
                language: open.language,
                custom_emoji_id: None,
            });
        }
    }

    fn finish(self) -> Rendered {
        Rendered {
            text: self.out,
            entities: self.entities,
        }
    }
}

impl Open {
    fn new(kind: EntityType, url: Option<String>, start: usize, start16: usize) -> Self {
        Self {
            kind,
            url,
            language: None,
            start,
            start16,
            empty_fill: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(r: &Rendered, kind: EntityType) -> &MessageEntity {
        r.entities
            .iter()
            .find(|e| e.entity_type == kind)
            .expect("entity not found")
    }

    fn covered(r: &Rendered, e: &MessageEntity) -> String {
        // Decode the UTF-16 range back out of the rendered text — the same
        // arithmetic Telegram applies.
        let utf16: Vec<u16> = r.text.encode_utf16().collect();
        String::from_utf16(&utf16[e.offset as usize..(e.offset + e.length) as usize]).unwrap()
    }

    #[test]
    fn bold_next_to_cjk_and_fullwidth_punctuation() {
        let r = render("看学院：**工程学院 (Engineering)**：必须做");
        assert_eq!(r.text, "看学院：工程学院 (Engineering)：必须做");
        let bold = entity(&r, EntityType::Bold);
        assert_eq!(covered(&r, bold), "工程学院 (Engineering)");
    }

    #[test]
    fn utf16_offsets_count_emoji_as_two() {
        let r = render("🎉 **yes**");
        let bold = entity(&r, EntityType::Bold);
        // 🎉 is a surrogate pair (2 units) plus the space.
        assert_eq!(bold.offset, 3);
        assert_eq!(bold.length, 3);
        assert_eq!(covered(&r, bold), "yes");
    }

    #[test]
    fn links_become_text_link() {
        let r = render("see [the docs](https://example.com/a?b=1) now");
        let link = entity(&r, EntityType::TextLink);
        assert_eq!(link.url.as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(covered(&r, link), "the docs");
    }

    #[test]
    fn fenced_code_is_pre_with_language() {
        let r = render("before\n\n```rust\nlet x = **not bold**;\n```\n\nafter");
        let pre = entity(&r, EntityType::Pre);
        assert_eq!(pre.language.as_deref(), Some("rust"));
        assert_eq!(covered(&r, pre), "let x = **not bold**;");
        assert!(r.text.contains("\n\nafter"));
    }

    #[test]
    fn nested_emphasis_produces_nested_entities() {
        let r = render("**bold _both_ end**");
        assert_eq!(covered(&r, entity(&r, EntityType::Bold)), "bold both end");
        assert_eq!(covered(&r, entity(&r, EntityType::Italic)), "both");
    }

    #[test]
    fn headings_render_as_bold_lines() {
        let r = render("# 标题\nbody");
        assert_eq!(covered(&r, entity(&r, EntityType::Bold)), "标题");
    }

    #[test]
    fn lists_keep_literal_markers() {
        let r = render("1. one\n2. two\n\n- a\n- b");
        // The bullet list parses as nested under the ordered one — it
        // renders indented, which reads correctly in a chat.
        assert_eq!(r.text, "1. one\n2. two\n  - a\n  - b\n");
    }

    #[test]
    fn unmatched_markers_stay_literal() {
        let r = render("this is ** not bold");
        assert_eq!(r.text, "this is ** not bold");
        assert!(r.entities.is_empty());
    }

    #[test]
    fn blockquote_covers_its_lines() {
        let r = render("> first\n> second\n\nrest");
        assert_eq!(
            covered(&r, entity(&r, EntityType::Blockquote)),
            "first\nsecond"
        );
    }

    #[test]
    fn image_without_alt_shows_url() {
        let r = render("pic: ![](https://example.com/x.png)");
        let link = entity(&r, EntityType::TextLink);
        assert_eq!(covered(&r, link), "https://example.com/x.png");
    }

    #[test]
    fn plain_text_is_unchanged() {
        let r = render("just words 中文混排 ok");
        assert_eq!(r.text, "just words 中文混排 ok");
        assert!(r.entities.is_empty());
    }
}
