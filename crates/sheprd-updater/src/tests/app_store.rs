use super::*;

#[test]
fn the_app_store_reason_follows_a_colon() {
    assert_eq!(
        Disabled::AppStore.to_string(),
        "this copy came from the Mac App Store, which installs its updates."
    );
}

#[cfg(feature = "mas")]
#[test]
fn an_app_store_copy_never_checks_or_installs() {
    let updater = Updater::start(UpdateChannel::default());
    assert_eq!(updater.state, State::Disabled(Disabled::AppStore));
    assert!(updater.commands.is_none(), "no update worker runs");
    let args = [
        OsString::from("sheprd"),
        OsString::from("--herdr-apply-update"),
    ];
    assert!(run_helper(&args).is_none(), "never the install helper");
}
