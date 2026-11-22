#![allow(clippy::unwrap_used)]

use super::*;

fn call(title: &str, output: &str) -> Tool {
    Tool {
        call_id: "call".to_owned(),
        content: vec![ToolContent::Text {
            text: output.to_owned(),
        }],
        kind: ToolKind::Execute,
        locations: Vec::new(),
        parent: None,
        started_ms: None,
        status: ToolStatus::Completed,
        title: title.to_owned(),
    }
}

/// What `found` read, as `(OWNER/REPO#NUMBER, actions)`.
fn read(title: &str, output: &str) -> Vec<(String, Vec<Action>)> {
    found(&call(title, output), None)
        .into_iter()
        .map(|pull| (format!("{}#{}", pull.repo, pull.number), pull.actions))
        .collect()
}

fn one(reference: &str, action: Action) -> Vec<(String, Vec<Action>)> {
    vec![(reference.to_owned(), vec![action])]
}

#[test]
fn a_created_pull_request_is_read_from_the_address_gh_prints() {
    assert_eq!(
        read(
            "`gh pr create --title \"feat: rail\" --body \"$(cat <<'EOF'\nSummary\nEOF\n)\"`",
            "```console\nhttps://github.com/sheprd-labs/herdr-gpui/pull/1446\n```",
        ),
        one("sheprd-labs/herdr-gpui#1446", Action::Created)
    );
}

#[test]
fn each_changing_gh_pr_command_names_what_it_did() {
    for (command, output, action) in [
        (
            "gh pr edit 7 -R o/r --add-label bug",
            "https://github.com/o/r/pull/7",
            Action::Edited,
        ),
        (
            "gh pr merge 7 --squash",
            "✓ Squashed and merged pull request o/r#7 (Fix)",
            Action::Merged,
        ),
        (
            "gh pr merge --auto --squash 7 --repo o/r",
            "",
            Action::Edited,
        ),
        (
            "gh pr comment 7 --body hi",
            "https://github.com/o/r/pull/7#issuecomment-99",
            Action::Commented,
        ),
        (
            "gh pr review 7 --approve",
            "✓ Approved pull request o/r#7",
            Action::Reviewed,
        ),
        (
            "gh pr ready",
            "✓ Pull request o/r#7 (Fix) is marked as \"ready for review\"",
            Action::Ready,
        ),
        (
            "gh pr close https://github.com/o/r/pull/7",
            "",
            Action::Closed,
        ),
        ("gh pr reopen --repo=o/r #7", "", Action::Reopened),
    ] {
        assert_eq!(read(command, output), one("o/r#7", action), "{command}");
    }
}

#[test]
fn commands_that_only_read_name_no_pull_request() {
    for command in [
        "gh pr view 7 --repo o/r",
        "gh pr list",
        "gh pr checks 7",
        "gh pr diff https://github.com/o/r/pull/7",
        "gh pr checkout 7",
        "gh api repos/o/r/pulls/7",
        "gh api repos/o/r/pulls/7 --method GET -f per_page=5",
        "gh issue comment 7 --body https://github.com/o/r/pull/7",
        "cat notes.md",
        "echo 'gh pr merge 7 --repo o/r'",
        "git log --oneline",
    ] {
        assert_eq!(
            read(command, "see https://github.com/o/r/pull/7"),
            [],
            "{command}"
        );
    }
}

#[test]
fn a_call_that_failed_or_did_not_run_a_command_names_none() {
    let mut failed = call("gh pr merge 7 -R o/r", "");
    failed.status = ToolStatus::Failed;
    assert!(found(&failed, None).is_empty());
    let mut running = call("gh pr merge 7 -R o/r", "");
    running.status = ToolStatus::InProgress;
    assert!(found(&running, None).is_empty());
    let mut edit = call("gh pr merge 7 -R o/r", "");
    edit.kind = ToolKind::Edit;
    assert!(found(&edit, None).is_empty());
}

