use std::ffi::{OsStr, OsString};

use super::*;
use crate::cli::{Parsed, parse};

const TOKEN: &str = "abcdefghijklmnopqrstuvwxyz0123_-";

/// `path`, absolute on this platform: Windows wants a drive.
fn abs(path: &str) -> String {
    if cfg!(windows) {
        format!("C:{path}")
    } else {
        path.to_owned()
    }
}

fn program() -> String {
    abs("/opt/sheprd/sheprd-acp")
}

/// A resumer for the chat `args` start, writing to a buffer.
fn build(
    args: &[&str],
    adapter: bool,
    cwd: &str,
    thread: Option<ThreadId>,
) -> Option<Resumer<Vec<u8>>> {
    build_with(TOKEN, &program(), args, adapter, cwd, thread)
}

fn build_with(
    token: &str,
    program: &str,
    args: &[&str],
    adapter: bool,
    cwd: &str,
    thread: Option<ThreadId>,
) -> Option<Resumer<Vec<u8>>> {
    let launch = Launch::new(
        OsStr::new(program),
        &options(args),
        adapter,
        Path::new(cwd),
        thread,
    )?;
    Resumer::new(OsStr::new(token), launch, 4242, Vec::new())
}

fn options(args: &[&str]) -> Options {
    match parse(args.iter().map(OsString::from)).unwrap() {
        Parsed::Run(options) => options,
        other => panic!("not a chat: {other:?}"),
    }
}

fn resumer(args: &[&str]) -> Option<Resumer<Vec<u8>>> {
    let adapter = args.contains(&"--acp");
    build(args, adapter, &abs("/home/user/project"), None)
}

/// Each report written, as its body's `key=value` pairs, with the mac
/// checked against the token as a terminal would.
fn reports(out: &[u8]) -> Vec<Vec<(String, String)>> {
    let out = std::str::from_utf8(out).unwrap();
    out.split_terminator("\x1b\\")
        .map(|report| {
            let body = report.strip_prefix("\x1b]7577;").unwrap();
            let (signed, mac) = body.rsplit_once(":mac=").unwrap();
            assert_eq!(mac, hex(&Sha256::digest(format!("{signed}:{TOKEN}"))));
            signed
                .split(':')
                .map(|pair| {
                    let (key, value) = pair.split_once('=').unwrap();
                    (key.to_owned(), value.to_owned())
                })
                .collect()
        })
        .collect()
}

fn value<'a>(report: &'a [(String, String)], key: &str) -> Option<&'a str> {
    report
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

fn argv(report: &[(String, String)]) -> Vec<String> {
    value(report, "argv")
        .unwrap()
        .split(',')
        .map(|arg| String::from_utf8(STANDARD.decode(arg).unwrap()).unwrap())
        .collect()
}

#[test]
fn signs_as_the_protocols_example() {
    // Revision 0.5's Neovim example, whose token is `dummy-token-replace-me`.
    let argv = [
        "nvim",
        "-S",
        "/home/user/.local/state/nvim/sessions/8f2c1d.vim",
    ]
    .map(str::to_owned);
    let pairs = [
        ("pid", "74877".to_owned()),
        ("time", "1795543200".to_owned()),
        ("argv", encode_argv(&argv).unwrap()),
    ];
    assert_eq!(
        signed(&pairs, "dummy-token-replace-me").unwrap(),
        "\x1b]7577;pid=74877:time=1795543200:argv=bnZpbQ==,LVM=,\
         L2hvbWUvdXNlci8ubG9jYWwvc3RhdGUvbnZpbS9zZXNzaW9ucy84ZjJjMWQudmlt\
         :mac=9dbb99bfb856749a87563839d7ce9062818d78cf25909db93f309e0cd3b1909b\x1b\\"
    );
}

#[test]
fn reports_the_command_that_continues_the_session() {
    let mut resumer = resumer(&["--acp", "claude"]).unwrap();
    resumer.opened("s-1");
    let reports = reports(&resumer.out);
    let [report] = reports.as_slice() else {
        panic!("{reports:?}");
    };
    assert_eq!(value(report, "pid"), Some("4242"));
    assert_eq!(
        argv(report),
        [program().as_str(), "--resume", "s-1", "--acp", "claude"]
    );
    assert_eq!(
        value(report, "cwd"),
        Some(STANDARD.encode(abs("/home/user/project")).as_str())
    );
    assert_eq!(report.last().unwrap().0, "cwd");
}

