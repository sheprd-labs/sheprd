use super::{
    Error, Result, Tmux,
    control::{self, Message},
};
use sheprd_backend::{
    ClientPaneInputEvent, History, HistoryAnswer, Role, Size, SurfaceUpdate, TerminalId,
};
use sheprd_terminal::{Emulator, Surface};
use std::{
    fmt::Write as _,
    io::{BufReader, Write},
    process::{Child, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    time::Duration,
};

struct Client(Child);

impl Drop for Client {
    fn drop(&mut self) {
        // The child is only the attaching client. Dropping a view must leave
        // the server and its program running.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub struct TmuxTerminal {
    _client: Option<Client>,
    commands: SyncSender<Command>,
    output: Receiver<Message>,
    emulator: Emulator,
    surface: Surface,
    role: Role,
    ended: bool,
    pending_exit: Option<sheprd_backend::Exit>,
}

pub(super) enum Command {
    Keys(Vec<u8>),
    Resize(Size),
}

fn write_command(out: &mut impl Write, pane: &str, cmd: Command) -> std::io::Result<()> {
    match cmd {
        Command::Resize(size) => writeln!(out, "refresh-client -C {}x{}", size.cols(), size.rows()),
        Command::Keys(bytes) => {
            let mut line = String::new();
            for chunk in bytes.chunks(256) {
                line.clear();
                let _ = write!(line, "send-keys -H -t {pane}");
                for byte in chunk {
                    let _ = write!(line, " {byte:02x}");
                }
                line.push('\n');
                out.write_all(line.as_bytes())?;
            }
            Ok(())
        }
    }
}

impl TmuxTerminal {
    pub(super) fn attach(tmux: &Tmux, id: &TerminalId, size: Size, role: Role) -> Result<Self> {
        let target = Tmux::target(id)?;
        let mut cmd = tmux.command();
        cmd.args([
            "display-message",
            "-p",
            "-t",
            &format!("{target}:"),
            "#{pane_id}",
        ]);
        let response = tmux.run(cmd)?;
        if response.iter().all(u8::is_ascii_whitespace) {
            return Err(Error::MissingPane);
        }
        let mut facts = std::str::from_utf8(&response)
            .map_err(|_| Error::Invalid)?
            .split_whitespace();
        let pane = facts
            .next()
            .filter(|p| {
                p.strip_prefix('%')
                    .is_some_and(|n| n.parse::<u32>().is_ok())
            })
            .ok_or(Error::Invalid)?
            .to_owned();
        let emulator = Emulator::new(size, tmux.scrollback);
        let history_limit = tmux.scrollback.min(100_000);
        let mut cmd = tmux.command();
        cmd.args(["-C", "attach-session", "-t", &target]);
        if role == Role::Observer {
            cmd.args(["-f", "ignore-size,read-only"]);
        }
        tmux.with_command(cmd, true, |cmd| {
            Self::control(cmd, pane, emulator, history_limit, size, role)
        })
    }

    fn control(
        mut cmd: std::process::Command,
        pane: String,
        emulator: Emulator,
        history_limit: usize,
        size: Size,
        role: Role,
    ) -> Result<Self> {
        let mut client = Client(
            cmd.stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?,
        );
        let mut stdin = client.0.stdin.take().ok_or(Error::Invalid)?;
        let stdout = client.0.stdout.take().ok_or(Error::Invalid)?;
        // The reader first seeds the emulator at the pane's actual size;
        // a driving view then asks for its own size.
        const DIMENSIONS: &str = "#{history_size} #{pane_height} #{pane_width} #{alternate_on} #{alternate_saved_x} #{alternate_saved_y}";
        const CURSOR: &str = "#{cursor_x} #{cursor_y} #{keypad_cursor_flag} #{bracket_paste_flag} #{mouse_standard_flag} #{mouse_button_flag} #{mouse_all_flag} #{mouse_sgr_flag} #{mouse_utf8_flag} #{cursor_flag}";
        let sentinel = uuid::Uuid::new_v4().to_string();
        let normal_start = if history_limit > 0 {
            format!(" -S -{history_limit}")
        } else {
            String::new()
        };
        let history_start = history_limit.max(1);
        writeln!(
            stdin,
            "display-message -p -t {pane} '{DIMENSIONS}' ; \
             if-shell -F -t {pane} '#{{alternate_on}}' \
             'capture-pane -p -e -J -S -{history_start} -E -1 -t {pane} ; display-message -p {sentinel} ; \
              capture-pane -a -q -p -e -J -t {pane} ; display-message -p {sentinel} ; \
              capture-pane -p -e -J -t {pane} ; display-message -p {sentinel}' \
             'capture-pane -p -e -J{normal_start} -t {pane} ; display-message -p {sentinel}' ; \
             display-message -p -t {pane} '{CURSOR}'"
        )?;
        // If a capture fails and aborts the command list above, this standalone
        // sentinel still runs so the reader exits the capture block at once.
        writeln!(stdin, "display-message -p {sentinel}")?;
        writeln!(
            stdin,
            "refresh-client -B 'sheprd-exit::#{{pane_dead}}/#{{pane_dead_status}}'"
        )?;
        let (send, output) = mpsc::sync_channel(16);
        let (ready, attached) = mpsc::sync_channel(1);
        let read_pane = pane.clone();
        std::thread::Builder::new()
            .name("tmux-output".into())
            .spawn(move || {
                if let Err(error) = control::read(
                    BufReader::new(stdout),
                    &read_pane,
                    history_limit,
                    &sentinel,
                    &send,
                    Some(ready),
                ) {
                    let _ = send.send(Message::Failed(error));
                }
            })?;
        // The first control reply proves the remote transport authenticated.
        // Keep cloud credentials in scope until that happens.
        attached
            .recv_timeout(Duration::from_secs(20))
            .map_err(|_| std::io::Error::other("tmux did not acknowledge attachment"))?;
        let (commands, queue) = mpsc::sync_channel::<Command>(256);
        std::thread::Builder::new()
            .name("tmux-input".into())
            .spawn(move || {
                for command in queue {
                    if write_command(&mut stdin, &pane, command).is_err() {
                        break;
                    }
                }
            })?;
        let mut terminal = Self {
            _client: Some(client),
            commands,
            output,
            emulator,
            surface: Surface::default(),
            role,
            ended: false,
            pending_exit: None,
        };
        sheprd_backend::Terminal::resize(&mut terminal, size)?;
        Ok(terminal)
    }

    #[cfg(test)]
    pub(super) fn for_test(
        commands: SyncSender<Command>,
        output: Receiver<Message>,
        size: Size,
    ) -> Self {
        Self {
            _client: None,
            commands,
            output,
            emulator: Emulator::new(size, 100),
            surface: Surface::default(),
            role: Role::Active,
            ended: false,
            pending_exit: None,
        }
    }

    fn drain_output(&mut self, timeout: Duration) -> Result<()> {
        if self.ended {
            return Ok(());
        }
        let mut msg = self.output.recv_timeout(timeout);
        for index in 0..128 {
            match msg {
                Ok(Message::Bytes(bytes)) => self.emulator.feed(&bytes),
                Ok(Message::Resize(size)) => self.emulator.resize(size),
                Ok(Message::Exit(exit)) => {
                    self.pending_exit = Some(exit);
                    self.ended = true;
                    break;
                }
                Ok(Message::Failed(error)) => return Err(error),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(Error::Invalid),
            }
            if index == 127 {
                break;
            }
            msg = match self.output.try_recv() {
                Ok(msg) => Ok(msg),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => Err(mpsc::RecvTimeoutError::Disconnected),
            };
        }
        Ok(())
    }

    fn command(&mut self, mut cmd: Command) -> Result<()> {
        loop {
            if self.ended {
                return Ok(());
            }
            match self.commands.try_send(cmd) {
                Ok(()) => return Ok(()),
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err(std::io::Error::other("tmux input closed").into());
                }
                Err(mpsc::TrySendError::Full(unsent)) => {
                    cmd = unsent;
                    // Drain pending output while waiting for room so tmux cannot
                    // stall on stdout while the input thread waits on stdin.
                    self.drain_output(Duration::from_millis(5))?;
                }
            }
        }
    }

    fn keys(&mut self, bytes: Vec<u8>) -> Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.command(Command::Keys(bytes))
    }
}

