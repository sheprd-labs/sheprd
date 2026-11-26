//! Reaches a relay over QUIC as a computer would: `probe ENDPOINT [PIN]`.
//! Reads its `/v1/info`, connects on UDP, and prints the round trip. With
//! PIN, a relay reporting another pin is refused, as the daemon refuses
//! one saved from Sheprd's signed list. `just relays-check` runs it on
//! each new public relay.

use sheprd_relay::{Endpoint, client, endpoint};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(text), pin, None) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: probe ENDPOINT [PIN]");
        return ExitCode::from(2);
    };
    let endpoint = match Endpoint::parse(&text) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            eprintln!("{text}: {error}");
            return ExitCode::from(2);
        }
    };
    let expected = match pin.as_deref().map(endpoint::decode_pin) {
        None => None,
        Some(Some(pin)) => Some(pin),
        Some(None) => {
            eprintln!("not a QUIC pin: {}", pin.unwrap_or_default());
            return ExitCode::from(2);
        }
    };
    match client::reach_pinned(&endpoint, expected).await {
        Ok(connection) => {
            println!(
                "{endpoint}: QUIC ok, round trip {} ms",
                connection.rtt().as_millis()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{endpoint}: {error}");
            ExitCode::FAILURE
        }
    }
}
