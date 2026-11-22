use super::*;

#[test]
fn a_pull_request_is_found_by_its_address_whichever_checkout_asked() {
    let mut cache = Cache::default();
    let input = Input {
        checkout: None,
        repo_key: Some("/repo/.git".into()),
        branch: "feature".into(),
    };
    assert!(
        cache
            .by_url("https://github.com/example/project/pull/8")
            .is_none()
    );
    cache.seed(input, fixture().unwrap(), Instant::now());
    let found = cache.by_url("https://github.com/Example/Project/pull/8");
    assert_eq!(found.map(|pr| pr.number), Some(8));
    assert!(
        cache
            .by_url("https://github.com/example/project/pull/9")
            .is_none()
    );
}
