//! The pull requests a finished tool call worked on, read from the command
//! it ran and what that printed: `gh pr` commands that change a pull
//! request, `gh api` writes to one, and `git push`. Commands that only read,
//! such as `gh pr view` or `gh pr list`, name none.
//!
//! The command is read whole from the call's input when the session kept
//! it, else from its title, and split the way a shell would only as far as
//! quotes and command separators go; anything subtler is left unread, as a
//! missed pull request costs less than a wrong one. Each pull request a
//! command names gets that command's actions alone. Only github.com
//! addresses are kept, through [`PullRequest::new`]; a command aimed at
//! another host names none.

use sheprd_chat_protocol::{
    PullRequest, PullRequestAction as Action, Tool, ToolContent, ToolKind, ToolStatus,
};

/// Pull requests one call may name; the rest are left out.
pub(crate) const MAX_FOUND: usize = 8;
/// The bytes of a command read, and of each text a call printed.
const MAX_COMMAND: usize = 16 * 1024;
const MAX_OUTPUT: usize = 64 * 1024;
const HOST: &str = "https://github.com/";

/// What one command of a call did, and the pull request its arguments
/// name, if they name one.
#[derive(Debug)]
struct Work {
    action: Action,
    named: Option<PullRequest>,
    /// A `gh pr` command, whose messages name what it changed.
    gh_pr: bool,
}

/// The pull requests `tool` worked on, each once with what was done to it.
/// `command` is the whole command the call ran, when the session kept it
/// from the call's input; the title shows only its first line.
pub(crate) fn found(tool: &Tool, command: Option<&str>) -> Vec<PullRequest> {
    if tool.status != ToolStatus::Completed
        || !matches!(tool.kind, ToolKind::Execute | ToolKind::Other)
    {
        return Vec::new();
    }
    let text = command.map_or_else(
        || command_text(&tool.title),
        |command| head(command, MAX_COMMAND).to_owned(),
    );
    let work: Vec<Work> = commands(&words(without_here_documents(&text)))
        .filter(|command| !elsewhere(command))
        .filter_map(|command| {
            gh_pr(&command)
                .or_else(|| gh_api(&command))
                .or_else(|| git_push(&command))
        })
        .collect();
    let mut pulls: Vec<PullRequest> = Vec::new();
    let mut record = |mut pull: PullRequest, actions: &[Action]| {
        if let Some(known) = pulls.iter_mut().find(|known| known.is(&pull)) {
            known.record(actions);
        } else if pulls.len() < MAX_FOUND {
            pull.record(actions);
            pulls.push(pull);
        }
    };
    for command in &work {
        if let Some(pull) = &command.named {
            record(pull.clone(), &[command.action]);
        }
    }
    // What the call printed names the pull requests of the commands that
    // named none, such as `gh pr create`; those commands' actions are
    // theirs. A pull request a command named keeps only its own.
    let unnamed: Vec<Action> = work
        .iter()
        .filter(|command| command.named.is_none())
        .map(|command| command.action)
        .collect();
    if unnamed.is_empty() {
        return pulls;
    }
    let gh_pr = work
        .iter()
        .any(|command| command.gh_pr && command.named.is_none());
    let printed = tool.content.iter().filter_map(|content| match content {
        ToolContent::Text { text } => Some(head(text, MAX_OUTPUT)),
        _ => None,
    });
    for text in printed {
        let messages = gh_pr.then(|| messages(text)).into_iter().flatten();
        for pull in addresses(text).chain(messages) {
            let named = work
                .iter()
                .any(|command| command.named.as_ref().is_some_and(|known| known.is(&pull)));
            if !named {
                record(pull, &unnamed);
            }
        }
    }
    pulls
}

/// The command a call's title shows: agents such as Claude Code put it in
/// backticks, escaping those inside, and a command of several lines shows
/// its first, followed by how many more there are.
fn command_text(title: &str) -> String {
    let title = head(title.trim(), MAX_COMMAND);
    let inner = title
        .strip_prefix('`')
        .and_then(|rest| {
            rest.strip_suffix('`').or_else(|| {
                let (first, more) = rest.rsplit_once("` (+")?;
                more.ends_with(" more line)")
                    .then_some(first)
                    .or_else(|| more.ends_with(" more lines)").then_some(first))
            })
        })
        .unwrap_or(title);
    inner.replace("\\`", "`")
}

