//! Blocking speech requests and audio playback, run by a background worker.
mod stream;
pub(crate) use stream::{Kind, Session};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
pub(crate) use sheprd_remote_control::control::voice::{Account, Provider};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("Set up a speaking voice in Settings → Voice.")]
    Disconnected,
    #[error("Playback stopped.")]
    Stopped,
    #[error("Check the API key and voice ID. Speech requests support up to 8,000 UTF-8 bytes.")]
    Invalid,
    #[error("Could not complete the voice request: {0}")]
    Http(#[from] ureq::Error),
    #[error("The provider returned an invalid response.")]
    Json(#[from] serde_json::Error),
    #[error("Speech download interrupted.")]
    Stream(#[source] std::io::Error),
    #[error("Could not decode the speech audio.")]
    Audio(#[from] rodio::decoder::DecoderError),
    #[error("Could not open the audio output.")]
    Device(#[from] rodio::DeviceSinkError),
    #[error("Could not start secure sign-in.")]
    Random(#[from] getrandom::Error),
    #[error(transparent)]
    Account(#[from] sheprd_remote_control::app::voice::Error),
    #[error(transparent)]
    Store(#[from] sheprd_remote_control::app::Error),
}

fn agent() -> ureq::Agent {
    http_agent(30)
}

fn http_agent(seconds: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .max_redirects(0)
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_global(Some(Duration::from_secs(seconds)))
        .build()
        .into()
}

pub(crate) fn accounts() -> Result<sheprd_remote_control::app::voice::Accounts, Error> {
    Ok(sheprd_remote_control::app::voice::Accounts::new(
        crate::remote::state_dir()?,
    ))
}

pub(crate) fn load() -> Result<Option<Account>, Error> {
    Ok(accounts()?.load()?)
}

pub(crate) fn save(account: Option<&Account>) -> Result<(), Error> {
    Ok(accounts()?.save(account, false)?)
}

pub(crate) const SAMPLE: &str = "Two chats need you. Would you like to open the first?";

pub(crate) fn read_message(words: &str, session: &Session) -> Result<(), Error> {
    let account = load()?.ok_or(Error::Disconnected)?;
    speak(&account, words, session)
}

pub(crate) fn speak(account: &Account, words: &str, session: &Session) -> Result<(), Error> {
    if session.cancelled() {
        return Ok(());
    }
    if !account.valid() || words.is_empty() || words.len() > 8000 {
        return Err(Error::Invalid);
    }
    let voice = &account.voice;
    let (url, body) = match account.provider {
        Provider::ElevenLabs => (
            format!("https://api.elevenlabs.io/v1/text-to-speech/{voice}/stream"),
            json!({"text":words,"model_id":"eleven_multilingual_v2"}),
        ),
        Provider::OpenAI => (
            "https://api.openai.com/v1/audio/speech".into(),
            json!({"input":words,"voice":voice,"model":"gpt-4o-mini-tts","response_format":"mp3"}),
        ),
        Provider::OpenRouter => (
            "https://openrouter.ai/api/v1/audio/speech".into(),
            json!({"input":words,"voice":voice,"model":"mistralai/voxtral-mini-tts-2603","response_format":"mp3"}),
        ),
        Provider::Deepgram => (
            format!("https://api.deepgram.com/v1/speak?encoding=mp3&model={voice}"),
            json!({"text":words}),
        ),
    };
    let request = session
        .agent()
        .post(&url)
        .header("Content-Type", "application/json");
    let key = account.key.expose();
    let request = match account.provider {
        Provider::ElevenLabs => request.header("xi-api-key", key),
        Provider::Deepgram => request.header("Authorization", format!("Token {key}")),
        _ => request.header("Authorization", format!("Bearer {key}")),
    };
    let response = request.send(serde_json::to_vec(&body)?.as_slice())?;
    stream::play(response.into_body(), session)
}

#[derive(Deserialize)]
pub(crate) struct Choice {
    pub voice_id: String,
    pub name: String,
}

pub(crate) fn choices(provider: Provider) -> Vec<Choice> {
    let ids: &[&str] = match provider {
        Provider::OpenAI => &["marin", "cedar", "coral", "alloy", "nova", "sage"],
        Provider::Deepgram => &["aura-2-thalia-en", "aura-2-apollo-en"],
        _ => &[provider.default_voice()],
    };
    ids.iter()
        .map(|id| Choice {
            name: id.to_string(),
            voice_id: id.to_string(),
        })
        .collect()
}

pub(crate) fn elevenlabs_voices(
    key: &sheprd_remote_control::control::github::Token,
) -> Result<Vec<Choice>, Error> {
    #[derive(Deserialize)]
    struct Page {
        voices: Vec<Choice>,
    }
    if !key.valid() {
        return Err(Error::Invalid);
    }
    let bytes = agent()
        .get("https://api.elevenlabs.io/v2/voices?page_size=100")
        .header("xi-api-key", key.expose())
        .call()?
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_vec()?;
    Ok(serde_json::from_slice::<Page>(&bytes)?
        .voices
        .into_iter()
        .take(100)
        .collect())
}

/// OpenRouter's no-callback PKCE grant for native clients. Only the challenge
/// leaves for the browser; the user returns with the short-lived code.
pub(crate) fn sign_in() -> Result<(SecretString, String), Error> {
    let mut random = [0; 32];
    getrandom::fill(&mut random)?;
    let verifier = SecretString::from(URL_SAFE_NO_PAD.encode(random));
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.expose_secret().as_bytes()));
    Ok((
        verifier,
        format!(
            "https://openrouter.ai/auth?code_challenge_method=S256&key_label=Sheprd&code_challenge={challenge}"
        ),
    ))
}

pub(crate) fn exchange(
    verifier: &SecretString,
    code: &str,
) -> Result<sheprd_remote_control::control::github::Token, Error> {
    #[derive(Deserialize)]
    struct Answer {
        key: sheprd_remote_control::control::github::Token,
    }
    let body = json!({"code": code, "code_verifier": verifier.expose_secret(), "code_challenge_method": "S256"});
    let bytes = agent()
        .post("https://openrouter.ai/api/v1/auth/keys")
        .header("Content-Type", "application/json")
        .send(serde_json::to_vec(&body)?.as_slice())?
        .body_mut()
        .with_config()
        .limit(32768)
        .read_to_vec()?;
    let answer: Answer = serde_json::from_slice(&bytes)?;
    Ok(answer.key)
}
