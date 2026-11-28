//! Pure parser for persistent agent interaction directives in chat replies:
//! `:::modal`, `:::urgent`, `:::countdown`, `:::otp`, `:::oauth`, `:::secret`,
//! `:::progress`, `:::runbook`, `:::drop`, `:::diff`, `:::spotlight` (closed by
//! `:::` or end-of-message while streaming), plus single-line `::cue <name>`.

use sheprd_control_protocol::protocol::modal::{
    CueKind, MAX_BODY, MAX_CHOICES, MAX_STEPS, MAX_TITLE, ModalScope, RunbookStep,
};

/// The directive keyword that opened an interaction block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BlockDirective {
    // keep-sorted start
    Countdown,
    Diff,
    Drop,
    Modal,
    Oauth,
    Otp,
    Progress,
    Runbook,
    Secret,
    Spotlight,
    Urgent,
    // keep-sorted end
}

impl BlockDirective {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name {
            "countdown" => Some(Self::Countdown),
            "diff" => Some(Self::Diff),
            "drop" => Some(Self::Drop),
            "modal" => Some(Self::Modal),
            "oauth" => Some(Self::Oauth),
            "otp" => Some(Self::Otp),
            "progress" => Some(Self::Progress),
            "runbook" => Some(Self::Runbook),
            "secret" => Some(Self::Secret),
            "spotlight" => Some(Self::Spotlight),
            "urgent" => Some(Self::Urgent),
            _ => None,
        }
    }

    pub(crate) const fn kind_name(self) -> &'static str {
        match self {
            Self::Countdown => "countdown",
            Self::Diff => "diff",
            Self::Drop => "drop",
            Self::Modal => "modal",
            Self::Oauth => "oauth",
            Self::Otp => "otp",
            Self::Progress => "progress",
            Self::Runbook => "runbook",
            Self::Secret => "secret",
            Self::Spotlight => "spotlight",
            Self::Urgent => "urgent",
        }
    }

    /// Default scope when the directive does not override `scope:`.
    pub(crate) const fn default_scope(self) -> ModalScope {
        match self {
            Self::Urgent | Self::Otp | Self::Oauth | Self::Progress | Self::Drop => ModalScope::All,
            Self::Modal
            | Self::Countdown
            | Self::Secret
            | Self::Runbook
            | Self::Diff
            | Self::Spotlight => ModalScope::Window,
        }
    }

    /// Default semantic cue when the directive does not specify `cue:`.
    pub(crate) const fn default_cue(self) -> Option<CueKind> {
        match self {
            Self::Urgent | Self::Otp | Self::Oauth => Some(CueKind::Yubikey),
            Self::Modal | Self::Diff => Some(CueKind::Decision),
            Self::Secret => Some(CueKind::Secret),
            Self::Countdown => Some(CueKind::Countdown),
            Self::Progress | Self::Runbook | Self::Drop | Self::Spotlight => None,
        }
    }

    /// Whether a newer chat entry automatically clears this interaction unless
    /// overridden by `auto_clear:`. Progress trackers and shared runbooks
    /// span multiple chat turns so they stay pinned until done or dismissed.
    pub(crate) const fn default_auto_clear(self) -> bool {
        !matches!(self, Self::Progress | Self::Runbook)
    }
}

/// Typed payload parsed from an interaction directive block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Payload {
    Modal {
        preset: Option<String>,
        choices: Vec<String>,
        quick_reply: bool,
    },
    Countdown {
        seconds: u32,
    },
    Otp,
    Oauth {
        url: String,
        code: Option<String>,
    },
    Secret {
        key: String,
        dest: String,
        save_keychain: bool,
    },
    Progress {
        percent: Option<u8>,
        step: String,
        done: bool,
    },
    Runbook {
        steps: Vec<RunbookStep>,
    },
    Drop {
        accept: String,
    },
    Diff {
        file: String,
        diff: String,
    },
    Spotlight {
        target: String,
        step: Option<String>,
    },
}