/// Whether `command` sets `GH_HOST` to a host other than github.com, so
/// what `gh` changes is not a pull request this can name.
fn elsewhere(command: &[&str]) -> bool {
    command.iter().any(|word| {
        word.strip_prefix("GH_HOST=")
            .is_some_and(|host| !is_github(host))
    })
}

fn is_github(host: &str) -> bool {
    host.eq_ignore_ascii_case("github.com")
}

/// `command` without the bodies of its here-documents (`<<EOF` … `EOF`),
/// which are a file's text, not commands. A body never closed runs to the
/// end, so what follows an unreadable one is left unread too.
fn without_here_documents(command: &str) -> String {
    let mut kept = String::with_capacity(command.len());
    let mut pending: Vec<(String, bool)> = Vec::new();
    let mut lines = command.split_inclusive('\n');
    while let Some(line) = lines.next() {
        kept.push_str(line);
        pending.extend(delimiters(line));
        for (delimiter, tabs) in pending.drain(..) {
            for body in lines.by_ref() {
                let body = body.trim_end_matches(['\n', '\r']);
                let body = if tabs {
                    body.trim_start_matches('\t')
                } else {
                    body
                };
                if body == delimiter {
                    break;
                }
            }
        }
    }
    kept
}

/// The here-documents `line` starts, in order: each delimiter, and whether
/// `<<-` lets its closing line be indented with tabs. A `<<` in quotes or a
/// comment is text, and `<<<` is a string, not a document.
fn delimiters(line: &str) -> Vec<(String, bool)> {
    let mut found = Vec::new();
    let mut quote = None;
    let mut chars = line.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        match (quote, ch) {
            (Some(open), ch) if ch == open => quote = None,
            (Some('"'), '\\') => {
                chars.next();
            }
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(ch),
            (None, '\\') => {
                chars.next();
            }
            (None, '#') if line[..at].ends_with(char::is_whitespace) || at == 0 => break,
            (None, '<') if line[at..].starts_with("<<<") => {
                while chars.next_if(|&(_, ch)| ch == '<').is_some() {}
            }
            (None, '<') if line[at..].starts_with("<<") => {
                chars.next();
                let after = &line[at + 2..];
                let (tabs, after) = match after.strip_prefix('-') {
                    Some(after) => (true, after),
                    None => (false, after),
                };
                let word: String = after
                    .trim_start()
                    .chars()
                    .take_while(|ch| {
                        !ch.is_whitespace() && !matches!(ch, ';' | '&' | '|' | ')' | '<' | '>')
                    })
                    .filter(|ch| !matches!(ch, '\'' | '"' | '\\'))
                    .collect();
                if !word.is_empty() {
                    found.push((word, tabs));
                }
            }
            (None, _) => {}
        }
    }
    found
}

/// At most `limit` bytes of `text`, cut at a character.
fn head(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// A word of a command, or a separator between two commands.
#[derive(Debug, PartialEq, Eq)]
enum Word {
    Text(String),
    Break,
}

/// `command` in words, with quotes taken off and separators (`;`, `&`, `|`,
/// a newline, or a parenthesis) between commands.
fn words(command: String) -> Vec<Word> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut chars = command.chars();
    let end_word = |words: &mut Vec<Word>, word: &mut String, started: &mut bool| {
        if *started {
            words.push(Word::Text(std::mem::take(word)));
            *started = false;
        }
    };
    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                started = true;
                word.extend(chars.by_ref().take_while(|&ch| ch != '\''));
            }
            '"' => {
                started = true;
                while let Some(ch) = chars.next() {
                    match ch {
                        '"' => break,
                        '\\' => word.extend(chars.next()),
                        ch => word.push(ch),
                    }
                }
            }
            '\\' => {
                started = true;
                word.extend(chars.next().filter(|&ch| ch != '\n'));
            }
            ';' | '&' | '|' | '\n' | '(' | ')' => {
                end_word(&mut words, &mut word, &mut started);
                if words.last().is_some_and(|last| *last != Word::Break) {
                    words.push(Word::Break);
                }
            }
            ch if ch.is_whitespace() => end_word(&mut words, &mut word, &mut started),
            ch => {
                started = true;
                word.push(ch);
            }
        }
    }
    end_word(&mut words, &mut word, &mut started);
    words
}

