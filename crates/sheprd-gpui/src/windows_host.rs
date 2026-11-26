//! A Windows host's PowerShell, kept open to report on the host every few
//! seconds: its load for `system_load`, its listening ports for
//! `listening_ports`. Its login shell is `cmd.exe` or PowerShell, so the
//! POSIX steps a Linux or macOS host runs never start there.
//!
//! Each feature gives the lines it prints; [`script`] wraps them in a loop
//! that prints one answer every period on its own, ended by [`END`], and
//! [`Session::sample`] takes the newest whole one. Nothing is asked for on
//! stdin because nothing would reach it: when the host's login shell is
//! PowerShell, sshd runs the command under `powershell -c`, which reads stdin
//! to its end before it runs the command. So stdin is empty, and a script
//! that should report differently is started again.
//!
//! On a Windows client, OpenSSH's `ssh.exe` stops delivering output into a
//! synchronous pipe or a file once its stdin has ended, so its stdout there
//! is an overlapped pipe (`sheprd_client::overlapped_output`).

mod error;
#[cfg(test)]
pub(crate) mod tests;

pub(crate) use error::Error;

use crate::Result;
use std::{
    io::Read,
    process::{Child, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// Ends each answer on a line of its own.
const END: &str = "@@herdr-answer-end"; // i18n: data

/// Output an answer, or what has come of an unfinished one, may reach.
const LIMIT: usize = 256 * 1024;

/// Chunks the reader may hold before the session takes them; also how many
/// one pass of [`Session::sample`] takes without waiting.
const CHANNEL: usize = 64;

/// How often a wait checks that the process is still running.
const POLL: Duration = Duration::from_millis(250);

/// How long the first answer may take: SSH, PowerShell loading its modules,
/// and a script compiling C#, on a slow or busy machine. Waiting less would
/// start it all again on every retry and might never get an answer. An SSH
/// connection that fails ends `ssh`, which is noticed at once.
const FIRST_ANSWER: Duration = Duration::from_secs(120);

/// The loop around a feature's lines. sshd ends only its own session process
/// when the connection does, not the shell it started, and a write to the
/// closed connection can fail silently; so the script ends itself when that
/// `sshd` ancestor is gone, or, started some other way, its parent.
// i18n: data
const TEMPLATE: &str = r#"$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'
$herdrWatch = $null
$herdrId = $PID
for ($herdrDepth = 0; $herdrDepth -lt 8; $herdrDepth++) {
    $herdrId = (Get-CimInstance Win32_Process -Filter "ProcessId=$herdrId").ParentProcessId
    if ($null -eq $herdrId) { break }
    $herdrAncestor = Get-Process -Id $herdrId
    if ($null -eq $herdrAncestor) { break }
    try { $null = $herdrAncestor.Handle } catch { break }
    if ($null -eq $herdrWatch) { $herdrWatch = $herdrAncestor }
    if ($herdrAncestor.ProcessName -like 'sshd*') { $herdrWatch = $herdrAncestor; break }
}
__SETUP__
while ($null -ne $herdrWatch -and -not $herdrWatch.HasExited) {
    $herdrOut = New-Object System.Text.StringBuilder
__BODY__
    [void]$herdrOut.AppendLine('__END__')
    try { [Console]::Out.Write($herdrOut.ToString()); [Console]::Out.Flush() } catch { exit }
    Start-Sleep -Seconds __PERIOD__
}
"#;

/// A script that runs `setup` once, then every `period` runs `body`, which
/// appends the answer's lines to the `StringBuilder` `$herdrOut`, and prints
/// them. Their variables should start with `$herdr`, as the loop's do.
pub(crate) fn script(setup: &str, body: &str, period: Duration) -> String {
    TEMPLATE
        .replace("__SETUP__", setup)
        .replace("__BODY__", body)
        .replace("__END__", END)
        .replace("__PERIOD__", &period.as_secs().max(1).to_string())
}

/// The longest command `cmd.exe`, Windows OpenSSH's default login shell,
/// runs; each feature's tests hold its script under it.
#[cfg(test)]
pub(crate) const COMMAND_LIMIT: usize = 8191;

/// `script` deflated, inside a short script that inflates and runs it, so the
/// encoded command sshd hands the login shell stays under the 8,191
/// characters `cmd.exe`, Windows OpenSSH's default login shell, accepts;
/// `powershell.exe -EncodedCommand` alone doubles a script and grows it by a
/// third again. The stream and the mode are named in full: PowerShell 5.1
/// finds a bare `0` ambiguous between `DeflateStream`'s constructors.
pub(crate) fn compressed(script: &str) -> Result<String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::io::Write as _;
    let mut deflate = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    deflate.write_all(script.as_bytes()).map_err(Error::Start)?;
    let bytes = deflate.finish().map_err(Error::Start)?;
    Ok(format!(
        // i18n: data
        "iex ((New-Object IO.StreamReader (New-Object IO.Compression.DeflateStream ((New-Object IO.MemoryStream (,[Convert]::FromBase64String('{}'))), [IO.Compression.CompressionMode]::Decompress))).ReadToEnd())",
        STANDARD.encode(bytes)
    ))
}

/// A PowerShell running a [`script`], and what it has printed so far.
pub(crate) struct Session {
    child: Child,
    output: mpsc::Receiver<std::io::Result<Vec<u8>>>,
    buffer: Vec<u8>,
    /// Whether an answer has come yet; see [`FIRST_ANSWER`].
    answered: bool,
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Session {
    /// `script` on the Windows host `target`, over an SSH connection of its
    /// own (see `sheprd_client::powershell_command`), so that ending `ssh`
    /// ends the connection and the script with it.
    pub fn connect(target: &str, script: &str) -> Result<Self> {
        let mut command = sheprd_client::powershell_command(target, &compressed(script)?)?;
        Self::start(command.stderr(Stdio::null()))
    }

    /// `script` on this Windows machine.
    #[cfg(windows)]
    pub fn local(script: &str) -> Result<Self> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut command = sheprd_client::new_command("powershell.exe");
        command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
            ])
            .arg(STANDARD.encode(utf16))
            .stderr(Stdio::null());
        Self::start(&mut command)
    }

    /// Any process printing a [`script`]'s answers on stdout, with nothing on
    /// stdin. PowerShell writes progress records to stderr as CLIXML, so
    /// callers discard it.
    pub fn start(command: &mut std::process::Command) -> Result<Self> {
        let (reader, stdout) = stdout()?;
        let spawned = command.stdin(Stdio::null()).stdout(stdout).spawn();
        // The command holds the child's end of the pipe until dropped or
        // replaced; an exited child must read as closed.
        command.stdout(Stdio::null());
        let mut child = spawned.map_err(Error::Start)?;
        let output = match read_on_thread(reader) {
            Ok(output) => output,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        Ok(Self {
            child,
            output,
            buffer: Vec::new(),
            answered: false,
        })
    }

    /// The newest whole answer, without its [`END`] line, waiting up to
    /// `timeout` for one when none has arrived since the last, or up to
    /// [`FIRST_ANSWER`] for the first. Older answers are dropped, so a caller
    /// that fell behind never reads a stale one. Any failure leaves the
    /// session unusable.
    pub fn sample(&mut self, timeout: Duration) -> Result<String> {
        let timeout = if self.answered {
            timeout
        } else {
            timeout.max(FIRST_ANSWER)
        };
        let deadline = Instant::now() + timeout;
        let mut latest = None;
        loop {
            // Take what has already arrived without waiting, at most a full
            // channel's worth, so a host that never stops printing cannot
            // keep this loop from its checks.
            for _ in 0..CHANNEL {
                match self.output.try_recv() {
                    Ok(Ok(chunk)) => latest = add(&mut self.buffer, &chunk)?.or(latest),
                    Ok(Err(_)) | Err(mpsc::TryRecvError::Disconnected) => {
                        return Err(Error::Closed.into());
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                }
            }
            if let Some(text) = latest {
                self.answered = true;
                return Ok(text);
            }
            // A child of the child, such as `ssh`'s, can keep the pipe open
            // after the process itself has exited; that is closed too.
            if !matches!(self.child.try_wait(), Ok(None)) {
                return Err(Error::Closed.into());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::Timeout.into());
            }
            match self.output.recv_timeout(left.min(POLL)) {
                Ok(Ok(chunk)) => latest = add(&mut self.buffer, &chunk)?,
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(Error::Closed.into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

/// Adds `chunk` to `buffer` and returns the newest whole answer it
/// completed, removing it and every one before it. Neither an answer nor
/// what is left of an unfinished one may pass [`LIMIT`], so `buffer` never
/// holds more than that and one chunk.
fn add(buffer: &mut Vec<u8>, chunk: &[u8]) -> Result<Option<String>> {
    buffer.extend_from_slice(chunk);
    let latest = take_latest(buffer);
    if buffer.len() > LIMIT || latest.as_ref().is_some_and(|text| text.len() > LIMIT) {
        return Err(Error::Size.into());
    }
    Ok(latest)
}

/// A pipe for the child's stdout: the end read here, and the child's.
#[cfg(windows)]
fn stdout() -> Result<(std::fs::File, Stdio)> {
    sheprd_client::overlapped_output().map_err(|error| Error::Start(error).into())
}

#[cfg(not(windows))]
fn stdout() -> Result<(std::io::PipeReader, Stdio)> {
    let (reader, writer) = std::io::pipe().map_err(Error::Start)?;
    Ok((reader, writer.into()))
}

/// Sends what `source` gives, chunk by chunk, until it ends or fails.
fn read_on_thread(
    mut source: impl Read + Send + 'static,
) -> Result<mpsc::Receiver<std::io::Result<Vec<u8>>>> {
    let (sender, output) = mpsc::sync_channel(CHANNEL);
    thread::Builder::new()
        .name("herdr-windows-host".into())
        .spawn(move || {
            let mut chunk = [0; 8192];
            loop {
                match source.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if sender.send(Ok(chunk[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        })
        .map_err(Error::Start)?;
    Ok(output)
}

/// The last whole answer in `buffer`, removed with every one before it; None
/// until one has arrived.
fn take_latest(buffer: &mut Vec<u8>) -> Option<String> {
    let mut latest = None;
    while let Some(text) = take_answer(buffer) {
        latest = Some(text);
    }
    latest
}

/// The text before the first complete [`END`] line, removed from `buffer`
/// with that line; None until one has arrived. Lines may end in CRLF.
fn take_answer(buffer: &mut Vec<u8>) -> Option<String> {
    let mut start = 0;
    while let Some(offset) = buffer[start..].iter().position(|byte| *byte == b'\n') {
        let end = start + offset;
        if buffer[start..end].trim_ascii() == END.as_bytes() {
            let text = String::from_utf8_lossy(&buffer[..start]).into_owned();
            buffer.drain(..=end);
            return Some(text);
        }
        start = end + 1;
    }
    None
}
