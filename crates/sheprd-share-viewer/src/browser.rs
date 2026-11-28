use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use sheprd_chat_protocol::{
    Cursor, Transcript,
    sharing::{
        MAX_RESPONSE_BYTES, Read, Update, VERSION,
        admission::{self, Envelope, MAX_LINK_BYTES, VIEWER_PATH},
    },
};
use sheprd_relay::{
    Endpoint, RouteId,
    socket::{self, Hello, ToPeer},
};
use sheprd_tunnel::{Identity, Initiator, Keys, Transport, frame, record::Opened};
use wasm_bindgen::prelude::*;
use web_time::Instant;

type Result<T> = std::result::Result<T, JsError>;

fn invalid(_: impl std::fmt::Debug) -> JsError {
    JsError::new("invalid")
}

fn signature(key: &SigningKey, bytes: &[u8]) -> [u8; 64] {
    let signed: Signature = key.sign(bytes);
    signed.to_bytes().into()
}

#[wasm_bindgen]
pub struct Guest {
    envelope: Envelope,
    relay: Endpoint,
    route: RouteId,
    host: Identity,
    keys: Keys,
    viewer: SigningKey,
    bootstrap: SigningKey,
    capability: [u8; 32],
    handshake: Option<Initiator>,
    transport: Option<Transport>,
    cursor: Option<Cursor>,
    transcript: Transcript,
}

#[wasm_bindgen]
impl Guest {
    #[wasm_bindgen(constructor)]
    pub fn new(landing: &str, fragment: &str) -> Result<Guest> {
        if landing.len() + fragment.len() + 1 > MAX_LINK_BYTES {
            return Err(invalid(()));
        }
        let base = landing
            .strip_suffix(VIEWER_PATH)
            .ok_or_else(|| invalid(()))?;
        let relay = Endpoint::parse(base).map_err(invalid)?;
        let env: Envelope =
            serde_json::from_slice(&B64.decode(fragment).map_err(invalid)?).map_err(invalid)?;
        if !relay.is_secure()
            || relay.to_string() != base
            || env.relay != base
            || env.share.0.is_nil()
            || env.grant.0.is_nil()
        {
            return Err(invalid(()));
        }
        if env.version != VERSION {
            return Err(JsError::new("version"));
        }
        let mut seed = [0; 32];
        getrandom::fill(&mut seed).map_err(invalid)?;
        Ok(Self {
            relay,
            route: RouteId::parse(&env.route).ok_or_else(|| invalid(()))?,
            host: Identity::from_bytes(&B64.decode(&env.identity).map_err(invalid)?)
                .map_err(invalid)?,
            bootstrap: SigningKey::from_slice(&B64.decode(&env.bootstrap).map_err(invalid)?)
                .map_err(invalid)?,
            capability: B64
                .decode(&env.capability)
                .map_err(invalid)?
                .try_into()
                .map_err(invalid)?,
            keys: Keys::generate().map_err(invalid)?,
            viewer: SigningKey::from_slice(&seed).map_err(invalid)?,
            envelope: env,
            handshake: None,
            transport: None,
            cursor: None,
            transcript: Transcript::default(),
        })
    }

    pub fn expiry(&self) -> u64 {
        self.envelope.expires_ms
    }

    pub fn url(&self) -> String {
        self.relay.dial(&self.route).to_string()
    }

    pub fn hello(&self, challenge: &str, enrollment: bool) -> Result<String> {
        let ToPeer::Challenge { nonce, version } =
            serde_json::from_str(challenge).map_err(invalid)?
        else {
            return Err(invalid(()));
        };
        if version != sheprd_relay::endpoint::PROTOCOL {
            return Err(JsError::new("version"));
        }
        let nonce = B64
            .decode(nonce)
            .map_err(invalid)?
            .try_into()
            .map_err(invalid)?;
        let key = if enrollment {
            &self.bootstrap
        } else {
            &self.viewer
        };
        let bytes = socket::dial_signed(&self.relay.proof_name(), &nonce, &self.route);
        serde_json::to_string(&Hello {
            key: B64.encode(key.verifying_key().to_sec1_point(true).as_bytes()),
            signature: B64.encode(signature(key, &bytes)),
            token: None,
        })
        .map_err(invalid)
    }

    pub fn begin(&mut self) -> Result<Vec<u8>> {
        self.transport = None;
        let (handshake, first) =
            Initiator::session(&self.keys, &self.host, self.route.as_bytes()).map_err(invalid)?;
        self.handshake = Some(handshake);
        frame::encode(&first).map_err(invalid)
    }

    pub fn admit(&mut self, answer: &[u8], enrollment: bool) -> Result<Vec<u8>> {
        let (transport, hash) = self
            .handshake
            .take()
            .ok_or_else(|| invalid(()))?
            .finish(answer)
            .map_err(invalid)?;
        self.transport = Some(transport);
        let env = &self.envelope;
        let sign = |msg: &[u8]| signature(&self.viewer, msg);
        let body = if enrollment {
            admission::join(
                env.share,
                env.grant,
                &self.capability,
                self.viewer.verifying_key().to_sec1_point(true).as_bytes(),
                sign,
                &hash,
            )
        } else {
            admission::resume(env.share, env.grant, sign, &hash)
        }
        .map_err(invalid)?;
        self.request(&body)
    }

    pub fn read(&mut self) -> Result<Vec<u8>> {
        self.request(
            &serde_json::to_vec(&Read {
                version: VERSION,
                cursor: self.cursor,
            })
            .map_err(invalid)?,
        )
    }

    pub fn open(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        match self
            .transport
            .as_mut()
            .ok_or_else(|| invalid(()))?
            .open(bytes)
            .map_err(invalid)?
        {
            Opened::Data(data) => Ok(data),
            Opened::Closed => Err(JsError::new("closed")),
        }
    }

    pub fn update(&mut self, bytes: &[u8]) -> Result<Option<String>> {
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(invalid(()));
        }
        self.cursor = Some(match serde_json::from_slice(bytes).map_err(invalid)? {
            Update::Snapshot { cursor, transcript } => {
                self.transcript = *transcript;
                cursor
            }
            Update::Changes { cursor, changes } => {
                for change in changes {
                    self.transcript.apply(&change).map_err(invalid)?;
                }
                cursor
            }
            Update::Unchanged { .. } => return Ok(None),
        });
        let json = serde_json::to_string(&self.transcript).map_err(invalid)?;
        if json.len() > MAX_RESPONSE_BYTES {
            return Err(invalid(()));
        }
        Ok(Some(json))
    }
}

impl Guest {
    fn request(&mut self, body: &[u8]) -> Result<Vec<u8>> {
        let len = u32::try_from(body.len()).map_err(invalid)?;
        let plain = [&len.to_be_bytes()[..], body].concat();
        let record = self
            .transport
            .as_mut()
            .ok_or_else(|| invalid(()))?
            .seal(&plain, Instant::now())
            .map_err(invalid)?;
        frame::encode(&record).map_err(invalid)
    }
}
