//! Decode incoming MP3 bytes on the network worker. The audio device only
//! consumes decoded samples, so a stalled socket cannot block its callback.
use super::{Error, http_agent};
use rodio::{Player, Source, buffer::SamplesBuffer};
use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, PartialEq)]
pub(crate) enum Kind {
    Message {
        window: gpui::EntityId,
        pane: gpui::SharedString,
        entry: u64,
    },
    Preview,
}

struct Playback {
    kind: Kind,
    agent: ureq::Agent,
    cancelled: AtomicBool,
}

static ACTIVE: Mutex<Option<Arc<Playback>>> = Mutex::new(None);

pub(crate) struct Session(Arc<Playback>);

impl Session {
    pub(crate) fn begin(kind: Kind) -> Option<Self> {
        let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(previous) = active.take() {
            previous.stop();
            if matches!(&kind, Kind::Message { .. }) && previous.kind == kind {
                return None;
            }
        }
        let next = Arc::new(Playback {
            kind,
            agent: http_agent(300),
            cancelled: AtomicBool::new(false),
        });
        *active = Some(next.clone());
        Some(Self(next))
    }

    pub(crate) fn playing(kind: &Kind) -> bool {
        ACTIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|p| &p.kind == kind && !p.cancelled.load(Ordering::Acquire))
    }

    pub(crate) fn stop_preview() {
        let active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(playing) = active.as_ref().filter(|p| p.kind == Kind::Preview) {
            playing.stop();
        }
    }

    pub(super) fn agent(&self) -> &ureq::Agent {
        &self.0.agent
    }
    pub(crate) fn cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }
}

impl Playback {
    fn stop(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.agent.clone().shutdown();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        if active.as_ref().is_some_and(|p| Arc::ptr_eq(p, &self.0)) {
            active.take();
        }
    }
}

/// Rodio accepts a seek interface even when decoding a forward-only stream.
/// Preserve transport errors: the decoder iterator would turn them into EOF.
struct Forward<R> {
    reader: R,
    failed: Arc<Mutex<Option<io::Error>>>,
}

impl<R: Read> Read for Forward<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buffer).inspect_err(|err| {
            *self.failed.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(io::Error::new(err.kind(), err.to_string()));
        })
    }
}
impl<R> Seek for Forward<R> {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

fn decode<R: Read + Send + Sync + 'static>(
    reader: R,
    mut enqueue: impl FnMut(SamplesBuffer) -> bool,
) -> Result<(), Error> {
    let failed = Arc::new(Mutex::new(None));
    let mut decoder = rodio::Decoder::builder()
        .with_data(Forward {
            reader,
            failed: failed.clone(),
        })
        .with_hint("mp3")
        .with_seekable(false)
        .build()?;
    let channels = decoder.channels();
    let rate = decoder.sample_rate();
    let block = (rate.get() as usize / 25).max(1) * usize::from(channels.get());
    loop {
        let samples: Vec<_> = decoder.by_ref().take(block).collect();
        if let Some(err) = failed.lock().unwrap_or_else(|e| e.into_inner()).take() {
            return Err(Error::Stream(err));
        }
        if samples.is_empty() || !enqueue(SamplesBuffer::new(channels, rate, samples)) {
            return Ok(());
        }
    }
}

pub(super) fn play(body: ureq::Body, session: &Session) -> Result<(), Error> {
    let output = rodio::DeviceSinkBuilder::open_default_sink()?;
    let player = Player::connect_new(output.mixer());
    play_to(body, session, &player)
}

fn play_to(body: ureq::Body, session: &Session, player: &Player) -> Result<(), Error> {
    let reader = body.into_with_config().limit(8 * 1024 * 1024).reader();
    decode(reader, |samples| {
        while player.len() >= 6 && !session.cancelled() {
            std::thread::sleep(Duration::from_millis(5));
        }
        if session.cancelled() {
            return false;
        }
        player.append(samples);
        true
    })?;
    while !player.empty() && !session.cancelled() {
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        net::{TcpListener, TcpStream},
        time::Instant,
    };

    #[test]
    fn decodes_before_eof_and_closes_a_stalled_download() {
        let mp3 = include_bytes!("../../../../assets/sounds/request.mp3");
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            // Consume the request headers before waiting for the client to close.
            let mut line = Vec::new();
            while !line.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                line.push(byte[0]);
            }
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: {}\r\n\r\n",
                mp3.len()
            )
            .unwrap();
            socket.write_all(&mp3[..mp3.len() / 2]).unwrap();
            let mut end = [0];
            matches!(TcpStream::read(&mut socket, &mut end), Ok(0))
        });
        let session = Session::begin(Kind::Preview).unwrap();
        let (player, mut output) = Player::new();
        let start = Instant::now();
        let worker = std::thread::spawn(move || {
            let response = session
                .agent()
                .get(format!("http://{address}/speech"))
                .call()
                .unwrap();
            let _ = play_to(response.into_body(), &session, &player);
            session.cancelled()
        });
        // Consume the same Rodio queue the device consumes, in real time.
        // The server will never send EOF until Stop closes the socket.
        loop {
            assert!(
                start.elapsed() < Duration::from_secs(4),
                "no output before EOF"
            );
            if output.by_ref().take(480).any(|s| s.abs() > 0.0001) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        eprintln!("First non-silent output after {:?}", start.elapsed());
        let stop = Instant::now();
        Session::stop_preview();
        assert!(worker.join().unwrap());
        assert!(stop.elapsed() < Duration::from_secs(1));
        assert!(
            server.join().unwrap(),
            "Stop must close the unfinished response"
        );
    }
}