/// Each command's words, between separators.
fn commands(words: &[Word]) -> impl Iterator<Item = Vec<&str>> {
    words.split(|word| *word == Word::Break).map(|command| {
        command
            .iter()
            .filter_map(|word| match word {
                Word::Text(text) => Some(text.as_str()),
                Word::Break => None,
            })
            .collect()
    })
}

/// Whether `word` runs `program`, by any path.
fn runs(word: &str, program: &str) -> bool {
    let name = word.rsplit(['/', '\\']).next().unwrap_or(word);
    name.strip_suffix(".exe").unwrap_or(name) == program
}

/// The words after `gh SUBCOMMAND`, wherever it starts in `command`.
fn gh<'a>(command: &'a [&'a str], subcommand: &str) -> Option<&'a [&'a str]> {
    let at = command
        .windows(2)
        .position(|pair| runs(pair[0], "gh") && pair[1] == subcommand)?;
    Some(&command[at + 2..])
}

/// `gh pr` options that take a value as the next word, for every verb
/// read; `merge` and `review` have their own, where `-r`, `-m`, and `-c`
/// are switches.
const PR_VALUES: &[&str] = &[
    "--add-assignee",
    "--add-label",
    "--add-project",
    "--add-reviewer",
    "--assignee",
    "--base",
    "--body",
    "--body-file",
    "--comment",
    "--head",
    "--label",
    "--milestone",
    "--project",
    "--recover",
    "--remove-assignee",
    "--remove-label",
    "--remove-project",
    "--remove-reviewer",
    "--repo",
    "--reviewer",
    "--template",
    "--title",
    "-B",
    "-F",
    "-H",
    "-R",
    "-T",
    "-a",
    "-b",
    "-c",
    "-l",
    "-m",
    "-p",
    "-r",
    "-t",
];
const REVIEW_VALUES: &[&str] = &["--body", "--body-file", "--repo", "-F", "-R", "-b"];
const MERGE_VALUES: &[&str] = &[
    "--author-email",
    "--body",
    "--body-file",
    "--match-head-commit",
    "--repo",
    "--subject",
    "-A",
    "-F",
    "-R",
    "-b",
    "-t",
];

/// A `gh pr` command that changes a pull request.
fn gh_pr(command: &[&str]) -> Option<Work> {
    let (verb, args) = gh(command, "pr")?.split_first()?;
    let flag = |name: &str| args.contains(&name);
    let action = match *verb {
        "create" | "new" => Action::Created,
        "edit" => Action::Edited,
        "merge" if flag("--auto") || flag("--disable-auto") => Action::Edited,
        "merge" => Action::Merged,
        "comment" => Action::Commented,
        "review" => Action::Reviewed,
        "ready" if flag("--undo") => Action::Edited,
        "ready" => Action::Ready,
        "close" => Action::Closed,
        "reopen" => Action::Reopened,
        _ => return None,
    };
    let values = match *verb {
        "merge" => MERGE_VALUES,
        "review" => REVIEW_VALUES,
        _ => PR_VALUES,
    };
    let mut repo = None;
    let mut target = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--repo=") {
            repo = Some(value);
        } else if values.contains(arg) {
            let value = args.next().copied();
            if matches!(*arg, "-R" | "--repo") {
                repo = value;
            }
        } else if target.is_none() && !arg.starts_with('-') {
            target = Some(*arg);
        }
    }
    // The pull request a command names: its address, or its number with
    // `--repo`. Without either, its own messages name it. `create` takes
    // none: what it makes is in what it prints.
    let named = target
        .filter(|_| action != Action::Created)
        .and_then(|arg| {
            PullRequest::parse(arg).or_else(|| {
                let number = arg.strip_prefix('#').unwrap_or(arg).parse().ok()?;
                PullRequest::new(repo?, number, Vec::new())
            })
        });
    Some(Work {
        action,
        named,
        gh_pr: true,
    })
}

