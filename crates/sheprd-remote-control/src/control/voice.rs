//! Voice pairing over the private desktop control socket.
use super::{changes::AskId, github::Token};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    ElevenLabs,
    OpenAI,
    Deepgram,
    OpenRouter,
}

impl Provider {
    pub const ALL: [Self; 4] = [
        Self::ElevenLabs,
        Self::OpenAI,
        Self::Deepgram,
        Self::OpenRouter,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::ElevenLabs => "ElevenLabs",
            Self::OpenAI => "OpenAI",
            Self::Deepgram => "Deepgram",
            Self::OpenRouter => "OpenRouter",
        }
    }

    pub fn default_voice(self) -> &'static str {
        match self {
            Self::ElevenLabs => "JBFqnCBsd6RMkjVDRZzb",
            Self::OpenAI => "marin",
            Self::Deepgram => "aura-2-thalia-en",
            Self::OpenRouter => "en_paul_neutral",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub provider: Provider,
    pub voice: String,
    pub key: Token,
}

impl Account {
    pub fn valid(&self) -> bool {
        self.key.valid()
            && (1..=96).contains(&self.voice.len())
            && self
                .voice
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    pub has_account: bool,
    pub account: Option<Account>,
}

impl Offer {
    pub fn valid(&self) -> bool {
        self.account
            .as_ref()
            .is_none_or(|account| self.has_account && account.valid())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub account: Option<Account>,
    pub wants_account: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Ask {
    pub id: AskId,
    pub offer: Offer,
}
