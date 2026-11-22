use super::*;
use control::{ControlCommand, McpClient, McpCommand};

fn mcp(args: &[&str]) -> Result<ControlCommand, CliError> {
    let mut all = vec!["mcp"];
    all.extend_from_slice(args);
    match LaunchOptions::parse(all)?.mode {
        LaunchMode::Control(invocation) => Ok(invocation.command),
        mode => panic!("{mode:?}"),
    }
}

#[test]
fn mcp_serves_by_default_and_prints_one_or_every_clients_setup() {
    assert_eq!(mcp(&[]).unwrap(), ControlCommand::Mcp(McpCommand::Serve));
    assert_eq!(
        mcp(&["serve"]).unwrap(),
        ControlCommand::Mcp(McpCommand::Serve)
    );
    assert_eq!(
        mcp(&["config"]).unwrap(),
        ControlCommand::Mcp(McpCommand::Config { client: None })
    );
    for client in McpClient::ALL {
        assert_eq!(
            mcp(&["config", "--client", client.key()]).unwrap(),
            ControlCommand::Mcp(McpCommand::Config {
                client: Some(*client)
            })
        );
    }
}

#[test]
fn mcp_refuses_unknown_verbs_clients_and_leftovers() {
    assert!(mcp(&["start"]).is_err());
    assert!(mcp(&["config", "--client", "vscode"]).is_err());
    assert!(mcp(&["serve", "extra"]).is_err());
}