#[test]
fn gh_api_writes_to_a_pull_request_are_read_by_their_path() {
    for (command, action) in [
        ("gh api -X PUT repos/o/r/pulls/7/merge", Action::Merged),
        (
            "gh api repos/o/r/pulls/7/comments -f body=hi",
            Action::Commented,
        ),
        (
            "gh api --method=POST /repos/o/r/pulls/7/reviews",
            Action::Reviewed,
        ),
        (
            "gh api -X PATCH repos/o/r/pulls/7 -f state=closed",
            Action::Closed,
        ),
        (
            "gh api -X PATCH repos/o/r/pulls/7 -f state=open",
            Action::Reopened,
        ),
        (
            "gh api -X PATCH repos/o/r/pulls/7 -f title=New",
            Action::Edited,
        ),
        (
            "gh api repos/o/r/pulls/7/requested_reviewers -f reviewers[]=a",
            Action::Edited,
        ),
    ] {
        assert_eq!(read(command, ""), one("o/r#7", action), "{command}");
    }
    // A placeholder `gh` fills in is not a repository this can name.
    assert_eq!(
        read("gh api -X PUT repos/{owner}/{repo}/pulls/7/merge", ""),
        []
    );
}

#[test]
fn a_push_names_the_pull_request_its_output_links_to() {
    let output = "remote:\nremote: View pull request for 'feat':\nremote:   \
                  https://github.com/o/r/pull/7\nTo github.com:o/r.git";
    assert_eq!(
        read("git -C /tmp/r push origin feat", output),
        one("o/r#7", Action::Pushed)
    );
    // A new branch's link opens a pull request; it names none yet.
    let new = "remote: Create a pull request for 'feat' on GitHub by visiting:\n\
               remote:      https://github.com/o/r/pull/new/feat";
    assert_eq!(read("git push -u origin feat", new), []);
}

#[test]
fn chained_commands_are_read_one_by_one_and_each_pull_request_once() {
    let pulls = read(
        "cd repo && git push && gh pr create --fill",
        "https://github.com/o/r/pull/7\nhttps://github.com/o/r/pull/7",
    );
    assert_eq!(
        pulls,
        vec![("o/r#7".to_owned(), vec![Action::Pushed, Action::Created])]
    );
    // GitHub reads names in any case.
    assert_eq!(
        read("gh pr close 7 -R O/R", "✓ Closed pull request o/r#7 (Fix)").len(),
        1
    );
}

#[test]
fn addresses_that_are_not_pull_requests_are_ignored() {
    let output = "https://github.com/o/r/issues/7 https://gitlab.com/o/r/pull/7 \
                  https://github.com/o/r/pull/7abc http://github.com/o/r/pull/8";
    assert_eq!(read("gh pr create --fill", output), []);
}

#[test]
fn a_call_names_a_bounded_number_of_pull_requests() {
    let output: String = (1..=MAX_FOUND as u64 + 5)
        .map(|number| format!("https://github.com/o/r/pull/{number}\n"))
        .collect();
    assert_eq!(read("gh pr edit --fill", &output).len(), MAX_FOUND);
}

#[test]
fn quotes_and_separators_split_words_as_a_shell_would() {
    let words = words("a 'b c' \"d \\\" e\" f\\ g;h&&i|j".to_owned());
    let text = |word: &str| Word::Text(word.to_owned());
    assert_eq!(
        words,
        [
            text("a"),
            text("b c"),
            text("d \" e"),
            text("f g"),
            Word::Break,
            text("h"),
            Word::Break,
            text("i"),
            Word::Break,
            text("j"),
        ]
    );
    assert_eq!(command_text("`echo \\`date\\``"), "echo `date`");
    assert!(runs("/opt/homebrew/bin/gh", "gh"));
    assert!(runs(r"C:\bin\gh.exe", "gh"));
    assert!(!runs("ghq", "gh"));
}

#[test]
fn a_command_of_several_lines_is_read_from_its_title_or_whole() {
    // The title native Claude and Codex give a command of several lines.
    let title = crate::harness::out::command_title(
        "gh pr create --title \"feat: rail\" --body \"$(cat <<'EOF'\nSummary\nEOF\n)\"",
    );
    assert!(title.ends_with("more lines)"), "{title}");
    let output = "https://github.com/o/r/pull/7";
    assert_eq!(read(&title, output), one("o/r#7", Action::Created));
    // A command whose `gh` is past the first line is read from the whole
    // command the session kept.
    let whole = "cd repo\ngit status\ngh pr merge 7 --repo o/r";
    let tool = call(&crate::harness::out::command_title(whole), "");
    assert!(found(&tool, None).is_empty(), "the title alone misses it");
    let pulls = found(&tool, Some(whole));
    assert_eq!(pulls.len(), 1);
    assert_eq!(pulls[0].actions, [Action::Merged]);
}

