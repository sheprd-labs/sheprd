//! A relay for other projects' tests: the real WebSocket relay on a
//! loopback port, with no dashboard, QUIC, metrics port, or logging set
//! up. It runs on a runtime of its own, so [`TestRelay::stop`] ends every
//! socket it holds at once, as a relay's restart does, and it works from
//! any test, async or not.
//!
//! ```no_run
//! # fn main() -> sheprd_relay::Result<()> {
//! let relay = sheprd_relay::testing::TestRelay::start()?;
//! let endpoint = relay.endpoint(); // http://127.0.0.1:PORT
//! // … listen and dial through `endpoint` …
//! relay.stop();
//! # Ok(())
//! # }
//! ```

use crate::{
    Endpoint, Error, Limits, Result,
    hub::{Access, Hub},
    web::{self, Shared, Source},
};
use std::{
    net::{SocketAddr, TcpListener as StdListener},
    sync::Arc,
    thread::JoinHandle,
    time::Duration,
};
use tokio::sync::oneshot;

/// Bounds that suit tests: quick timeouts, room for many dials from one
/// address, no bandwidth limits, and no automatic bans.
#[must_use]
pub fn limits() -> Limits {
    Limits {
        accept_timeout: Duration::from_secs(5),
        auto_ban: false,
        connections_per_minute: 6_000,
        dials_per_minute: 6_000,
        idle_timeout: Duration::from_secs(60),
        listener_silence: Duration::from_secs(30),
        listens_per_minute: 6_000,
        max_connections: 256,
        max_connections_per_ip: 256,
        max_listeners: 64,
        max_listeners_per_ip: 64,
        max_sessions: 128,
        max_sessions_per_ip: 128,
        max_sessions_per_key: 16,
        max_sessions_per_route: 32,
        max_transfer_per_hour: 0,
        session_bandwidth: 0,
        total_bandwidth: 0,
        ..Limits::default()
    }
}

/// How to start a test relay.
#[derive(Clone, Debug)]
pub struct Config {
    /// Where to listen: `127.0.0.1:0` picks a free port. Pass a stopped
    /// relay's [`TestRelay::address`] to start it again where clients
    /// expect it.
    pub address: SocketAddr,
    pub limits: Limits,
    /// Listen tokens, one per line, as `--listen-token-file` takes them;
    /// `None` lets anyone listen.
    pub listen_tokens: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            address: SocketAddr::from(([127, 0, 0, 1], 0)),
            limits: limits(),
            listen_tokens: None,
        }
    }
}

/// A running test relay. Dropping it stops it, as [`TestRelay::stop`]
/// does without waiting.
#[derive(Debug)]
pub struct TestRelay {
    address: SocketAddr,
    endpoint: Endpoint,
    hub: Arc<Hub>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl TestRelay {
    /// A relay with [`limits`] on a free loopback port.
    pub fn start() -> Result<Self> {
        Self::start_with(Config::default())
    }

    /// A relay as `config` says. Its public name, which WebSocket proofs
    /// cover, is the address it bound.
    pub fn start_with(config: Config) -> Result<Self> {
        let access = match &config.listen_tokens {
            Some(tokens) => Access::tokens(tokens)?,
            None => Access::Open,
        };
        let listener = StdListener::bind(config.address).map_err(|source| Error::Bind {
            address: config.address,
            source,
        })?;
        let address = listener.local_addr().map_err(|source| Error::Bind {
            address: config.address,
            source,
        })?;
        listener.set_nonblocking(true).map_err(Error::Runtime)?;
        let endpoint = Endpoint::parse(&format!("http://{address}"))?;
        let hub = Arc::new(Hub::new(config.limits, access));
        let shared = Shared {
            hub: Arc::clone(&hub),
            public_host: Some(Arc::from(address.to_string())),
            quic: None,
            // The viewer needs QUIC, which a test relay does not run.
            share_viewer: false,
            source: Source::Peer,
        };
        let router = web::router(shared.clone()).layer(axum::middleware::from_fn_with_state(
            shared,
            web::refuse_banned,
        ));
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(Error::Runtime)?;
        let (stop, stopped) = oneshot::channel::<()>();
        let thread = std::thread::Builder::new()
            .name(format!("test relay {address}"))
            .spawn(move || {
                runtime.block_on(async move {
                    let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                        return;
                    };
                    let app = router.into_make_service_with_connect_info::<SocketAddr>();
                    tokio::select! {
                        _ = axum::serve(listener, app) => {}
                        _ = stopped => {}
                    }
                });
                // Every socket's task ends with the runtime.
                runtime.shutdown_timeout(Duration::from_secs(1));
            })
            .map_err(Error::Runtime)?;
        Ok(Self {
            address,
            endpoint,
            hub,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    /// The loopback address it listens on.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    /// Its base URL, `http://127.0.0.1:PORT`.
    #[must_use]
    pub fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// Its endpoint, as clients take it.
    #[must_use]
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// What it holds: listeners, sessions, and counters, for assertions.
    #[must_use]
    pub fn hub(&self) -> &Arc<Hub> {
        &self.hub
    }

    /// Stops it and waits until every socket it held is closed.
    pub fn stop(mut self) {
        self.shut_down(true);
    }

    fn shut_down(&mut self, wait: bool) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let (true, Some(thread)) = (wait, self.thread.take()) {
            let _ = thread.join();
        }
    }
}

impl Drop for TestRelay {
    fn drop(&mut self) {
        self.shut_down(false);
    }
}
