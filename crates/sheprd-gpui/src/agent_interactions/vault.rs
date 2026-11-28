//! Secret-safe vault injection: writes a user-provided secret directly to a
//! workspace `.env` file (mode `0600`, exclusive temp file, no symlinks,
//! strictly inside the source workspace root) or to Sheprd's system credentials
//! store (`sheprd_credentials`) without ever returning or logging the secret
//! bytes to an agent or chat transcript.

use super::error::Error;
use secrecy::{ExposeSecret as _, SecretString};
use std::{
    ffi::CString,
    fmt, fs,
    io::Write as _,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// Secret bytes that never appear in `Debug` output and are zeroized on drop.
#[derive(Clone)]
pub(crate) struct SecretValue(SecretString);

impl SecretValue {
    pub(crate) fn new(value: String) -> Result<Self, Error> {
        if value.trim().is_empty() {
            return Err(Error::EmptySecret);
        }
        if value.len() > sheprd_credentials::LIMIT {
            return Err(Error::SecretTooLarge {
                limit: sheprd_credentials::LIMIT,
            });
        }
        Ok(Self(SecretString::from(value)))
    }

    pub(crate) fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    pub(crate) fn as_secret(&self) -> &SecretString {
        &self.0
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.0.expose_secret().len()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue(\"[REDACTED]\")")
    }
}

/// Validated destination for a secret injection request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Destination {
    EnvFile(PathBuf),
    Keychain(String),
}

/// Outcome returned to the chat / agent: contains only metadata, never bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InjectionReceipt {
    pub(crate) key: String,
    pub(crate) summary: String,
}

/// Validates an environment variable key (`[A-Za-z_][A-Za-z0-9_]*`, max 128).
pub(crate) fn validate_key(key: &str) -> Result<(), Error> {
    if key.is_empty() || key.len() > 128 {
        return Err(Error::InvalidKey);
    }
    let mut bytes = key.bytes();
    let Some(first) = bytes.next() else {
        return Err(Error::InvalidKey);
    };
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return Err(Error::InvalidKey);
    }
    if !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(Error::InvalidKey);
    }
    Ok(())
}

/// Parses `env-file:<relative>` or `keychain:<name>`.
pub(crate) fn parse_destination(raw: &str) -> Result<Destination, Error> {
    let trimmed = raw.trim();
    if let Some(rel) = trimmed.strip_prefix("env-file:") {
        let rel = rel.trim();
        if rel.is_empty() || rel.contains('\0') {
            return Err(Error::InvalidDestination);
        }
        if rel.starts_with('/') || rel.starts_with('\\') {
            return Err(Error::PathEscapesRoot);
        }
        let path = PathBuf::from(rel);
        if path.is_absolute()
            || path.components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(Error::PathEscapesRoot);
        }
        if !path.components().any(|c| matches!(c, Component::Normal(_))) {
            return Err(Error::InvalidDestination);
        }
        return Ok(Destination::EnvFile(path));
    }
    if let Some(name) = trimmed.strip_prefix("keychain:") {
        let name = name.trim();
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err(Error::InvalidDestination);
        }
        return Ok(Destination::Keychain(name.to_owned()));
    }
    Err(Error::InvalidDestination)
}

/// Injects `secret` for `key` into `destination`, confining `env-file:` writes
/// strictly to `workspace_root` and storing `keychain:` entries in Sheprd's
/// credential store (never inside the project workspace).
/// Must be called on a background thread (performs file/keyring I/O).
pub(crate) fn inject(
    workspace_root: Option<&Path>,
    key: &str,
    destination: &Destination,
    secret: &SecretValue,
) -> Result<InjectionReceipt, Error> {
    let creds_dir = default_credentials_dir();
    let store = sheprd_credentials::Store::for_policy(false);
    inject_with_store(workspace_root, &creds_dir, store, key, destination, secret)
}

/// Testable entry point accepting an explicit credentials directory and store.
pub(crate) fn inject_with_store(
    workspace_root: Option<&Path>,
    credentials_dir: &Path,
    store: sheprd_credentials::Store,
    key: &str,
    destination: &Destination,
    secret: &SecretValue,
) -> Result<InjectionReceipt, Error> {
    validate_key(key)?;
    match destination {
        Destination::EnvFile(relative) => {
            let root = workspace_root.ok_or(Error::WorkspaceNotFound)?;
            let target = resolve_safe_path(root, relative)?;
            write_env_entry(&target, key, secret.expose())?;
            Ok(InjectionReceipt {
                key: key.to_owned(),
                summary: format!(
                    "Injected {key} ({} bytes) into {} (mode 0600; redacted from chat)",
                    secret.byte_len(),
                    relative.display()
                ),
            })
        }
        Destination::Keychain(service) => {
            save_keychain_secret(credentials_dir, store, service, key, secret)?;
            let backend = match store {
                sheprd_credentials::Store::Keyring => "system keychain",
                sheprd_credentials::Store::File => "Sheprd private credentials store",
                sheprd_credentials::Store::Environment => "credentials store",
            };
            Ok(InjectionReceipt {
                key: key.to_owned(),
                summary: format!(
                    "Saved {key} ({} bytes) to {backend} '{service}' (redacted from chat)",
                    secret.byte_len()
                ),
            })
        }
    }
}

