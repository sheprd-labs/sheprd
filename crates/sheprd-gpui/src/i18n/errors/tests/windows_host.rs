use super::*;
use crate::windows_host::Error;

#[test]
fn every_windows_host_variant_has_a_message() {
    assert_localized(&[
        Error::Closed,
        Error::Size,
        Error::Start(std::io::Error::other("no ssh")),
        Error::Timeout,
    ]);
}
