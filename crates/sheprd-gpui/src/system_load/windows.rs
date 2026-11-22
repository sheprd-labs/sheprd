//! A Windows host's CPU, memory, home disk and uptime. Such a host has no
//! `/bin/sh` for the remote [`Shell`](crate::usage::Shell) to run, so one
//! `powershell.exe` stays open over SSH instead and answers each line read
//! from its stdin with one sample. It answers in the words of `/proc/stat`,
//! `/proc/meminfo` and `df -Pk`, so [`super::sample::parse`] reads it as it
//! reads a Linux host's.

use crate::Result;
use std::{
    io::{self, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// Long enough for PowerShell to start and compile the script's Win32 imports.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// An answer is a few hundred bytes; far more is not this script.
const LIMIT: usize = 64 * 1024;
/// The line that ends each answer.
pub(super) const END: &str = "@@sheprd-sample-end";

/// `GetSystemTimes` counts idle time within kernel time, summed over every
/// core since boot, so `user 0 kernel-idle idle` adds up as `/proc/stat`'s
/// columns do and the next answer's difference is the usage between them.
/// Memory is `GlobalMemoryStatusEx`'s available memory, as Task Manager
/// counts it; the disk is the volume holding the user's profile. Every
/// number printed is an integer, so no culture's decimal separator reaches
/// the parser.
// i18n: data
pub(super) const SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Add-Type -Namespace Sheprd -Name HostLoad -MemberDefinition @'
[DllImport("kernel32.dll")]
public static extern bool GetSystemTimes(out long idle, out long kernel, out long user);
[StructLayout(LayoutKind.Sequential)]
public struct Memory {
    public uint Length; public uint Load; public ulong Total; public ulong Available;
    public ulong TotalPage; public ulong AvailablePage; public ulong TotalVirtual;
    public ulong AvailableVirtual; public ulong AvailableExtended;
}
[DllImport("kernel32.dll")]
public static extern bool GlobalMemoryStatusEx(ref Memory memory);
[DllImport("kernel32.dll")]
public static extern ulong GetTickCount64();
'@
try { $drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($env:USERPROFILE)) } catch { $drive = $null }
$out = [Console]::Out
while ($null -ne [Console]::In.ReadLine()) {
    $idle = 0L; $kernel = 0L; $user = 0L
    if ([Sheprd.HostLoad]::GetSystemTimes([ref]$idle, [ref]$kernel, [ref]$user)) {
        $out.WriteLine("cpu $user 0 $($kernel - $idle) $idle")
    }
    $memory = New-Object Sheprd.HostLoad+Memory
    $memory.Length = [Runtime.InteropServices.Marshal]::SizeOf($memory)
    if ([Sheprd.HostLoad]::GlobalMemoryStatusEx([ref]$memory)) {
        $out.WriteLine("MemTotal: $($memory.Total -shr 10) kB")
        $out.WriteLine("MemAvailable: $($memory.Available -shr 10) kB")
    }
    $out.WriteLine("cores $([Environment]::ProcessorCount)")
    $out.WriteLine("uptime $([uint64][math]::Floor([Sheprd.HostLoad]::GetTickCount64() / 1000))")
    if ($null -ne $drive) {
        try {
            $total = $drive.TotalSize -shr 10
            $used = $total - ($drive.TotalFreeSpace -shr 10)
            $percent = if ($total -gt 0) { [uint64][math]::Ceiling($used * 100 / $total) } else { 0 }
            $out.WriteLine("disk $($drive.Name) $total $used $($drive.AvailableFreeSpace -shr 10) $percent% $($drive.Name)")
        } catch {}
    }
    $out.WriteLine('@@sheprd-sample-end')
    $out.Flush()
}
"#;

/// One process answering as [`SCRIPT`] does.
pub(super) struct Sampler {
    child: Child,
    stdin: ChildStdin,
    output: mpsc::Receiver<io::Result<Vec<u8>>>,
    buffer: Vec<u8>,
    /// The host's shared SSH channel this runs on, if any. Fields drop after
    /// `Drop::drop` reaps the child, so it is freed only then.
    _channel: Option<sheprd_client::SshChannel>,
}

impl Drop for Sampler {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Sampler {
    /// Starts [`SCRIPT`] on `target` and waits for its first answer, which
    /// proves the host runs it. Blocking: call it from the host's worker.
    pub(super) fn connect(target: &str) -> Result<Self> {
        let (mut command, channel) = sheprd_client::shared_powershell_command(
            target,
            SCRIPT,
            sheprd_client::SshLane::Quick,
        )?;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut sampler = Self::start(command, channel)?;
        sampler.sample(CONNECT_TIMEOUT)?;
        Ok(sampler)
    }

    /// Any process answering as [`SCRIPT`] does: SSH in production, a local
    /// shell in tests.
    pub(super) fn start(
        mut command: Command,
        channel: Option<sheprd_client::SshChannel>,
    ) -> Result<Self> {
        let mut child = command.spawn()?;
        let (Some(stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::from(io::ErrorKind::BrokenPipe).into());
        };
        let (sender, output) = mpsc::sync_channel(64);
        let reader = thread::Builder::new()
            .name("herdr-system-load-windows".into())
            .spawn(move || {
                let mut chunk = [0; 8192];
                loop {
                    match stdout.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            if sender.send(Ok(chunk[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                        Err(error) => {
                            let _ = sender.send(Err(error));
                            break;
                        }
                    }
                }
            });
        if let Err(error) = reader {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        Ok(Self {
            child,
            stdin,
            output,
            buffer: Vec::new(),
            _channel: channel,
        })
    }

    /// Asks for one answer and returns it without its [`END`] line. After a
    /// failure the sampler is dropped, since a late answer could still arrive.
    pub(super) fn sample(&mut self, timeout: Duration) -> Result<String> {
        self.stdin
            .write_all(b"\n")
            .and_then(|()| self.stdin.flush())
            .map_err(|_| sheprd_usage::Error::Unreachable)?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(answer) = self.take_answer() {
                return Ok(answer);
            }
            if self.buffer.len() > LIMIT {
                return Err(sheprd_usage::Error::Size.into());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            match self.output.recv_timeout(left) {
                Ok(Ok(chunk)) => self.buffer.extend_from_slice(&chunk),
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(sheprd_usage::Error::Unreachable.into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(sheprd_usage::Error::Timeout.into());
                }
            }
        }
    }

    /// The text before the first line that is [`END`], taken from the buffer
    /// with that line; None until that line has arrived in full.
    fn take_answer(&mut self) -> Option<String> {
        let end = END.as_bytes();
        let mut from = 0;
        let start = loop {
            let at = from + find(&self.buffer[from..], end)?;
            if at == 0 || self.buffer[at - 1] == b'\n' {
                break at;
            }
            from = at + 1;
        };
        let newline = start + self.buffer[start..].iter().position(|b| *b == b'\n')?;
        let answer = String::from_utf8_lossy(&self.buffer[..start]).into_owned();
        self.buffer.drain(..=newline);
        Some(answer)
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