fn default_credentials_dir() -> PathBuf {
    let config = crate::config::daemon_config_path(sheprd_client::local_var);
    config
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
        .join("agent-vault")
}

fn validate_secret_len(value: &SecretString) -> Result<(), Error> {
    let bytes = value.expose_secret().len();
    if bytes == 0 {
        return Err(Error::EmptySecret);
    }
    if bytes > sheprd_credentials::LIMIT {
        return Err(Error::SecretTooLarge {
            limit: sheprd_credentials::LIMIT,
        });
    }
    Ok(())
}

fn save_keychain_secret(
    credentials_dir: &Path,
    store: sheprd_credentials::Store,
    service: &str,
    key: &str,
    secret: &SecretValue,
) -> Result<(), Error> {
    match store {
        sheprd_credentials::Store::Keyring => {
            let keyring_service = format!("sheprd.agent.{service}");
            let label = format!("Sheprd agent secret ({service}/{key})");
            sheprd_credentials::keyring_save(
                &keyring_service,
                &label,
                Some(secret.as_secret()),
                key,
            )?;
            Ok(())
        }
        sheprd_credentials::Store::File => {
            ensure_private_dir(credentials_dir)?;
            let name = CString::new(format!("agent-{service}-{key}"))
                .map_err(|_| Error::InvalidDestination)?;
            sheprd_credentials::file::store_checked(
                credentials_dir,
                &name,
                Some(secret.as_secret()),
                true,
                validate_secret_len,
            )?;
            Ok(())
        }
        sheprd_credentials::Store::Environment => {
            Err(Error::Credentials(sheprd_credentials::Error::Unsupported))
        }
    }
}

fn ensure_private_dir(dir: &Path) -> Result<(), Error> {
    if let Ok(meta) = fs::symlink_metadata(dir)
        && meta.file_type().is_symlink()
    {
        return Err(Error::SymlinkRefused {
            path: dir.to_path_buf(),
        });
    }
    fs::create_dir_all(dir).map_err(|source| Error::Write {
        path: dir.to_path_buf(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|source| {
            Error::Write {
                path: dir.to_path_buf(),
                source,
            }
        })?;
    }
    Ok(())
}

fn resolve_safe_path(root: &Path, relative: &Path) -> Result<PathBuf, Error> {
    if let Ok(meta) = fs::symlink_metadata(root)
        && meta.file_type().is_symlink()
    {
        return Err(Error::SymlinkRefused {
            path: root.to_path_buf(),
        });
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(part) => {
                current.push(part);
                if let Ok(meta) = fs::symlink_metadata(&current)
                    && meta.file_type().is_symlink()
                {
                    return Err(Error::SymlinkRefused { path: current });
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::PathEscapesRoot);
            }
        }
    }
    Ok(current)
}

fn write_env_entry(path: &Path, key: &str, value: &str) -> Result<(), Error> {
    let parent = path.parent().ok_or(Error::InvalidDestination)?;
    fs::create_dir_all(parent).map_err(|source| Error::Write {
        path: parent.to_path_buf(),
        source,
    })?;

    if let Ok(meta) = fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        return Err(Error::SymlinkRefused {
            path: path.to_path_buf(),
        });
    }

    let existing = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(source) => {
            return Err(Error::Write {
                path: path.to_path_buf(),
                source,
            });
        }
    };

    let prefix = format!("{key}=");
    let escaped = format_env_value(value);
    let replacement = format!("{prefix}{escaped}");
    let mut lines: Vec<String> = existing.lines().map(ToOwned::to_owned).collect();
    if let Some(slot) = lines.iter_mut().find(|line| line.starts_with(&prefix)) {
        *slot = replacement;
    } else {
        lines.push(replacement);
    }
    let mut out = lines.join("\n");
    out.push('\n');

    let (tmp_path, mut file) = create_exclusive_private_temp(parent)?;
    let write_res = (|| -> Result<(), Error> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|source| Error::Write {
                    path: tmp_path.clone(),
                    source,
                })?;
        }
        file.write_all(out.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|source| Error::Write {
                path: tmp_path.clone(),
                source,
            })?;
        drop(file);
        if let Ok(meta) = fs::symlink_metadata(path)
            && meta.file_type().is_symlink()
        {
            return Err(Error::SymlinkRefused {
                path: path.to_path_buf(),
            });
        }
        fs::rename(&tmp_path, path).map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })
    })();
    if write_res.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    write_res
}

/// Creates a brand-new private (`0600`) temporary file inside `parent` using
/// `create_new(true)` (`O_CREAT | O_EXCL`), preventing symlink swaps and
/// pre-existing mode inheritance.
fn create_exclusive_private_temp(parent: &Path) -> Result<(PathBuf, fs::File), Error> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());

    for _ in 0..16 {
        let seq = NEXT.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".sheprd-vault-{pid}-{nanos}-{seq}.tmp"));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        match options.open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(Error::Write {
                    path: candidate,
                    source,
                });
            }
        }
    }
    Err(Error::Write {
        path: parent.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not allocate unique temporary file",
        ),
    })
}

fn format_env_value(value: &str) -> String {
    let needs_quotes = value.is_empty()
        || value
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '#' | '"' | '\'' | '\\' | '$' | '`'));
    if !needs_quotes {
        return value.to_owned();
    }
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}