/// A parsed interaction ready to be shown inline in chat and synced into the
/// global/window interaction store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedInteraction {
    pub(crate) directive: BlockDirective,
    pub(crate) scope: ModalScope,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) cue: Option<CueKind>,
    pub(crate) auto_clear: bool,
    pub(crate) payload: Payload,
}

/// Opening line of a block interaction directive (`:::urgent Tap YubiKey`).
pub(crate) fn opening(line: &str) -> Option<(BlockDirective, String)> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix(":::")?;
    if rest.is_empty() {
        return None;
    }
    let mut parts = rest.splitn(2, char::is_whitespace);
    let name = parts.next()?;
    let directive = BlockDirective::parse(name)?;
    let title = parts
        .next()
        .map(str::trim)
        .unwrap_or("")
        .chars()
        .take(MAX_TITLE)
        .collect();
    Some((directive, title))
}

/// Single-line semantic cue directive (`::cue yubikey`).
pub(crate) fn cue_line(line: &str) -> Option<CueKind> {
    let rest = line.trim().strip_prefix("::")?;
    if rest.starts_with(':') {
        return None;
    }
    let name = rest.strip_prefix("cue ")?;
    CueKind::parse(name)
}

/// Parses the inner lines of an interaction directive into a bounded
/// [`ParsedInteraction`].
pub(crate) fn parse_block(
    directive: BlockDirective,
    raw_title: &str,
    inner: &str,
) -> ParsedInteraction {
    let mut scope = directive.default_scope();
    let mut cue = directive.default_cue();
    let mut auto_clear = directive.default_auto_clear();
    let mut preset = None;
    let mut choices = Vec::new();
    let mut quick_reply = false;
    let mut seconds: u32 = 15;
    let mut url = String::new();
    let mut code = None;
    let mut key = String::new();
    let mut dest = String::new();
    let mut save_keychain = true;
    let mut percent = None;
    let mut step = String::new();
    let mut done = false;
    let mut accept = String::new();
    let mut file = String::new();
    let mut target = String::new();
    let mut step_label = None;

    let mut body_lines = Vec::new();
    let mut steps = Vec::new();
    let mut in_headers = true;

    for raw in inner.lines() {
        let trimmed = raw.trim();
        if directive == BlockDirective::Runbook
            && let Some(parsed_step) = parse_runbook_line(trimmed)
        {
            if steps.len() < MAX_STEPS {
                steps.push(parsed_step);
            }
            in_headers = false;
            continue;
        }
        if in_headers && let Some((k, v)) = split_header(trimmed) {
            match k {
                "scope" => {
                    scope = match v {
                        "all" | "global" => ModalScope::All,
                        "window" => ModalScope::Window,
                        _ => scope,
                    };
                    continue;
                }
                "cue" => {
                    if let Some(parsed) = CueKind::parse(v) {
                        cue = Some(parsed);
                    }
                    continue;
                }
                "auto_clear" => {
                    auto_clear = matches!(v, "true" | "1" | "yes");
                    continue;
                }
                "preset" => {
                    preset = (!v.is_empty()).then(|| clip(v, 32));
                    continue;
                }
                "choices" => {
                    choices = v
                        .split('|')
                        .map(str::trim)
                        .filter(|c| !c.is_empty())
                        .take(MAX_CHOICES)
                        .map(|c| clip(c, 80))
                        .collect();
                    continue;
                }
                "reply" | "quick_reply" => {
                    quick_reply = matches!(v, "true" | "1" | "yes");
                    continue;
                }
                "seconds" => {
                    if let Ok(parsed) = v.parse::<u32>() {
                        seconds = parsed.clamp(1, 300);
                    }
                    continue;
                }
                "url" => {
                    url = clip(v, 512);
                    continue;
                }
                "code" => {
                    code = (!v.is_empty()).then(|| clip(v, 64));
                    continue;
                }
                "key" => {
                    key = clip(v, 128);
                    continue;
                }
                "dest" => {
                    dest = clip(v, 256);
                    continue;
                }
                "save_keychain" => {
                    save_keychain = matches!(v, "true" | "1" | "yes");
                    continue;
                }
                "percent" => {
                    let clean = v.trim_end_matches('%');
                    if let Ok(parsed) = clean.parse::<u8>() {
                        percent = Some(parsed.min(100));
                    }
                    continue;
                }
                "step" => {
                    if directive == BlockDirective::Spotlight {
                        step_label = (!v.is_empty()).then(|| clip(v, 32));
                    } else {
                        step = clip(v, 160);
                    }
                    continue;
                }
                "done" => {
                    done = matches!(v, "true" | "1" | "yes");
                    if done {
                        cue = Some(CueKind::Done);
                    }
                    continue;
                }
                "accept" => {
                    accept = clip(v, 120);
                    continue;
                }
                "file" => {
                    file = clip(v, 256);
                    continue;
                }
                "target" => {
                    target = clip(v, 160);
                    continue;
                }
                _ => {}
            }
        }
        if !trimmed.is_empty() {
            in_headers = false;
        }
        body_lines.push(raw);
    }

    let raw_body = body_lines.join("\n");
    let body = clip(raw_body.trim(), MAX_BODY);
    let title = if raw_title.trim().is_empty() {
        default_title(directive).to_owned()
    } else {
        clip(raw_title.trim(), MAX_TITLE)
    };

    let payload = match directive {
        BlockDirective::Modal | BlockDirective::Urgent => Payload::Modal {
            preset,
            choices,
            quick_reply,
        },
        BlockDirective::Countdown => Payload::Countdown { seconds },
        BlockDirective::Otp => Payload::Otp,
        BlockDirective::Oauth => Payload::Oauth { url, code },
        BlockDirective::Secret => Payload::Secret {
            key,
            dest,
            save_keychain,
        },
        BlockDirective::Progress => Payload::Progress {
            percent,
            step,
            done,
        },
        BlockDirective::Runbook => Payload::Runbook { steps },
        BlockDirective::Drop => Payload::Drop { accept },
        BlockDirective::Diff => Payload::Diff {
            file,
            diff: body.clone(),
        },
        BlockDirective::Spotlight => Payload::Spotlight {
            target,
            step: step_label,
        },
    };

    ParsedInteraction {
        directive,
        scope,
        title,
        body: if directive == BlockDirective::Diff {
            String::new()
        } else {
            body
        },
        cue,
        auto_clear,
        payload,
    }
}

