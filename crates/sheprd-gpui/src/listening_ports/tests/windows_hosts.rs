//! A Windows host's ports: its PowerShell answer, its daemon's socket paths,
//! and, on Windows, the script run against a real listener.
use super::*;
use crate::listening_ports::windows::script;

/// One answer as the Windows script prints it, with CRLF line endings: a
/// dev server in workspace `w1`, its IPv4 and IPv6 sockets, a listener
/// without a workspace, a socket path with a space, and Claude Code's own
/// listener in the same workspace, run by a Node installed under a folder
/// with a space.
const ANSWER: &str = "L 4242 0.0.0.0:5173 node\r\n\
L 4242 [::]:5173 node\r\n\
L 4242 127.0.0.1:9229 node\r\n\
L 900 0.0.0.0:445 System\r\n\
L 777 127.0.0.1:61000 node\r\n\
E 4242 w1 C:\\Users\\Ada Lovelace\\.config\\herdr\\herdr.sock\r\n\
W 4242 \"C:\\Program Files\\nodejs\\node.exe\" node_modules\\vite\\bin\\vite.js\r\n\
E 777 w1 C:\\Users\\Ada Lovelace\\.config\\herdr\\herdr.sock\r\n\
W 777 \"C:\\Program Files\\nodejs\\node.exe\" \
\"C:\\Users\\Ada Lovelace\\AppData\\Roaming\\npm\\node_modules\\@anthropic-ai\\claude-code\\cli.js\"\r\n";

#[test]
fn a_windows_answer_reads_like_any_host() {
    let scan = parse(ANSWER).unwrap();
    assert_eq!(
        of(&scan, "w1"),
        [
            port(5173, Bind::Any, "node"),
            port(9229, Bind::Loopback, "node")
        ]
    );
    let owner = scan.pids.get(&4242).unwrap();
    assert_eq!(
        owner.socket,
        "C:\\Users\\Ada Lovelace\\.config\\herdr\\herdr.sock"
    );
    assert!(!scan.pids.contains_key(&900), "no workspace, not listed");
    // The agent's listener names its workspace but is no page to open.
    assert_eq!(scan.pids.get(&777).unwrap().workspace, "w1");
}

/// The daemon's socket path is the Windows host's own, whatever this machine
/// runs, so its sessions are told apart by `\` as by `/`.
#[test]
fn a_windows_socket_path_names_its_session() {
    let default = "C:\\Users\\ada\\.config\\herdr\\herdr.sock";
    let work = "C:\\Users\\ada\\.config\\herdr\\sessions\\work\\herdr.sock";
    assert!(remote("default").owns(default));
    assert!(!remote("work").owns(default));
    assert!(remote("work").owns(work));
    assert!(!remote("default").owns(work));
    // Unix paths read as before.
    assert!(remote("work").owns("/home/ada/.config/herdr/sessions/work/herdr.sock"));
    assert!(remote("default").owns("/home/ada/.config/herdr/herdr.sock"));
}

#[test]
fn the_script_lists_listeners_and_reads_their_environment() {
    let script = script();
    assert!(script.contains("Get-NetTCPConnection -State Listen"));
    assert!(script.contains("[HerdrEnvironment]::Of("));
    assert!(script.contains("'HERDR_WORKSPACE_ID'"));
    assert!(script.contains("'HC_TERMINAL_ID'"));
    assert!(script.contains("Start-Sleep -Seconds 5\n"));
    assert!(!script.contains("__"), "every placeholder is filled");
    // The C# here-string ends at the start of its own line, as PowerShell
    // requires.
    assert!(script.contains("\n\"@\n"));
}

/// A listener started with a workspace in its environment, as a Herdr pane
/// starts one: PowerShell holding a socket on a free loopback port.
#[cfg(windows)]
fn listener(workspace: &str) -> (std::process::Child, u16) {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = std::process::Command::new("powershell.exe")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(format!(
            "$l = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, {port}); $l.Start(); Start-Sleep 120"
        ))
        .env("HERDR_WORKSPACE_ID", workspace)
        .env("HERDR_SOCKET_PATH", "C:\\herdr-test\\herdr.sock")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    (child, port)
}

/// Scans until `pid`'s workspace shows, as the worker would.
#[cfg(windows)]
fn find(scanner: &mut super::super::Scanner, pid: u32, workspace: &str, port: u16) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    loop {
        let scan = super::super::scan(scanner).unwrap();
        if let Some(owner) = scan.pids.get(&pid) {
            assert_eq!(owner.workspace, workspace);
            assert_eq!(owner.socket, "C:\\herdr-test\\herdr.sock");
            assert!(
                of(&scan, workspace)
                    .iter()
                    .any(|found| found.number == port)
            );
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{pid} never listed: {scan:?}"
        );
    }
}

/// This machine scanned through its own PowerShell, which reads a listening
/// process's environment for its workspace.
#[cfg(windows)]
#[test]
fn this_windows_machine_names_a_listeners_workspace() {
    let (mut child, port) = listener("wLocal");
    let mut scanner = super::super::open(&Host::Local, false).unwrap();
    find(&mut scanner, child.id(), "wLocal", port);
    let _ = child.kill();
    let _ = child.wait();
}

/// A real Windows SSH host that is this machine, named by
/// `SHEPRD_TEST_WINDOWS_SSH`: its POSIX shell does not come up, and its
/// PowerShell lists a listener this test starts here.
#[cfg(windows)]
#[test]
#[ignore = "needs SHEPRD_TEST_WINDOWS_SSH to name this machine over SSH"]
fn a_windows_ssh_host_names_a_listeners_workspace() {
    let target = std::env::var("SHEPRD_TEST_WINDOWS_SSH").expect("SHEPRD_TEST_WINDOWS_SSH");
    let (mut child, port) = listener("wRemote");
    let mut scanner = super::super::open(&Host::Ssh(target), false).unwrap();
    assert!(matches!(scanner, super::super::Scanner::Windows(_)));
    find(&mut scanner, child.id(), "wRemote", port);
    let _ = child.kill();
    let _ = child.wait();
}

/// The scan script fits the command line `cmd.exe`, Windows OpenSSH's default
/// login shell, accepts.
#[test]
fn the_scan_script_fits_the_command_line() {
    let length = crate::windows_host::tests::remote_command_length(&script());
    assert!(length < crate::windows_host::COMMAND_LIMIT, "{length}");
}
