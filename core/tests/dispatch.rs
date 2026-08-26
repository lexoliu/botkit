//! End-to-end dispatch: register handlers, route an event, read the response.
//!
//! Exercises the seam every adapter sits on — `BotBuilder` routing, extractor
//! application, and `IntoResponse` conversion — without any network.

use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use botkit_core::types::{ActionRow, Button, Component, Embed};
use botkit_core::{
    BotBuilder, ButtonId, CommandArgs, Context, ContextData, Event, OptionValue, Response, User,
};
use futures_lite::future::block_on;

/// A context whose fields the test controls.
struct FakeContext {
    channel_id: String,
    user_id: String,
    user_name: String,
    command_name: Option<String>,
    command_args: Option<String>,
    button_id: Option<String>,
    message_content: Option<String>,
}

impl FakeContext {
    fn command(name: &str, args: &str) -> Self {
        Self {
            command_name: Some(name.into()),
            command_args: Some(args.into()),
            ..Self::base()
        }
    }

    fn button(id: &str) -> Self {
        Self {
            button_id: Some(id.into()),
            ..Self::base()
        }
    }

    fn message(text: &str) -> Self {
        Self {
            message_content: Some(text.into()),
            ..Self::base()
        }
    }

    fn base() -> Self {
        Self {
            channel_id: "chan".into(),
            user_id: "u1".into(),
            user_name: "Ada".into(),
            command_name: None,
            command_args: None,
            button_id: None,
            message_content: None,
        }
    }
}

impl ContextData for FakeContext {
    fn channel_id(&self) -> &str {
        &self.channel_id
    }
    fn user_id(&self) -> &str {
        &self.user_id
    }
    fn user_name(&self) -> &str {
        &self.user_name
    }
    fn command_name(&self) -> Option<&str> {
        self.command_name.as_deref()
    }
    fn command_args(&self) -> Option<&str> {
        self.command_args.as_deref()
    }
    fn option(&self, _name: &str) -> Option<OptionValue> {
        None
    }
    fn button_id(&self) -> Option<&str> {
        self.button_id.as_deref()
    }
    fn message_content(&self) -> Option<&str> {
        self.message_content.as_deref()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Route an event and run whatever handler it lands on.
fn dispatch(builder: &BotBuilder, event: Event<'_>, data: FakeContext) -> Option<Response> {
    let handler = builder.route(event)?.clone();
    Some(block_on(handler.call(Context::new(data))))
}

async fn ping() -> &'static str {
    "Pong!"
}

async fn greet(user: User) -> String {
    format!("Hello, {}!", user.name)
}

async fn echo(args: CommandArgs) -> String {
    args.0
}

async fn pressed(id: ButtonId) -> Response {
    Response::text(format!("You pressed {}", id.0)).ephemeral()
}

fn bot() -> BotBuilder {
    BotBuilder::new()
        .command("ping", ping)
        .command_with_description("greet", "Say hello", greet)
        .command("echo", echo)
        .button("confirm_*", pressed)
        .message(
            |content: botkit_core::MessageContent| async move { format!("heard: {}", content.0) },
        )
}

#[test]
fn commands_reach_their_handler() {
    let response = dispatch(
        &bot(),
        Event::Command("ping"),
        FakeContext::command("ping", ""),
    )
    .expect("routed");
    assert_eq!(response.content(), Some("Pong!"));
}

#[test]
fn extractors_read_from_the_dispatched_context() {
    let response = dispatch(
        &bot(),
        Event::Command("greet"),
        FakeContext::command("greet", ""),
    )
    .expect("routed");
    assert_eq!(response.content(), Some("Hello, Ada!"));

    let response = dispatch(
        &bot(),
        Event::Command("echo"),
        FakeContext::command("echo", "say this back"),
    )
    .expect("routed");
    assert_eq!(response.content(), Some("say this back"));
}

#[test]
fn wildcard_buttons_reach_their_handler_with_the_full_id() {
    let response = dispatch(
        &bot(),
        Event::Button("confirm_delete"),
        FakeContext::button("confirm_delete"),
    )
    .expect("routed");

    assert_eq!(response.content(), Some("You pressed confirm_delete"));
    assert!(response.is_ephemeral());
}

#[test]
fn the_message_handler_catches_everything_else() {
    let response =
        dispatch(&bot(), Event::Message, FakeContext::message("hi there")).expect("routed");
    assert_eq!(response.content(), Some("heard: hi there"));
}

#[test]
fn unregistered_events_route_nowhere() {
    let bot = bot();
    assert!(bot.route(Event::Command("nope")).is_none());
    assert!(bot.route(Event::Button("nope")).is_none());
}

#[test]
fn handlers_are_shared_not_copied_per_dispatch() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);

    let bot = BotBuilder::new().command("count", move || {
        let counter = Arc::clone(&counter);
        async move {
            counter.fetch_add(1, Ordering::Relaxed);
            "counted"
        }
    });

    for _ in 0..3 {
        dispatch(
            &bot,
            Event::Command("count"),
            FakeContext::command("count", ""),
        )
        .expect("routed");
    }

    assert_eq!(calls.load(Ordering::Relaxed), 3);
}

#[test]
fn rich_responses_survive_the_round_trip() {
    let bot = BotBuilder::new().command("menu", || async {
        Response::text("Pick one")
            .with_embed(Embed::new().title("Menu").description("Choose wisely"))
            .with_components(vec![Component::ActionRow(ActionRow::buttons(vec![
                Button::primary("yes", "Yes"),
                Button::link("https://example.com", "Docs"),
            ]))])
    });

    let response = dispatch(
        &bot,
        Event::Command("menu"),
        FakeContext::command("menu", ""),
    )
    .expect("routed");

    assert_eq!(response.content(), Some("Pick one"));
    assert_eq!(response.embeds().len(), 1);
    assert_eq!(response.embeds()[0].title.as_deref(), Some("Menu"));
    assert_eq!(response.components().len(), 1);
}

#[test]
fn command_descriptions_are_available_for_platform_menus() {
    let commands: Vec<_> = bot()
        .commands()
        .map(|c| (c.name.to_string(), c.description.to_string()))
        .collect();

    assert_eq!(
        commands,
        vec![
            ("ping".to_string(), String::new()),
            ("greet".to_string(), "Say hello".to_string()),
            ("echo".to_string(), String::new()),
        ]
    );
}
