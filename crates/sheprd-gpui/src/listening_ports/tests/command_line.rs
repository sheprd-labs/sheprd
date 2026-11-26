//! Windows command lines split by Windows' rules, as `CommandLineToArgvW`
//! documents them.
use crate::listening_ports::command_line::windows_args;

#[test]
fn a_quoted_program_keeps_its_spaces() {
    assert_eq!(
        windows_args(r#""C:\Program Files\nodejs\node.exe" cli.js --port 3000"#),
        [
            r"C:\Program Files\nodejs\node.exe",
            "cli.js",
            "--port",
            "3000"
        ]
    );
    assert_eq!(
        windows_args(r"C:\tools\node.exe  cli.js"),
        [r"C:\tools\node.exe", "cli.js"]
    );
}

/// The program takes no escapes: a backslash before its closing quote is
/// part of the path.
#[test]
fn the_program_ends_at_its_closing_quote() {
    assert_eq!(windows_args(r#""C:\dir\" x"#), [r"C:\dir\", "x"]);
}

#[test]
fn quotes_group_words_in_later_arguments() {
    assert_eq!(
        windows_args(r#"node "C:\Users\Ada Lovelace\cli.js" a" b"c"#),
        ["node", r"C:\Users\Ada Lovelace\cli.js", "a bc"]
    );
    assert_eq!(windows_args(r#"x "say ""hi""""#), ["x", r#"say "hi""#]);
    assert_eq!(windows_args(r#"x """#), ["x", ""]);
}

/// Backslashes are literal except before a quote: each pair is one, and an
/// odd one left over escapes the quote.
#[test]
fn backslashes_escape_only_quotes() {
    assert_eq!(windows_args(r"x a\\b c\"), ["x", r"a\\b", r"c\"]);
    assert_eq!(windows_args(r#"x a\"b"#), ["x", r#"a"b"#]);
    assert_eq!(windows_args(r#"x a\\"b c""#), ["x", r"a\b c"]);
    assert_eq!(windows_args(r#"x a\\\"b"#), ["x", r#"a\"b"#]);
}

#[test]
fn blank_lines_name_an_empty_program() {
    assert_eq!(windows_args(""), [""]);
    assert_eq!(windows_args(" \t "), [""]);
    assert_eq!(windows_args("node\t\t"), ["node"]);
}