/// `gh api` options that take a value as the next word.
const API_VALUES: &[&str] = &[
    "--cache",
    "--field",
    "--header",
    "--hostname",
    "--input",
    "--jq",
    "--method",
    "--preview",
    "--raw-field",
    "--template",
    "-F",
    "-H",
    "-X",
    "-f",
    "-p",
    "-q",
    "-t",
];

/// A `gh api` request that writes to a pull request:
/// `repos/OWNER/REPO/pulls/NUMBER[/PART]` with a method other than `GET`,
/// or with fields, which `gh` sends as a `POST`.
fn gh_api(command: &[&str]) -> Option<Work> {
    let args = gh(command, "api")?;
    let mut path = None;
    let mut method = None;
    let mut fields = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value)),
            _ => (*arg, None),
        };
        if !API_VALUES.contains(&name) {
            if !arg.starts_with('-') && path.is_none() {
                path = Some(*arg);
            }
            continue;
        }
        let value = inline.or_else(|| args.next().copied());
        match name {
            "--hostname" if !value.is_some_and(is_github) => return None,
            "-X" | "--method" => method = value,
            "-f" | "-F" | "--field" | "--raw-field" => fields.extend(value),
            "--input" => fields.push(""),
            _ => {}
        }
    }
    let writes = method.map_or(!fields.is_empty(), |method| {
        !method.eq_ignore_ascii_case("GET")
    });
    if !writes {
        return None;
    }
    let mut parts = path?.trim_start_matches('/').split('/');
    let (repos, owner, repo, pulls, number) = (
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
    );
    if repos != "repos" || pulls != "pulls" {
        return None;
    }
    let pull = PullRequest::new(&format!("{owner}/{repo}"), number.parse().ok()?, Vec::new())?;
    let state = |value: &str| fields.contains(&format!("state={value}").as_str());
    let action = match parts.next() {
        Some("merge") => Action::Merged,
        Some("comments") => Action::Commented,
        Some("reviews") => Action::Reviewed,
        Some(_) => Action::Edited,
        None if state("closed") => Action::Closed,
        None if state("open") => Action::Reopened,
        None => Action::Edited,
    };
    Some(Work {
        action,
        named: Some(pull),
        gh_pr: false,
    })
}

/// `git push`, past `git`'s own options; what it printed may name the pull
/// request the branch belongs to.
fn git_push(command: &[&str]) -> Option<Work> {
    let at = command.iter().position(|word| runs(word, "git"))?;
    let mut args = command[at + 1..].iter();
    while let Some(arg) = args.next() {
        match *arg {
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" => {
                args.next();
            }
            arg if arg.starts_with('-') => {}
            "push" => {
                return Some(Work {
                    action: Action::Pushed,
                    named: None,
                    gh_pr: false,
                });
            }
            _ => return None,
        }
    }
    None
}

/// Every pull request address in `text`.
fn addresses(text: &str) -> impl Iterator<Item = PullRequest> + '_ {
    text.match_indices(HOST)
        .filter_map(|(at, _)| PullRequest::scan(&text[at..]).map(|(pull, _)| pull))
}

/// The pull requests `gh pr` names in its messages, such as
/// `✓ Merged pull request OWNER/REPO#7 (Title)`.
fn messages(text: &str) -> impl Iterator<Item = PullRequest> + '_ {
    const PHRASE: &str = "pull request ";
    // ASCII lowering keeps every byte where it was.
    let lowered = text.to_ascii_lowercase();
    let starts: Vec<usize> = lowered
        .match_indices(PHRASE)
        .map(|(at, _)| at + PHRASE.len())
        .collect();
    starts.into_iter().filter_map(move |start| {
        let rest = &text[start..];
        let reference = rest.split(char::is_whitespace).next()?;
        let reference = reference.trim_end_matches(|ch: char| !ch.is_ascii_digit());
        let (repo, number) = reference.split_once('#')?;
        PullRequest::new(repo, number.parse().ok()?, Vec::new())
    })
}

#[cfg(test)]
mod tests;