impl sheprd_backend::Terminal for TmuxTerminal {
    type Error = Error;

    fn input(&mut self, input: &ClientPaneInputEvent) -> Result<()> {
        let bytes = self.emulator.input_bytes(input, self.role);
        self.keys(bytes)
    }

    fn resize(&mut self, size: Size) -> Result<()> {
        if self.role == Role::Active {
            self.command(Command::Resize(size))?;
            // The server publishes the applied layout in control mode.
        }
        Ok(())
    }

    fn focus(&mut self, focused: bool) -> Result<()> {
        if self.role == Role::Active && self.emulator.modes().focus_reporting {
            self.keys(if focused { b"\x1b[I" } else { b"\x1b[O" }.to_vec())?;
        }
        Ok(())
    }

    fn cell_pixels(&mut self, width: u16, height: u16) -> Result<()> {
        self.emulator.set_cell_pixels(width.into(), height.into());
        Ok(())
    }

    fn next(&mut self, timeout: Duration) -> Result<Vec<SurfaceUpdate>> {
        self.drain_output(self.surface.timeout(timeout))?;
        let mut updates = self.surface.updates(&mut self.emulator, self.ended);
        if let Some(exit) = self.pending_exit.take() {
            updates.push(SurfaceUpdate::Ended(exit));
        }
        Ok(updates)
    }

    fn history(&mut self, request: &History) -> Result<HistoryAnswer> {
        Ok(self.emulator.history(request)?)
    }
}
