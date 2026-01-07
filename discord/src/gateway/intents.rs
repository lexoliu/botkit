/// Discord Gateway intents
#[derive(Debug, Clone, Copy, Default)]
pub struct GatewayIntents(u32);

impl GatewayIntents {
    /// No intents
    pub const fn empty() -> Self {
        Self(0)
    }

    /// All intents
    pub const fn all() -> Self {
        Self(0x3FFFF)
    }

    /// Guilds intent
    pub const GUILDS: Self = Self(1 << 0);

    /// Guild members intent (privileged)
    pub const GUILD_MEMBERS: Self = Self(1 << 1);

    /// Guild moderation intent
    pub const GUILD_MODERATION: Self = Self(1 << 2);

    /// Guild expressions intent
    pub const GUILD_EXPRESSIONS: Self = Self(1 << 3);

    /// Guild integrations intent
    pub const GUILD_INTEGRATIONS: Self = Self(1 << 4);

    /// Guild webhooks intent
    pub const GUILD_WEBHOOKS: Self = Self(1 << 5);

    /// Guild invites intent
    pub const GUILD_INVITES: Self = Self(1 << 6);

    /// Guild voice states intent
    pub const GUILD_VOICE_STATES: Self = Self(1 << 7);

    /// Guild presences intent (privileged)
    pub const GUILD_PRESENCES: Self = Self(1 << 8);

    /// Guild messages intent
    pub const GUILD_MESSAGES: Self = Self(1 << 9);

    /// Guild message reactions intent
    pub const GUILD_MESSAGE_REACTIONS: Self = Self(1 << 10);

    /// Guild message typing intent
    pub const GUILD_MESSAGE_TYPING: Self = Self(1 << 11);

    /// Direct messages intent
    pub const DIRECT_MESSAGES: Self = Self(1 << 12);

    /// Direct message reactions intent
    pub const DIRECT_MESSAGE_REACTIONS: Self = Self(1 << 13);

    /// Direct message typing intent
    pub const DIRECT_MESSAGE_TYPING: Self = Self(1 << 14);

    /// Message content intent (privileged)
    pub const MESSAGE_CONTENT: Self = Self(1 << 15);

    /// Guild scheduled events intent
    pub const GUILD_SCHEDULED_EVENTS: Self = Self(1 << 16);

    /// Auto moderation configuration intent
    pub const AUTO_MODERATION_CONFIGURATION: Self = Self(1 << 20);

    /// Auto moderation execution intent
    pub const AUTO_MODERATION_EXECUTION: Self = Self(1 << 21);

    /// Get the raw bits value
    pub const fn bits(&self) -> u32 {
        self.0
    }

    /// Create from raw bits
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Check if this contains another intent
    pub const fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Combine with another intent
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOr for GatewayIntents {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for GatewayIntents {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}
