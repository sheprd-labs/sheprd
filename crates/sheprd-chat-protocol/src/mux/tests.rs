use super::*;
use std::{
    io::{self, Read, Write},
    sync::mpsc,
    thread,
    time::Duration,
};

const ID: &str = "0123456789abcdef0123456789abcdef";
const OTHER: &str = "fedcba9876543210fedcba9876543210";

fn endpoint(id: &str) -> crate::EndpointId {
    id.parse().unwrap()
}

fn encoded(frames: &[Frame]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for frame in frames {
        frame.encode(&mut bytes).unwrap();
    }
    bytes
}

fn decode(bytes: &[u8]) -> Result<Option<Frame>> {
    Frame::read_from(&mut &bytes[..])
}

/// Runs `mux`'s loops over a fresh pair of pipes, returning the far ends.
fn wire(mux: &Mux) -> (io::PipeReader, io::PipeWriter) {
    let (to_mux_reader, to_mux) = io::pipe().unwrap();
    let (from_mux, from_mux_writer) = io::pipe().unwrap();
    let sender = mux.clone();
    thread::spawn(move || sender.send_loop(from_mux_writer));
    let receiver = mux.clone();
    thread::spawn(move || receiver.receive_loop(to_mux_reader));
    (from_mux, to_mux)
}

/// A client and host connected, once the host has greeted.
fn connected(max_streams: usize) -> (Mux, Mux) {
    let (client, host) = (Mux::client(), Mux::host(max_streams));
    let (client_in, client_out) = wire(&client);
    let (host_in, host_out) = wire(&host);
    thread::spawn(move || io::copy(&mut { client_in }, &mut { host_out }));
    thread::spawn(move || io::copy(&mut { host_in }, &mut { client_out }));
    client.wait_hello(Duration::from_secs(5)).unwrap();
    (client, host)
}

/// [`connected`], the host echoing every stream it accepts.
fn echoing(max_streams: usize) -> (Mux, Mux) {
    let (client, host) = connected(max_streams);
    let accepting = host.clone();
    thread::spawn(move || {
        while let Some((_, stream)) = accepting.accept() {
            thread::spawn(move || {
                let _ = io::copy(&mut stream.clone(), &mut stream.clone());
                stream.close();
            });
        }
    });
    (client, host)
}

fn round_trip(stream: &Stream, text: &[u8]) {
    stream.clone().write_all(text).unwrap();
    let mut back = vec![0; text.len()];
    stream.clone().read_exact(&mut back).unwrap();
    assert_eq!(back, text);
}

#[test]
fn every_frame_round_trips() {
    let frames = [
        Frame::hello(16),
        Frame::Open {
            stream: 1,
            endpoint: endpoint(ID),
        },
        Frame::Data {
            stream: 1,
            bytes: vec![7; MAX_DATA],
        },
        Frame::Credit {
            stream: 1,
            bytes: WINDOW,
        },
        Frame::Close {
            stream: 1,
            reason: CloseReason::Unreachable,
        },
    ];
    let bytes = encoded(&frames);
    let mut reader = bytes.as_slice();
    for frame in &frames {
        assert_eq!(Frame::read_from(&mut reader).unwrap().as_ref(), Some(frame));
    }
    assert!(Frame::read_from(&mut reader).unwrap().is_none());
}

#[test]
fn malformed_frames_are_refused() {
    let header = |kind: u8, stream: u32, len: u32| {
        let mut bytes = vec![kind];
        bytes.extend_from_slice(&stream.to_le_bytes());
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes
    };
    assert!(matches!(
        decode(&header(9, 1, 0)),
        Err(Error::UnknownKind(9))
    ));
    let too_large = u32::try_from(MAX_DATA + 1).unwrap();
    assert!(matches!(
        decode(&header(3, 1, too_large)),
        Err(Error::TooLarge(_))
    ));
    assert!(matches!(decode(&header(3, 1, 0)), Err(Error::Malformed(3))));
    // Data on the connection's own id, a greeting on a stream's.
    assert!(matches!(
        decode(&[header(3, 0, 1), vec![1]].concat()),
        Err(Error::Malformed(3))
    ));
    let mut hello = encoded(&[Frame::hello(1)]);
    hello[1] = 1;
    assert!(matches!(decode(&hello), Err(Error::Malformed(1))));
    let mut stranger = encoded(&[Frame::hello(1)]);
    stranger[9] = b'X';
    assert!(matches!(decode(&stranger), Err(Error::NotMultiplexer)));
    let not_an_id = [header(2, 1, 32), vec![b'z'; 32]].concat();
    assert!(matches!(decode(&not_an_id), Err(Error::Malformed(2))));
    let zero_credit = [header(4, 1, 4), vec![0; 4]].concat();
    assert!(matches!(decode(&zero_credit), Err(Error::Malformed(4))));
    let odd_reason = [header(5, 1, 1), vec![9]].concat();
    assert!(matches!(decode(&odd_reason), Err(Error::Malformed(5))));
    let truncated = &encoded(&[Frame::Credit {
        stream: 1,
        bytes: 1,
    }])[..11];
    assert!(matches!(decode(truncated), Err(Error::Io(_))));
    let oversized = Frame::Data {
        stream: 1,
        bytes: vec![0; MAX_DATA + 1],
    };
    assert!(matches!(
        oversized.encode(&mut Vec::new()),
        Err(Error::TooLarge(_))
    ));
}

