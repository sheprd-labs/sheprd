//! Named destinations and highlights, stable across layouts and languages.
use serde::{Deserialize, Serialize};

macro_rules! pages {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum SettingsPage { $($variant),+ }
        impl SettingsPage {
            pub const ALL: &[Self] = &[$(Self::$variant),+];
            pub fn name(self) -> &'static str { match self { $(Self::$variant => $name),+ } }
            pub fn parse(name: &str) -> Option<Self> { Self::ALL.iter().copied().find(|page| page.name() == name) }
        }
    };
}
pages! {
    // keep-sorted start
    Chats => "chats",
    Computers => "computers",
    Delivery => "delivery",
    DeskHold => "desk-hold",
    Fleet => "fleet",
    Github => "github",
    Notifications => "notifications",
    Privacy => "privacy",
    Relays => "relays",
    Sounds => "sounds",
    TerminalKeys => "terminal-keys",
    TextShortcuts => "text-shortcuts",
    Voice => "voice",
    // keep-sorted end
}

macro_rules! highlights {
    ($($variant:ident => ($name:literal, $page:ident)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum Highlight { $($variant),+ }
        impl Highlight {
            pub const ALL: &[Self] = &[$(Self::$variant),+];
            pub fn name(self) -> &'static str { match self { $(Self::$variant => $name),+ } }
            pub fn page(self) -> SettingsPage { match self { $(Self::$variant => SettingsPage::$page),+ } }
            pub fn parse(name: &str) -> Option<Self> { Self::ALL.iter().copied().find(|target| target.name() == name) }
        }
    };
}
highlights! {
    // keep-sorted start
    ChatRows => ("chat-rows", Chats),
    FaceId => ("face-id", Privacy),
    Haptics => ("haptics", Sounds),
    LiveActivities => ("live-activities", Notifications),
    PushRelay => ("push-relay", Notifications),
    Sounds => ("sounds", Sounds),
    SpeakResults => ("speak-results", Voice),
    Storage => ("storage", Privacy),
    TestPush => ("test-push", Notifications),
    VoiceConfirmation => ("voice-confirmation", Voice),
    VoiceLanguage => ("voice-language", Voice),
    // keep-sorted end
}
