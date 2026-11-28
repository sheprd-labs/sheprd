//! `phone luau` and `phone snapshot`: an agent changing a Luau screen of
//! the phone's app while it runs, and seeing what the phone shows.
use super::{Asked, NO_PHONE, drive, output};
use crate::control::{
    ErrorCode,
    client::{Context, Output, finish, unexpected},
    protocol::{Request, Response, phone::Answer},
};
use sheprd_remote_control::control::phone::{self, Command, LuauReport, Outcome, Phone};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

/// How often `--watch` looks at the folder again.
const POLL: Duration = Duration::from_millis(300);
/// How often `--watch` pushes again a version the phone did not take.
const RETRY: Duration = Duration::from_secs(3);

/// What changes when a file is saved, added or removed.
type Stamp = Vec<(String, Option<SystemTime>, usize)>;

pub(super) fn luau(
    device: Option<String>,
    dir: &Path,
    screen: Option<String>,
    watch: bool,
    reset: bool,
    context: &Context,
) -> crate::Result<Output> {
    let screen = match screen {
        Some(screen) => screen,
        None => screen_of(dir)?,
    };
    if reset {
        return push(device, &screen, BTreeMap::new(), context).map(|(output, _)| output);
    }
    if !watch {
        return push(device, &screen, read(dir)?, context).map(|(output, _)| output);
    }
    watch_dir(device, dir, &screen, context)
}

/// Until stopped: pushes each time a file changes and says what the phone
/// found, as `--json` asks, so an agent watching sees each save land. A
/// version the phone did not take, as when it is locked or away, is pushed
/// again every [`RETRY`] until it does; one it took with Luau errors waits
/// for the next save. A file caught mid-save is read again next time.
fn watch_dir(
    device: Option<String>,
    dir: &Path,
    screen: &str,
    context: &Context,
) -> crate::Result<Output> {
    let mut watch = Watch::default();
    loop {
        match read(dir) {
            Ok(files) => {
                let stamp = stamp(dir, &files);
                if watch.due(&stamp, Instant::now()) {
                    let pushed = push(device.clone(), screen, files, context);
                    let taken = matches!(pushed, Ok((_, true)));
                    watch.pushed(stamp, taken, Instant::now());
                    let pushed = pushed.map(|(output, _)| output);
                    if taken {
                        watch.said = None;
                        finish(pushed, context.json);
                    } else {
                        watch.once(pushed, context.json);
                    }
                }
            }
            Err(error) => watch.once(Err(error), context.json),
        }
        std::thread::sleep(POLL);
    }
}

/// What `--watch` pushed last, so it pushes each version until the phone
/// takes it, and says each failure once.
#[derive(Default)]
struct Watch {
    /// The version the phone took.
    taken: Option<Stamp>,
    /// The version last pushed and not taken, and when.
    tried: Option<(Stamp, Instant)>,
    /// What was last said of a push the phone did not take or a read that
    /// failed, so a retry that fails the same way says nothing.
    said: Option<String>,
}

impl Watch {
    /// Whether `stamp` is to be pushed now.
    fn due(&self, stamp: &Stamp, now: Instant) -> bool {
        if self.taken.as_ref() == Some(stamp) {
            return false;
        }
        !matches!(&self.tried, Some((tried, at)) if tried == stamp && now.duration_since(*at) < RETRY)
    }

    fn pushed(&mut self, stamp: Stamp, taken: bool, now: Instant) {
        if taken {
            self.taken = Some(stamp);
            self.tried = None;
        } else {
            self.tried = Some((stamp, now));
        }
    }

    /// Prints `result` unless it says what was last said.
    fn once(&mut self, result: crate::Result<Output>, json: bool) {
        let said = match &result {
            Ok(output) => output.text().to_owned(),
            Err(error) => error.to_string(),
        };
        if self.said.as_ref() != Some(&said) {
            finish(result, json);
            self.said = Some(said);
        }
    }
}

/// Pushes `files`; whether the phone took them, Luau errors and all.
fn push(
    device: Option<String>,
    screen: &str,
    files: BTreeMap<String, String>,
    context: &Context,
) -> crate::Result<(Output, bool)> {
    if !phone::valid_files(&files) {
        return Err(rejected(format!(
            "At most {} .luau files and {} KiB as JSON, named as a screen's folder names them",
            phone::MAX_LUAU_FILES,
            phone::MAX_LUAU_BYTES / 1024
        )));
    }
    let ask = drive(
        device,
        Command::Luau {
            screen: screen.to_owned(),
            files,
        },
    );
    match context.call(Request::Phone, ask)? {
        Response::Phone { answer } => match &answer {
            Answer::Driven {
                phone,
                outcome: Outcome::Luau { report },
            } => {
                let text = reported(phone, screen, report);
                let output = Output::data(text, &answer)?;
                let output = if report.problems.is_empty() {
                    output
                } else {
                    output.failed()
                };
                Ok((output, true))
            }
            _ => Ok((output(&answer, Asked::Show)?, false)),
        },
        response => Err(unexpected(response)),
    }
}

