use crate::gitlab::{Api, Scheme, agent, cli_hosts, sign_in_agent};
use std::time::Duration;

/// The authorities the agent `make` builds for `subject` verifies a
/// certificate against, read back from that agent.
fn roots(subject: &str, make: impl Fn(&str) -> ureq::Agent) -> String {
    format!("{:?}", make(subject).config().tls_config().root_certs())
}

/// The same, for the agent a remote's API is read with.
fn api_roots(api: &Api) -> String {
    format!(
        "{:?}",
        agent(api, Duration::from_secs(1))
            .config()
            .tls_config()
            .root_certs()
    )
}

/// A self-managed instance is usually served by the organization's own
/// authority, which only the machine's store holds: checked against Mozilla's
/// roots instead, GitLab would answer `UnknownIssuer` and be unreadable. That
/// is the reported failure at `https://gitlab.suse.de`. gitlab.com needs no
/// store on the machine, so it keeps Mozilla's, and a port it names changes
/// nothing, since an authority installed on the machine must not read that
/// token.
#[test]
fn a_self_managed_instance_is_verified_against_the_system() {
    let sign_in = |host: &str| roots(host, |host| sign_in_agent(host, Duration::from_secs(1)));

    for host in [
        "gitlab.suse.de",
        "gitlab.example.com",
        "GitLab.Example.COM",
        "gitlab.example.com:8443",
        "gitlab.com.example.com",
        "notgitlab.com",
    ] {
        assert!(
            sign_in(host).contains("PlatformVerifier"),
            "{host} should be verified against the system's certificates"
        );
    }
    for host in [
        "gitlab.com",
        "GitLab.com",
        "GITLAB.COM",
        "gitlab.com:443",
        "gitlab.com:8443",
    ] {
        assert!(
            sign_in(host).contains("WebPki"),
            "{host} should be verified against Mozilla's certificates"
        );
    }
}

/// The host dialled decides, not the instance's name: glab may send a remote
/// on `gitlab-ssh.example.com` to that instance's `api_host`, and the
/// certificate presented is the one at the address requests go to.
#[test]
fn the_host_dialled_decides_and_the_instance_does_not() {
    let mapped = |instance: &str, api_host: &str, ssh_host: &str| {
        Api::new(
            ssh_host,
            Scheme::Https,
            &[cli_hosts::CliHost {
                host: instance.into(),
                api_host: Some(api_host.into()),
                api_protocol: None,
                ssh_host: Some(ssh_host.into()),
            }],
        )
    };

    // Both names are self-managed, so the roots agree either way.
    let self_managed = mapped(
        "gitlab.example.com",
        "gitlab-api.example.com",
        "gitlab-ssh.example.com",
    );
    assert_eq!(self_managed.instance, "gitlab.example.com");
    assert_eq!(self_managed.host, "gitlab-api.example.com");
    assert!(api_roots(&self_managed).contains("PlatformVerifier"));

    // An instance's API served from a self-managed host instead: the two names
    // disagree, so an agent built from the instance would trust Mozilla's roots
    // where the certificate presented is the self-managed host's.
    let elsewhere = mapped(
        "gitlab.com",
        "gitlab-api.example.com",
        "gitlab-ssh.example.com",
    );
    assert_eq!(elsewhere.instance, "gitlab.com");
    assert_eq!(elsewhere.host, "gitlab-api.example.com");
    assert!(api_roots(&elsewhere).contains("PlatformVerifier"));

    // gitlab.com's own API keeps Mozilla's roots, as it needs no store on the
    // machine to be read.
    let public = Api::of_instance("gitlab.com", &[]);
    assert!(api_roots(&public).contains("WebPki"));
}
