//! A Windows command line split into arguments as `CommandLineToArgvW` and
//! the C runtime split it, so `"C:\Program Files\nodejs\node.exe" cli.js`
//! names `node.exe` as its program rather than `"C:\Program`. Pure: the
//! line is a remote host's, read as text and never run.

use std::iter::repeat_n;

/// `line`'s arguments, the program first. The program ends at its closing
/// quote, or at whitespace when unquoted, with no escapes; in the others,
/// quotes group words, `""` inside quotes is a quote, and backslashes are
/// literal unless they precede a quote, where each pair is one backslash
/// and an odd one left over escapes the quote.
pub(crate) fn windows_args(line: &str) -> Vec<String> {
    let mut chars = line.trim_start_matches([' ', '\t']).chars().peekable();
    let mut args = Vec::new();
    let mut program = String::new();
    if chars.next_if_eq(&'"').is_some() {
        program.extend(chars.by_ref().take_while(|c| *c != '"'));
    } else {
        while let Some(c) = chars.next_if(|c| !matches!(c, ' ' | '\t')) {
            program.push(c);
        }
    }
    args.push(program);
    loop {
        while chars.next_if(|c| matches!(c, ' ' | '\t')).is_some() {}
        if chars.peek().is_none() {
            return args;
        }
        let mut arg = String::new();
        let mut quoted = false;
        while let Some(&c) = chars.peek() {
            match c {
                ' ' | '\t' if !quoted => break,
                '\\' => {
                    let mut slashes = 0;
                    while chars.next_if_eq(&'\\').is_some() {
                        slashes += 1;
                    }
                    if chars.peek() == Some(&'"') {
                        arg.extend(repeat_n('\\', slashes / 2));
                        if slashes % 2 == 1 {
                            arg.push('"');
                            chars.next();
                        }
                    } else {
                        arg.extend(repeat_n('\\', slashes));
                    }
                }
                '"' => {
                    chars.next();
                    if quoted && chars.next_if_eq(&'"').is_some() {
                        arg.push('"');
                    } else {
                        quoted = !quoted;
                    }
                }
                _ => {
                    arg.push(c);
                    chars.next();
                }
            }
        }
        args.push(arg);
    }
}
