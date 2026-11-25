//! Active speech account explicitly connected in Voice settings.
use crate::control::voice::{Account, Answer, Offer};
use secrecy::{ExposeSecret, SecretString};
use sheprd_credentials::{Entry, Store};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Check the voice ID and API key.")]
    Invalid,
    #[error("Could not access the voice credential store.")]
    Store(#[from] sheprd_credentials::Error),
}

fn decode(value: &SecretString) -> Result<Account, Error> {
    let account: Account =
        serde_json::from_str(value.expose_secret()).map_err(|_| Error::Invalid)?;
    account.valid().then_some(account).ok_or(Error::Invalid)
}

const ENTRY: Entry<Error> = Entry {
    service: "org.sheprd.voice",
    account: "active",
    label: "Sheprd speaking voice",
    file: c"voice-account.json",
    validate: |value| decode(value).map(drop),
};

/// The desktop owns this keyring entry. Imports and settings edits share a lock.
pub struct Accounts {
    directory: PathBuf,
    store: Store,
}

impl Accounts {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            store: Store::for_policy(false),
        }
    }

    #[cfg(all(test, unix, feature = "server"))]
    pub(crate) fn isolated(directory: PathBuf) -> Self {
        Self {
            directory,
            store: Store::File,
        }
    }

    pub fn load(&self) -> Result<Option<Account>, Error> {
        if self.store == Store::File && !self.directory.exists() {
            return Ok(None);
        }
        sheprd_credentials::read_entry(self.store, &ENTRY, &self.directory)?
            .as_ref()
            .map(decode)
            .transpose()
    }

    pub fn exchange(&self, offer: Offer) -> super::Result<Answer> {
        if !offer.valid() {
            return Err(Error::Invalid.into());
        }
        if let Some(account) = offer.account {
            self.save(Some(&account), true)?;
            return Ok(Answer::default());
        }
        let account = self.load()?;
        if offer.has_account {
            Ok(Answer {
                wants_account: account.is_none(),
                account: None,
            })
        } else {
            Ok(Answer {
                account,
                wants_account: false,
            })
        }
    }

    pub fn save(&self, account: Option<&Account>, only_if_empty: bool) -> super::Result<()> {
        let dir = &self.directory;
        std::fs::create_dir_all(dir).map_err(|source| super::Error::Io {
            operation: "create",
            path: dir.to_owned(),
            source,
        })?;
        super::edited::locked(&dir.join("voice-account.json"), || {
            if only_if_empty && self.load()?.is_some() {
                return Ok(());
            }
            let secret = account
                .map(serde_json::to_string)
                .transpose()
                .map_err(|_| Error::Invalid)?
                .map(SecretString::from);
            sheprd_credentials::save_entry(self.store, &ENTRY, dir, secret.as_ref())?;
            Ok(())
        })
    }
}
