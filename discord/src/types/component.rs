//! Conversion from botkit's unified components to Discord's wire format.
//!
//! The shared [`Component`] type serializes to botkit's own representation,
//! which is not what Discord's API accepts — it wants numeric component and
//! style codes. Rendering here keeps Discord's wire format out of the core
//! crate, the same way the Telegram adapter renders inline keyboards.

use botkit_core::types::{Button, ButtonStyle, Component, SelectMenu};
use serde_json::{Value, json};

/// Discord component type codes.
const ACTION_ROW: u8 = 1;
const BUTTON: u8 = 2;
const STRING_SELECT: u8 = 3;

/// Render components for a Discord message payload.
///
/// Bare buttons are wrapped in an action row, which Discord requires: a
/// message's top-level `components` may only contain rows.
pub fn to_discord(components: &[Component]) -> Vec<Value> {
    let mut rows = Vec::new();
    // Consecutive loose buttons share one row rather than each getting their own.
    let mut loose: Vec<Value> = Vec::new();

    for component in components {
        match component {
            Component::ActionRow(row) => {
                flush(&mut loose, &mut rows);
                let children: Vec<_> = row.components.iter().filter_map(child).collect();
                if !children.is_empty() {
                    rows.push(json!({ "type": ACTION_ROW, "components": children }));
                }
            }
            Component::Button(button) => loose.push(button_json(button)),
            Component::SelectMenu(menu) => {
                flush(&mut loose, &mut rows);
                // Discord requires a select menu to be alone in its row.
                rows.push(json!({ "type": ACTION_ROW, "components": [select_json(menu)] }));
            }
        }
    }

    flush(&mut loose, &mut rows);
    rows
}

fn flush(loose: &mut Vec<Value>, rows: &mut Vec<Value>) {
    if !loose.is_empty() {
        rows.push(json!({ "type": ACTION_ROW, "components": std::mem::take(loose) }));
    }
}

/// Render a component nested inside an action row.
fn child(component: &Component) -> Option<Value> {
    match component {
        Component::Button(button) => Some(button_json(button)),
        Component::SelectMenu(menu) => Some(select_json(menu)),
        // Discord does not allow nested action rows.
        Component::ActionRow(_) => None,
    }
}

fn button_json(button: &Button) -> Value {
    let mut json = json!({
        "type": BUTTON,
        "style": style_code(button.style),
        "label": button.label,
        "disabled": button.disabled,
    });

    // A link button carries a URL; every other style carries a custom id.
    match button.style {
        ButtonStyle::Link => {
            if let Some(url) = &button.url {
                json["url"] = json!(url);
            }
        }
        _ => {
            if let Some(custom_id) = &button.custom_id {
                json["custom_id"] = json!(custom_id);
            }
        }
    }

    if let Some(emoji) = &button.emoji {
        json["emoji"] = emoji_json(emoji);
    }

    json
}

fn select_json(menu: &SelectMenu) -> Value {
    let options: Vec<_> = menu
        .options
        .iter()
        .map(|option| {
            let mut json = json!({
                "label": option.label,
                "value": option.value,
                "default": option.default,
            });
            if let Some(description) = &option.description {
                json["description"] = json!(description);
            }
            if let Some(emoji) = &option.emoji {
                json["emoji"] = emoji_json(emoji);
            }
            json
        })
        .collect();

    let mut json = json!({
        "type": STRING_SELECT,
        "custom_id": menu.custom_id,
        "options": options,
        "min_values": menu.min_values,
        "max_values": menu.max_values,
        "disabled": menu.disabled,
    });

    if let Some(placeholder) = &menu.placeholder {
        json["placeholder"] = json!(placeholder);
    }

    json
}

/// Custom emoji arrive as `name:id`; anything else is a Unicode emoji.
fn emoji_json(emoji: &str) -> Value {
    match emoji.rsplit_once(':') {
        Some((name, id)) if id.chars().all(|c| c.is_ascii_digit()) && !id.is_empty() => {
            json!({ "name": name, "id": id })
        }
        _ => json!({ "name": emoji }),
    }
}

