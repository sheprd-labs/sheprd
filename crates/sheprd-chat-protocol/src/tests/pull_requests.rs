use super::*;

fn pull(repo: &str, number: u64, actions: &[PullRequestAction]) -> PullRequest {
    PullRequest::new(repo, number, actions.to_vec()).unwrap()
}

fn add(transcript: &mut Transcript, pull_request: PullRequest) -> Result<()> {
    transcript.apply(&Change::PullRequest { pull_request })
}

#[test]
fn a_pull_request_address_is_read_with_or_without_a_page_after_it() {
    for url in [
        "https://github.com/sheprd-labs/herdr-gpui/pull/1445",
        "https://github.com/sheprd-labs/herdr-gpui/pull/1445/files",
        "https://github.com/sheprd-labs/herdr-gpui/pull/1445#issuecomment-12",
        "https://github.com/sheprd-labs/herdr-gpui/pull/1445?w=1",
    ] {
        let pull = PullRequest::parse(url).unwrap();
        assert_eq!(pull.repo, "sheprd-labs/herdr-gpui", "{url}");
        assert_eq!(pull.number, 1445);
        assert_eq!(
            pull.url,
            "https://github.com/sheprd-labs/herdr-gpui/pull/1445"
        );
        assert!(pull.actions.is_empty());
    }
}

#[test]
fn addresses_that_are_not_a_github_pull_request_are_refused() {
    for url in [
        "",
        "http://github.com/o/r/pull/1",
        "https://gitlab.com/o/r/pull/1",
        "https://github.com.evil.example/o/r/pull/1",
        "https://github.com/o/r/issues/1",
        "https://github.com/o/r/pull/new/branch",
        "https://github.com/o/r/pull/",
        "https://github.com/o/r/pull/0",
        "https://github.com/o/r/pull/12abc",
        "https://github.com/o/r/pull/99999999999999999999999",
        "https://github.com/-o/r/pull/1",
        "https://github.com/o/../pull/1",
        "https://github.com/o/r r/pull/1",
        "https://github.com/o/r/pull/1 and more",
        "https://github.com/o/pull/1",
    ] {
        assert_eq!(PullRequest::parse(url), None, "{url:?}");
    }
    let long = format!("https://github.com/{}/r/pull/1", "o".repeat(40));
    assert_eq!(PullRequest::parse(&long), None);
}

#[test]
fn an_address_is_found_at_the_start_of_text_with_what_follows() {
    let (pull, rest) = PullRequest::scan("https://github.com/o/r/pull/7). Done").unwrap();
    assert_eq!(pull.number, 7);
    assert_eq!(rest, "). Done");
    assert!(PullRequest::scan("see https://github.com/o/r/pull/7").is_none());
}

#[test]
fn a_pull_request_is_kept_once_with_each_thing_done_to_it() {
    let mut transcript = Transcript::new("Claude");
    add(
        &mut transcript,
        pull("o/r", 1, &[PullRequestAction::Created]),
    )
    .unwrap();
    add(&mut transcript, pull("o/other", 2, &[])).unwrap();
    add(
        &mut transcript,
        pull(
            "O/R",
            1,
            &[PullRequestAction::Created, PullRequestAction::Merged],
        ),
    )
    .unwrap();
    assert_eq!(transcript.pull_requests.len(), 2);
    let first = &transcript.pull_requests[0];
    assert_eq!(first.repo, "o/r", "the first spelling stays");
    assert_eq!(
        first.actions,
        [PullRequestAction::Created, PullRequestAction::Merged]
    );
    assert!(transcript.pull_request(&pull("o/R", 1, &[])).is_some());
    assert!(transcript.pull_request(&pull("o/r", 3, &[])).is_none());
}

#[test]
fn a_pull_request_that_does_not_fit_is_refused() {
    let mut transcript = Transcript::new("Claude");
    let mut wrong_url = pull("o/r", 1, &[]);
    wrong_url.url = "https://github.com/o/r/pull/2".to_owned();
    let mut wrong_repo = pull("o/r", 1, &[]);
    wrong_repo.repo = "o".to_owned();
    let twice = pull(
        "o/r",
        1,
        &[PullRequestAction::Merged, PullRequestAction::Merged],
    );
    for refused in [wrong_url, wrong_repo, twice] {
        assert!(matches!(
            add(&mut transcript, refused),
            Err(Error::PullRequest)
        ));
    }
    assert!(transcript.pull_requests.is_empty());
}

#[test]
fn a_full_list_refuses_another_but_still_records_actions() {
    let mut transcript = Transcript::new("Claude");
    for number in 1..=MAX_PULL_REQUESTS as u64 {
        add(&mut transcript, pull("o/r", number, &[])).unwrap();
    }
    let error = add(&mut transcript, pull("o/r", 1000, &[])).unwrap_err();
    assert!(matches!(error, Error::PullRequestLimit));
    add(
        &mut transcript,
        pull("o/r", 1, &[PullRequestAction::Closed]),
    )
    .unwrap();
    assert_eq!(transcript.pull_requests.len(), MAX_PULL_REQUESTS);
    assert_eq!(
        transcript.pull_requests[0].actions,
        [PullRequestAction::Closed]
    );
}

#[test]
fn pull_requests_read_from_older_and_newer_programs() {
    let old: Transcript = serde_json::from_str(r#"{"agent": "Claude"}"#).unwrap();
    assert!(old.pull_requests.is_empty());
    let empty = serde_json::to_value(Transcript::new("Claude")).unwrap();
    assert!(empty.get("pull_requests").is_none(), "left out when empty");
    let change: Change = serde_json::from_str(
        r#"{"change": "pull_request", "pull_request": {
            "number": 3, "repo": "o/r", "url": "https://github.com/o/r/pull/3",
            "actions": ["created", "auto_merged"]}}"#,
    )
    .unwrap();
    let Change::PullRequest { pull_request } = change else {
        panic!("expected a pull request change");
    };
    assert_eq!(
        pull_request.actions,
        [PullRequestAction::Created, PullRequestAction::Other]
    );
    let command: Command = serde_json::from_str(
        r#"{"command": "add_pull_request", "url": "https://github.com/o/r/pull/3"}"#,
    )
    .unwrap();
    assert_eq!(
        command,
        Command::AddPullRequest {
            url: "https://github.com/o/r/pull/3".to_owned()
        }
    );
}