#[test]
fn keeps_thread_and_view_choices() {
    let thread = ThreadId::random();
    let mut resumer = build(
        &["--headless", "--no-attach", "codex"],
        false,
        &abs("/w"),
        Some(thread),
    )
    .unwrap();
    resumer.opened("t-9");
    let reports = reports(&resumer.out);
    assert_eq!(
        argv(&reports[0]),
        [
            program().as_str(),
            "--resume",
            "t-9",
            "--thread",
            &thread.to_string(),
            "--headless",
            "--no-attach",
            "codex",
        ]
    );
}

#[test]
fn times_increase_and_a_switch_clears_before_the_next_session() {
    let mut resumer = resumer(&["--choose", "claude"]).unwrap();
    resumer.opened("s-1");
    resumer.switched(Agent::Codex);
    resumer.opened("s-2");
    resumer.ended();
    let reports = reports(&resumer.out);
    assert_eq!(reports.len(), 4);
    assert_eq!(value(&reports[1], "clear"), Some("1"));
    assert_eq!(argv(&reports[2]).last().map(String::as_str), Some("codex"));
    assert_eq!(value(&reports[3], "clear"), Some("1"));
    let times: Vec<u64> = reports
        .iter()
        .map(|report| value(report, "time").unwrap().parse().unwrap())
        .collect();
    assert!(times.windows(2).all(|pair| pair[0] < pair[1]), "{times:?}");
}

#[test]
fn clears_only_what_it_reported() {
    let mut resumer = resumer(&["claude"]).unwrap();
    resumer.ended();
    resumer.switched(Agent::Codex);
    assert!(resumer.out.is_empty());
    resumer.opened("s-1");
    resumer.ended();
    resumer.ended();
    assert_eq!(reports(&resumer.out).len(), 2);
}

#[test]
fn a_session_it_cannot_type_back_clears_the_earlier_one() {
    let mut resumer = resumer(&["claude"]).unwrap();
    resumer.opened("s-1");
    resumer.opened("-rm");
    resumer.opened("a b");
    let reports = reports(&resumer.out);
    assert_eq!(reports.len(), 2);
    assert_eq!(value(&reports[1], "clear"), Some("1"));
}

#[test]
fn a_report_too_long_for_every_terminal_is_not_sent() {
    let cwd = abs(&format!("/{}", "d".repeat(1600)));
    let mut resumer = build(&["claude"], false, &cwd, None).unwrap();
    resumer.opened("s-1");
    assert!(resumer.out.is_empty());
}

#[test]
fn keeps_nothing_it_could_not_resume_safely() {
    let new = |token: &str, program: &str, args: &[&str], cwd: &str| {
        build_with(token, program, args, false, cwd, None).is_some()
    };
    let (program, cwd) = (program(), abs("/w"));
    assert!(new(TOKEN, &program, &["claude"], &cwd));
    assert!(!new("short", &program, &["claude"], &cwd));
    assert!(!new(&"t".repeat(65), &program, &["claude"], &cwd));
    assert!(!new(
        "abcdefghijklmnopqrstuv:x",
        &program,
        &["claude"],
        &cwd
    ));
    assert!(!new(TOKEN, "sheprd-acp", &["claude"], &cwd));
    assert!(!new(TOKEN, &program, &["claude"], "relative"));
    assert!(!new(TOKEN, &program, &["claude"], &abs("/w\u{1b}]0;x")));
    assert!(!new(
        TOKEN,
        &program,
        &["claude", "--", "--model", "x"],
        &cwd
    ));
    assert!(!new(TOKEN, &program, &["--", "my-agent"], &cwd));
}

#[test]
fn keeps_the_adapter_the_environment_chose() {
    // `HERDR_ACP_ADAPTER=1` chose the adapter, not `--acp`: a restarted
    // pane may not set it, so the command says so itself.
    let mut resumer = build(&["claude"], true, &abs("/w"), None).unwrap();
    resumer.opened("s-1");
    let reports = reports(&resumer.out);
    assert!(argv(&reports[0]).iter().any(|arg| arg == "--acp"));
}
