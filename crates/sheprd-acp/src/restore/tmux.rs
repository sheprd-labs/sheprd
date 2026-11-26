//! Keep the provider's session on the machine that owns the tmux terminal.
use super::{Agent, AgentChoice, Continuity, EndpointId, Path, PathBuf};
use sheprd_backend::TerminalId;
use sheprd_tmux::{ChatRecovery, Tmux};

#[derive(Clone)]
pub(super) struct Keeper {
    id: TerminalId,
    cwd: PathBuf,
    chat: ChatRecovery,
}

impl Keeper {
    pub(super) fn new(
        endpoint: Option<&EndpointId>,
        agent: &AgentChoice,
        adapter: bool,
        cwd: &Path,
    ) -> Option<Self> {
        let id = Tmux::caller()?;
        let AgentChoice::Builtin { agent, args } = agent else {
            return None;
        };
        Some(Self {
            id,
            cwd: cwd.into(),
            chat: ChatRecovery {
                agent: *agent,
                adapter,
                args: args
                    .iter()
                    .map(|arg| arg.to_str().map(str::to_owned))
                    .collect::<Option<_>>()?,
                endpoint: endpoint?.to_string(),
                session: String::new(),
                helper: std::env::current_exe().ok()?,
                directories: Default::default(),
            },
        })
    }
}

impl Continuity for Keeper {
    fn copy(&self) -> Box<dyn Continuity> {
        Box::new(self.clone())
    }

    fn hold_for_switch(&self) -> crate::Result<()> {
        Ok(Tmux::new(0).close_chat(&self.id)?)
    }

    fn switched(&mut self, agent: Agent) {
        self.chat.agent = agent;
        self.chat.args.clear();
    }

    fn opened(&self, session: &str) {
        let directories = sheprd_client::herdr::AgentKind::parse(self.chat.agent.name())
            .into_iter()
            .flat_map(sheprd_client::herdr::sessions::environment_keys)
            .filter_map(|key| Some(((*key).to_owned(), std::env::var(key).ok()?)))
            .collect();
        let chat = ChatRecovery {
            directories,
            session: session.into(),
            ..self.chat.clone()
        };
        if let Err(error) = Tmux::new(0).keep_chat(&self.id, self.cwd.clone(), chat) {
            self.ended();
            eprintln!("sheprd-acp: could not keep tmux chat: {error}");
        }
    }

    fn ended(&self) {
        if let Err(error) = Tmux::new(0).close_chat(&self.id) {
            eprintln!("sheprd-acp: could not close tmux chat record: {error}");
        }
    }
}
