//! Optional behaviors under `[features]`. Each flag is declared once below,
//! which generates its config field, its `Feature` variant, and the messages
//! (`features.ftl`) Settings > General lists it with. An entry may carry
//! attributes, such as a `#[cfg]` naming the Cargo feature it needs: a
//! build without it has no field, variant, or row, and ignores the key.
use serde::Deserialize;

macro_rules! features {
    ($($(#[$attr:meta])* $field:ident: $variant:ident = $default:expr, $message:literal, $description:literal;)+) => {
        /// Optional behaviors the config file turns on or off. Each flag
        /// declares its own default, so a missing or empty `[features]` table
        /// is the shipped experience, and a table naming one flag leaves the
        /// others at theirs.
        #[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
        #[serde(default)]
        pub struct Features {
            $($(#[$attr])* #[doc = concat!("Described by the `", $description, "` message.")] pub $field: bool,)+
        }

        impl Default for Features {
            fn default() -> Self {
                Self {
                    $($(#[$attr])* $field: $default,)+
                }
            }
        }

        /// One `[features]` flag, for code that lists or edits them all.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Feature {
            $($(#[$attr])* $variant,)+
        }

        impl Feature {
            /// Every flag, alphabetical by key, as Settings lists them.
            pub const ALL: &[Self] = &[$($(#[$attr])* Self::$variant,)+];

            /// The key under `[features]`.
            pub const fn key(self) -> &'static str {
                match self {
                    $($(#[$attr])* Self::$variant => stringify!($field),)+
                }
            }

            /// The id of the flag's row in Settings > General, which its
            /// search entry and debug selector share.
            pub const fn setting_id(self) -> &'static str {
                match self {
                    $($(#[$attr])* Self::$variant => concat!("general-feature-", stringify!($field)),)+
                }
            }

            /// The message naming the flag.
            pub const fn message(self) -> &'static str {
                match self {
                    $($(#[$attr])* Self::$variant => $message,)+
                }
            }

            /// The message describing the flag.
            pub const fn description_message(self) -> &'static str {
                match self {
                    $($(#[$attr])* Self::$variant => $description,)+
                }
            }
        }

        impl Features {
            pub fn enabled(&self, feature: Feature) -> bool {
                match feature {
                    $($(#[$attr])* Feature::$variant => self.$field,)+
                }
            }
        }
    };
}

// Settings lists the flags in this order, alphabetical by key, so additions
// from parallel branches land on different lines.
features! {
    // keep-sorted start
    acp_chats: AcpChats = true, "features-acp-chats",
        "features-acp-chats-description";
    // Each cloud provider built in, read once at startup (`crate::cloud::enabled`)
    // and only while `cloud_providers` is on. Off, the provider is offered
    // nowhere and its saved devices stay in the file, hidden until it is back.
    #[cfg(feature = "azure")]
    cloud_azure: CloudAzure = true, "features-cloud-azure",
        "features-cloud-azure-description";
    #[cfg(feature = "boat")]
    cloud_boat: CloudBoat = true, "features-cloud-boat",
        "features-cloud-boat-description";
    #[cfg(feature = "boxd")]
    cloud_boxd: CloudBoxd = true, "features-cloud-boxd",
        "features-cloud-boxd-description";
    #[cfg(feature = "coder")]
    cloud_coder: CloudCoder = true, "features-cloud-coder",
        "features-cloud-coder-description";
    #[cfg(feature = "container")]
    cloud_container: CloudContainer = true, "features-cloud-container",
        "features-cloud-container-description";
    #[cfg(feature = "daytona")]
    cloud_daytona: CloudDaytona = true, "features-cloud-daytona",
        "features-cloud-daytona-description";
    #[cfg(feature = "opensandbox")]
    cloud_opensandbox: CloudOpenSandbox = true, "features-cloud-opensandbox",
        "features-cloud-opensandbox-description";
    #[cfg(feature = "cloud")]
    cloud_providers: CloudProviders = true, "features-cloud-providers",
        "features-cloud-providers-description";
    #[cfg(feature = "railway")]
    cloud_railway: CloudRailway = true, "features-cloud-railway",
        "features-cloud-railway-description";
    #[cfg(feature = "sprites")]
    cloud_sprites: CloudSprites = true, "features-cloud-sprites",
        "features-cloud-sprites-description";
    // `sheprd mcp`, the MCP server agents and other clients start
    // (`docs/mcp-plan.md`). The app refuses its requests while this is off.
    #[cfg(feature = "mcp")]
    mcp: Mcp = false, "features-mcp",
        "features-mcp-description";
    moonframe_tools: MoonframeTools = false, "features-moonframe-tools",
        "features-moonframe-tools-description";
    moonframe_window: MoonframeWindow = false, "features-moonframe-window",
        "features-moonframe-window-description";
    qa_menu: QaMenu = false, "features-qa-menu",
        "features-qa-menu-description";
    sidebar_hover_menu: SidebarHoverMenu = false, "features-sidebar-hover-menu",
        "features-sidebar-hover-menu-description";
    // keep-sorted end
}
