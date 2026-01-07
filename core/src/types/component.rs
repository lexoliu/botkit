use serde::{Deserialize, Serialize};

/// Interactive component types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Component {
    /// Row of components
    ActionRow(ActionRow),
    /// Clickable button
    Button(Button),
    /// Dropdown select menu
    SelectMenu(SelectMenu),
}

/// Container for a row of components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRow {
    pub components: Vec<Component>,
}

impl ActionRow {
    /// Create a new action row with the given components
    pub fn new(components: Vec<Component>) -> Self {
        Self { components }
    }

    /// Create an action row with buttons
    pub fn buttons(buttons: Vec<Button>) -> Self {
        Self {
            components: buttons.into_iter().map(Component::Button).collect(),
        }
    }
}

/// Button component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Button {
    /// Custom ID for identifying button clicks
    pub custom_id: Option<String>,
    /// Button label text
    pub label: String,
    /// Button style
    pub style: ButtonStyle,
    /// URL for link buttons
    pub url: Option<String>,
    /// Whether the button is disabled
    pub disabled: bool,
    /// Emoji to show on the button
    pub emoji: Option<String>,
}

impl Button {
    /// Create a primary button
    pub fn primary(custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            custom_id: Some(custom_id.into()),
            label: label.into(),
            style: ButtonStyle::Primary,
            url: None,
            disabled: false,
            emoji: None,
        }
    }

    /// Create a secondary button
    pub fn secondary(custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            custom_id: Some(custom_id.into()),
            label: label.into(),
            style: ButtonStyle::Secondary,
            url: None,
            disabled: false,
            emoji: None,
        }
    }

    /// Create a danger (red) button
    pub fn danger(custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            custom_id: Some(custom_id.into()),
            label: label.into(),
            style: ButtonStyle::Danger,
            url: None,
            disabled: false,
            emoji: None,
        }
    }

    /// Create a success (green) button
    pub fn success(custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            custom_id: Some(custom_id.into()),
            label: label.into(),
            style: ButtonStyle::Success,
            url: None,
            disabled: false,
            emoji: None,
        }
    }

    /// Create a link button
    pub fn link(url: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            custom_id: None,
            label: label.into(),
            style: ButtonStyle::Link,
            url: Some(url.into()),
            disabled: false,
            emoji: None,
        }
    }

    /// Set button as disabled
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    /// Add an emoji to the button
    pub fn with_emoji(mut self, emoji: impl Into<String>) -> Self {
        self.emoji = Some(emoji.into());
        self
    }
}

/// Button visual styles
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    Success,
    Danger,
    Link,
}

/// Dropdown select menu
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectMenu {
    /// Custom ID for identifying selections
    pub custom_id: String,
    /// Placeholder text when nothing is selected
    pub placeholder: Option<String>,
    /// Available options
    pub options: Vec<SelectOption>,
    /// Minimum number of selections
    pub min_values: u8,
    /// Maximum number of selections
    pub max_values: u8,
    /// Whether the menu is disabled
    pub disabled: bool,
}

impl SelectMenu {
    /// Create a new select menu
    pub fn new(custom_id: impl Into<String>, options: Vec<SelectOption>) -> Self {
        Self {
            custom_id: custom_id.into(),
            placeholder: None,
            options,
            min_values: 1,
            max_values: 1,
            disabled: false,
        }
    }

    /// Set placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set min/max selection values
    pub fn min_max(mut self, min: u8, max: u8) -> Self {
        self.min_values = min;
        self.max_values = max;
        self
    }
}

/// Option in a select menu
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    /// Display label
    pub label: String,
    /// Value sent when selected
    pub value: String,
    /// Description of the option
    pub description: Option<String>,
    /// Whether this option is selected by default
    pub default: bool,
    /// Emoji to show
    pub emoji: Option<String>,
}

impl SelectOption {
    /// Create a new select option
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            description: None,
            default: false,
            emoji: None,
        }
    }

    /// Add a description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Mark as default selected
    pub fn default(mut self) -> Self {
        self.default = true;
        self
    }
}