#[test]
fn each_pull_request_gets_only_what_its_own_command_did() {
    let pulls = read(
        "gh pr merge 1 -R o/r && gh pr close 2 -R o/r",
        "✓ Merged pull request o/r#1 (One)\n✓ Closed pull request o/r#2 (Two)",
    );
    assert_eq!(
        pulls,
        vec![
            ("o/r#1".to_owned(), vec![Action::Merged]),
            ("o/r#2".to_owned(), vec![Action::Closed]),
        ]
    );
    // What a command that named its pull request printed adds no other.
    assert_eq!(
        read(
            "gh pr comment 7 -R o/r --body hi",
            "see https://github.com/o/r/pull/8"
        ),
        one("o/r#7", Action::Commented)
    );
}

#[test]
fn option_values_are_never_taken_for_the_pull_request() {
    assert_eq!(
        read(
            "gh pr comment --body https://github.com/o/r/pull/99 7 -R o/r",
            "https://github.com/o/r/pull/7#issuecomment-1",
        ),
        one("o/r#7", Action::Commented)
    );
    assert_eq!(
        read(
            "gh pr create --title 5 --body https://github.com/o/r/pull/99 -R o/r",
            "https://github.com/o/r/pull/12",
        ),
        one("o/r#12", Action::Created)
    );
    // `-c` is a switch for `review` and `-r` one for `merge`.
    assert_eq!(
        read("gh pr review -c -b looks-good 7 -R o/r", ""),
        one("o/r#7", Action::Reviewed)
    );
    assert_eq!(
        read("gh pr merge -r 7 -R o/r", ""),
        one("o/r#7", Action::Merged)
    );
}

#[test]
fn a_command_aimed_at_another_host_names_nothing() {
    for command in [
        "gh api --hostname github.example.com -X PUT repos/o/r/pulls/7/merge",
        "gh api --hostname=github.example.com -X PUT repos/o/r/pulls/7/merge",
        "GH_HOST=github.example.com gh pr merge 7 -R o/r",
    ] {
        assert_eq!(read(command, ""), [], "{command}");
    }
    assert_eq!(
        read(
            "gh api --hostname github.com -X PUT repos/o/r/pulls/7/merge",
            ""
        ),
        one("o/r#7", Action::Merged)
    );
}

#[test]
fn here_document_bodies_are_text_not_commands() {
    let pulls = |command: &str| found(&call("`cat`", ""), Some(command));
    for command in [
        "cat > notes.txt <<'EOF'\ngh pr merge 7 -R o/r\nEOF",
        "cat <<-NOTE > notes.txt\n\tgh pr close 7 -R o/r\n\tNOTE",
        "cat <<\"EOF\" | tee notes.txt\ngh pr merge 7 -R o/r\nEOF",
        // Never closed: the rest is the document's.
        "cat <<EOF\ngh pr merge 7 -R o/r",
    ] {
        assert!(pulls(command).is_empty(), "{command:?}");
    }
    // A command after the document still counts, and a here-string is no
    // document.
    let after = pulls("cat <<EOF > notes.txt\ngh pr merge 1 -R o/r\nEOF\ngh pr merge 7 -R o/r");
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].number, 7);
    assert_eq!(pulls("grep x <<< \"$text\"\ngh pr merge 7 -R o/r").len(), 1);
    // `<<` in quotes or a comment starts no document.
    for command in [
        "echo 'a << b'\ngh pr merge 7 -R o/r",
        "echo \"a << b\"\ngh pr merge 7 -R o/r",
        "true # see <<EOF\ngh pr merge 7 -R o/r",
    ] {
        assert_eq!(pulls(command).len(), 1, "{command:?}");
    }
    // The body of `gh pr create`'s own document is skipped; its URL counts.
    let create = found(
        &call("`gh pr create`", "https://github.com/o/r/pull/9"),
        Some("gh pr create --body \"$(cat <<'EOF'\nFixes https://github.com/o/r/pull/3\nEOF\n)\""),
    );
    assert_eq!(create.len(), 1);
    assert_eq!(create[0].number, 9);
}