fn style_code(style: ButtonStyle) -> u8 {
    match style {
        ButtonStyle::Primary => 1,
        ButtonStyle::Secondary => 2,
        ButtonStyle::Success => 3,
        ButtonStyle::Danger => 4,
        ButtonStyle::Link => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use botkit_core::types::{ActionRow, SelectOption};

    #[test]
    fn action_rows_use_discords_numeric_type_codes() {
        let components = vec![Component::ActionRow(ActionRow::buttons(vec![
            Button::primary("go", "Go"),
        ]))];

        assert_eq!(
            to_discord(&components),
            json!([{
                "type": 1,
                "components": [{
                    "type": 2,
                    "style": 1,
                    "label": "Go",
                    "disabled": false,
                    "custom_id": "go"
                }]
            }])
            .as_array()
            .unwrap()
            .clone()
        );
    }

    #[test]
    fn every_button_style_maps_to_its_discord_code() {
        for (style, code) in [
            (ButtonStyle::Primary, 1),
            (ButtonStyle::Secondary, 2),
            (ButtonStyle::Success, 3),
            (ButtonStyle::Danger, 4),
            (ButtonStyle::Link, 5),
        ] {
            assert_eq!(style_code(style), code);
        }
    }

    #[test]
    fn link_buttons_carry_a_url_and_no_custom_id() {
        let button = button_json(&Button::link("https://example.com", "Docs"));
        assert_eq!(button["style"], 5);
        assert_eq!(button["url"], "https://example.com");
        assert!(button.get("custom_id").is_none());
    }

    #[test]
    fn callback_buttons_carry_a_custom_id_and_no_url() {
        let button = button_json(&Button::danger("delete", "Delete"));
        assert_eq!(button["style"], 4);
        assert_eq!(button["custom_id"], "delete");
        assert!(button.get("url").is_none());
    }

    #[test]
    fn loose_buttons_are_wrapped_into_one_row() {
        let components = vec![
            Component::Button(Button::primary("a", "A")),
            Component::Button(Button::secondary("b", "B")),
        ];

        let rows = to_discord(&components);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["components"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn select_menus_get_a_row_of_their_own() {
        let components = vec![
            Component::Button(Button::primary("a", "A")),
            Component::SelectMenu(
                SelectMenu::new(
                    "pick",
                    vec![SelectOption::new("One", "1").description("first")],
                )
                .placeholder("Choose")
                .min_max(1, 2),
            ),
        ];

        let rows = to_discord(&components);
        assert_eq!(rows.len(), 2, "button row then select row");

        let select = &rows[1]["components"][0];
        assert_eq!(select["type"], 3);
        assert_eq!(select["custom_id"], "pick");
        assert_eq!(select["placeholder"], "Choose");
        assert_eq!(select["min_values"], 1);
        assert_eq!(select["max_values"], 2);
        assert_eq!(select["options"][0]["value"], "1");
        assert_eq!(select["options"][0]["description"], "first");
    }

    #[test]
    fn unicode_and_custom_emoji_are_distinguished() {
        assert_eq!(emoji_json("🎉"), json!({ "name": "🎉" }));
        assert_eq!(
            emoji_json("partyblob:123456789"),
            json!({ "name": "partyblob", "id": "123456789" })
        );
        // A colon that is not an id stays part of the name.
        assert_eq!(emoji_json("a:b"), json!({ "name": "a:b" }));
    }

    #[test]
    fn nested_action_rows_are_dropped() {
        let components = vec![Component::ActionRow(ActionRow::new(vec![
            Component::ActionRow(ActionRow::new(vec![])),
            Component::Button(Button::primary("a", "A")),
        ]))];

        let rows = to_discord(&components);
        assert_eq!(rows[0]["components"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn empty_rows_are_omitted() {
        assert!(to_discord(&[]).is_empty());
        assert!(to_discord(&[Component::ActionRow(ActionRow::new(vec![]))]).is_empty());
    }
}
