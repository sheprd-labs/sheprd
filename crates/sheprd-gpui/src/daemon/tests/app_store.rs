use super::*;

#[test]
fn only_a_developer_id_build_starts_the_local_daemon() {
    let local = [
        ConnectTarget::Local,
        ConnectTarget::Session {
            name: "work".into(),
            development: false,
        },
    ];
    for target in local {
        assert_eq!(may_start(&target), !cfg!(feature = "mas"), "{target:?}");
    }
    assert!(!may_start(&ConnectTarget::Socket(socket())));
}