#[test]
fn streams_carry_bytes_both_ways() {
    let (client, _host) = echoing(MAX_STREAMS);
    let first = client.open(&endpoint(ID)).unwrap();
    let second = client.open(&endpoint(OTHER)).unwrap();
    round_trip(&first, b"hello");
    // More than a window each way, so credit must come back.
    let large: Vec<u8> = (0..WINDOW as usize * 3).map(|n| n as u8).collect();
    let writer = {
        let (stream, large) = (second.clone(), large.clone());
        thread::spawn(move || stream.clone().write_all(&large))
    };
    let mut back = vec![0; large.len()];
    second.clone().read_exact(&mut back).unwrap();
    writer.join().unwrap().unwrap();
    assert_eq!(back, large);
}

#[test]
fn closing_one_stream_leaves_the_others_running() {
    let (client, host) = echoing(MAX_STREAMS);
    let first = client.open(&endpoint(ID)).unwrap();
    let second = client.open(&endpoint(OTHER)).unwrap();
    round_trip(&first, b"one");
    round_trip(&second, b"two");
    first.close();
    assert_eq!(first.clone().read(&mut [0; 4]).unwrap(), 0);
    assert!(first.clone().write(b"x").is_err());
    round_trip(&second, b"still here");
    // The host drops the closed stream once its echo sees the end.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while host.streams() != 1 {
        assert!(std::time::Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_host_refuses_streams_past_its_limit() {
    let (client, _host) = echoing(1);
    let _first = client.open(&endpoint(ID)).unwrap();
    assert!(matches!(client.open(&endpoint(OTHER)), Err(Error::Limit)));

    // A client that ignores the greeting's limit is told no, per stream.
    let host = Mux::host(1);
    let (mut from_host, mut to_host) = wire(&host);
    let opens = [1, 2].map(|stream| Frame::Open {
        stream,
        endpoint: endpoint(ID),
    });
    to_host.write_all(&encoded(&opens)).unwrap();
    assert_eq!(
        Frame::read_from(&mut from_host).unwrap(),
        Some(Frame::hello(1))
    );
    assert_eq!(
        Frame::read_from(&mut from_host).unwrap(),
        Some(Frame::Close {
            stream: 2,
            reason: CloseReason::Limit,
        })
    );
    assert_eq!(host.streams(), 1);
}

#[test]
fn a_peer_overrunning_its_window_ends_the_connection() {
    let host = Mux::host(MAX_STREAMS);
    let mut frames = vec![Frame::Open {
        stream: 1,
        endpoint: endpoint(ID),
    }];
    let chunks = WINDOW as usize / MAX_DATA + 1;
    frames.extend((0..chunks).map(|_| Frame::Data {
        stream: 1,
        bytes: vec![0; MAX_DATA],
    }));
    let bytes = encoded(&frames);
    assert!(matches!(
        host.receive_loop(bytes.as_slice()),
        Err(Error::Credit(1))
    ));
    assert!(host.is_ended());
}

#[test]
fn streams_open_in_order_only() {
    let host = Mux::host(MAX_STREAMS);
    let open = |stream| Frame::Open {
        stream,
        endpoint: endpoint(ID),
    };
    let bytes = encoded(&[open(2), open(2)]);
    assert!(matches!(
        host.receive_loop(bytes.as_slice()),
        Err(Error::Order(2))
    ));
    // Data for a stream never opened, or long closed, is dropped.
    let host = Mux::host(MAX_STREAMS);
    let stray = encoded(&[Frame::Data {
        stream: 7,
        bytes: vec![1],
    }]);
    assert!(host.receive_loop(stray.as_slice()).is_ok());
}

#[test]
fn a_writer_waits_for_credit_instead_of_buffering() {
    // A host that accepts but does not read yet.
    let (client, host) = connected(MAX_STREAMS);
    let stream = client.open(&endpoint(ID)).unwrap();
    let (done, finished) = mpsc::channel();
    let writing = stream.clone();
    thread::spawn(move || {
        let result = writing.clone().write_all(&vec![1; WINDOW as usize * 2]);
        let _ = done.send(result.is_ok());
    });
    assert!(finished.recv_timeout(Duration::from_millis(300)).is_err());
    let (_, accepted) = host.accept().unwrap();
    let mut drained = vec![0; WINDOW as usize * 2];
    accepted.clone().read_exact(&mut drained).unwrap();
    assert_eq!(finished.recv_timeout(Duration::from_secs(5)), Ok(true));
}

#[test]
fn an_unreachable_chat_reads_as_refused() {
    let (client, host) = connected(MAX_STREAMS);
    let stream = client.open(&endpoint(ID)).unwrap();
    let (_, accepted) = host.accept().unwrap();
    accepted.close_with(CloseReason::Unreachable);
    let error = stream.clone().read(&mut [0; 4]).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
}

#[test]
fn a_peer_that_never_greets_is_no_multiplexer() {
    let client = Mux::client();
    // An older program prints its usage error to stderr and exits.
    assert!(client.receive_loop(&b""[..]).is_ok());
    assert!(matches!(
        client.wait_hello(Duration::from_secs(1)),
        Err(Error::NotMultiplexer)
    ));
    let silent = Mux::client();
    assert!(matches!(
        silent.wait_hello(Duration::from_millis(10)),
        Err(Error::Timeout)
    ));
}

#[test]
fn ending_the_connection_ends_every_stream() {
    let (client, host) = echoing(MAX_STREAMS);
    let stream = client.open(&endpoint(ID)).unwrap();
    round_trip(&stream, b"up");
    client.shutdown();
    assert_eq!(stream.clone().read(&mut [0; 4]).unwrap(), 0);
    assert!(stream.clone().write(b"x").is_err());
    assert!(matches!(client.open(&endpoint(ID)), Err(Error::Ended)));
    drop(host);
}

#[test]
fn an_idle_connection_ends_itself() {
    let (client, _host) = echoing(MAX_STREAMS);
    let stream = client.open(&endpoint(ID)).unwrap();
    let waiting = client.clone();
    let (done, idle) = mpsc::channel();
    thread::spawn(move || {
        waiting.end_when_idle(Duration::from_millis(50));
        let _ = done.send(());
    });
    assert!(idle.recv_timeout(Duration::from_millis(200)).is_err());
    assert!(!client.is_ended());
    stream.close();
    assert!(idle.recv_timeout(Duration::from_secs(5)).is_ok());
    // Ended under the lock that saw it idle, so no stream slips in after.
    assert!(client.is_ended());
    assert!(matches!(client.open(&endpoint(ID)), Err(Error::Ended)));
}

#[test]
fn a_peer_that_stops_reading_cannot_grow_the_queue() {
    // A full host's every refusal is a queued reply; a client that keeps
    // opening and never reads would grow them without end.
    let host = Mux::host(1);
    let opens: Vec<Frame> = (1..=40_000)
        .map(|stream| Frame::Open {
            stream,
            endpoint: endpoint(ID),
        })
        .collect();
    let bytes = encoded(&opens);
    assert!(matches!(
        host.receive_loop(bytes.as_slice()),
        Err(Error::Backlog)
    ));
    assert!(host.is_ended());
}

#[test]
fn closed_streams_data_counts_against_the_queue() {
    // Nothing sends: whatever streams write stays queued.
    let host = Mux::host(1);
    let mut frames = Vec::new();
    for stream in 1..=2 {
        frames.push(Frame::Open {
            stream,
            endpoint: endpoint(ID),
        });
    }
    let bytes = encoded(&frames[..1]);
    let (mut input, mut feed) = io::pipe().unwrap();
    feed.write_all(&bytes).unwrap();
    let receiver = host.clone();
    thread::spawn(move || receiver.receive_loop(&mut input));
    let (_, first) = host.accept().unwrap();
    first.clone().write_all(&vec![1; WINDOW as usize]).unwrap();
    first.close();
    feed.write_all(&encoded(&frames[1..2])).unwrap();
    let (_, second) = host.accept().unwrap();
    // The first stream's window is still queued, so the second cannot
    // queue a full window more.
    assert!(second.clone().write_all(&vec![2; WINDOW as usize]).is_err());
    assert!(host.is_ended());
}