/// What the phone found, as a person reads it.
fn reported(phone: &Phone, screen: &str, report: &LuauReport) -> String {
    let name = &phone.name;
    let mut lines = vec![if report.problems.is_empty() && report.shown {
        format!("{screen} reloaded on {name}")
    } else if report.problems.is_empty() {
        format!("{screen} loads on {name}; it shows when the screen opens")
    } else {
        format!("{screen} did not load on {name}; it keeps the last version that did")
    }];
    lines.extend(
        report
            .problems
            .iter()
            .map(|line| format!("problem: {line}")),
    );
    lines.extend(
        report
            .warnings
            .iter()
            .map(|line| format!("warning: {line}")),
    );
    lines.extend(report.logs.iter().map(|line| format!("log: {line}")));
    lines.join("\n")
}

pub(super) fn snapshot(
    device: Option<String>,
    out: Option<PathBuf>,
    context: &Context,
) -> crate::Result<Output> {
    match context.call(Request::Phone, drive(device, Command::Snapshot))? {
        Response::Phone { answer } => match &answer {
            Answer::Snapshot { phone, path } => {
                let Some(out) = out else {
                    return Output::data(path.clone(), &answer);
                };
                std::fs::copy(path, &out).map_err(|error| {
                    rejected(format!("Could not write {}: {error}", out.display()))
                })?;
                Output::data(format!("{} from {}", out.display(), phone.name), &answer)
            }
            Answer::NoPhone => Ok(Output::data(NO_PHONE, &answer)?.nothing()),
            _ => output(&answer, Asked::Show),
        },
        response => Err(unexpected(response)),
    }
}

/// The screen a folder is: its name, as the app's bundle names it.
fn screen_of(dir: &Path) -> crate::Result<String> {
    let name = std::fs::canonicalize(dir)
        .ok()
        .and_then(|dir| {
            dir.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    if phone::valid_screen(&name) {
        Ok(name)
    } else {
        Err(rejected(format!(
            "Name the screen with --screen: the folder's name, {name:?}, is not one"
        )))
    }
}

/// Every `.luau` file under `dir`, at most two folders down, by its path
/// from `dir`.
fn read(dir: &Path) -> crate::Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    walk(dir, dir, 0, &mut files)?;
    if files.is_empty() {
        return Err(rejected(format!("No .luau files in {}", dir.display())));
    }
    Ok(files)
}

fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    files: &mut BTreeMap<String, String>,
) -> crate::Result<()> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| rejected(format!("Could not read {}: {error}", dir.display())))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if hidden {
            continue;
        }
        if path.is_dir() && depth < 2 {
            walk(root, &path, depth + 1, files)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "luau")
        {
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let name = relative.to_string_lossy().replace('\\', "/");
            let text = std::fs::read_to_string(&path)
                .map_err(|error| rejected(format!("Could not read {}: {error}", path.display())))?;
            files.insert(name, text);
        }
    }
    Ok(())
}

fn stamp(dir: &Path, files: &BTreeMap<String, String>) -> Stamp {
    files
        .iter()
        .map(|(name, text)| {
            let modified = std::fs::metadata(dir.join(name))
                .and_then(|meta| meta.modified())
                .ok();
            (name.clone(), modified, text.len())
        })
        .collect()
}

fn rejected(message: String) -> crate::Error {
    crate::control::Error::Rejected {
        code: ErrorCode::InvalidRequest,
        message,
    }
    .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_read_as_a_screen_names_its_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("pull-requests");
        std::fs::create_dir_all(root.join("components")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("main.luau"), "return 1").unwrap();
        std::fs::write(root.join("components/row.luau"), "return 2").unwrap();
        std::fs::write(root.join("notes.md"), "not Luau").unwrap();
        std::fs::write(root.join(".git/x.luau"), "hidden").unwrap();

        let files = read(&root).unwrap();
        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            ["components/row.luau", "main.luau"]
        );
        assert!(phone::valid_files(&files));
        assert_eq!(screen_of(&root).unwrap(), "pull-requests");
    }

    #[test]
    fn a_report_says_whether_it_loaded() {
        let phone = Phone {
            device: sheprd_remote_control::control::DeviceId("d".into()),
            name: "iPhone".into(),
            active_ms: 0,
        };
        let report = LuauReport {
            shown: true,
            problems: vec!["main.luau:3: oops".into()],
            ..LuauReport::default()
        };
        let text = reported(&phone, "pull-requests", &report);
        assert!(text.starts_with("pull-requests did not load on iPhone"));
        assert!(text.contains("problem: main.luau:3: oops"));
    }

    #[test]
    fn a_watch_pushes_again_what_the_phone_did_not_take() {
        let now = Instant::now();
        let saved = vec![("main.luau".to_owned(), None, 1)];
        let mut watch = Watch::default();
        assert!(watch.due(&saved, now));

        // The phone was locked: not again at once, but after a while.
        watch.pushed(saved.clone(), false, now);
        assert!(!watch.due(&saved, now + POLL));
        assert!(watch.due(&saved, now + RETRY));

        // A new save goes at once; once taken, it is not pushed again.
        let edited = vec![("main.luau".to_owned(), None, 2)];
        assert!(watch.due(&edited, now + POLL));
        watch.pushed(edited.clone(), true, now + POLL);
        assert!(!watch.due(&edited, now + RETRY * 2));
    }
}