fn default_title(directive: BlockDirective) -> &'static str {
    match directive {
        BlockDirective::Modal => "Agent needs your attention",
        BlockDirective::Urgent => "Action required",
        BlockDirective::Countdown => "Confirm scheduled action",
        BlockDirective::Otp => "Enter 2FA code",
        BlockDirective::Oauth => "Complete sign-in",
        BlockDirective::Secret => "Provide secret securely",
        BlockDirective::Progress => "Working…",
        BlockDirective::Runbook => "Shared checklist",
        BlockDirective::Drop => "Drop a file for the agent",
        BlockDirective::Diff => "Approve change?",
        BlockDirective::Spotlight => "Agent highlight",
    }
}

fn split_header(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        return None;
    }
    Some((key, value.trim()))
}

fn parse_runbook_line(line: &str) -> Option<RunbookStep> {
    let rest = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))?;
    let (done, rest) = if let Some(after) = rest
        .strip_prefix("[x] ")
        .or_else(|| rest.strip_prefix("[X] "))
    {
        (true, after)
    } else {
        (false, rest.strip_prefix("[ ] ")?)
    };
    let (human, label) = if let Some(after) = rest.strip_prefix("you:") {
        (true, after.trim())
    } else if let Some(after) = rest.strip_prefix("human:") {
        (true, after.trim())
    } else if let Some(after) = rest.strip_prefix("agent:") {
        (false, after.trim())
    } else {
        (true, rest.trim())
    };
    if label.is_empty() {
        return None;
    }
    Some(RunbookStep {
        label: clip(label, 180),
        human,
        done,
    })
}

fn clip(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}
