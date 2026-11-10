use crate::{
    BitwardenError as Error, BitwardenField as Field, SecretFieldValue, VaultInfo,
    VaultItemSummary, VaultReference, VaultStatus,
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, Instant},
};
use totp_rs::{Algorithm, Secret, TOTP};
use zeroize::Zeroizing;

const MAX_OUTPUT: u64 = 16 * 1024 * 1024;

struct Cli {
    program: PathBuf,
    script: Option<PathBuf>,
}
trait CliRunner: Send + Sync {
    fn run(
        &self,
        profile: &Path,
        args: &[&str],
        env: &[(&str, &str)],
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<Zeroizing<Vec<u8>>, Error>;
}
impl Cli {
    fn discover() -> Option<Self> {
        let paths = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&paths) {
            let native = dir.join(if cfg!(windows) { "bw.exe" } else { "bw" });
            if native.is_file() {
                return Some(Self {
                    program: native,
                    script: None,
                });
            }
            let node = dir.join(if cfg!(windows) { "node.exe" } else { "node" });
            let script = dir.join("node_modules/@bitwarden/cli/build/bw.js");
            if node.is_file() && script.is_file() {
                return Some(Self {
                    program: node,
                    script: Some(script),
                });
            }
        }
        None
    }
}
impl CliRunner for Cli {
    fn run(
        &self,
        profile: &Path,
        args: &[&str],
        env: &[(&str, &str)],
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<Zeroizing<Vec<u8>>, Error> {
        if is_cancelled() {
            return Err(Error::VaultLocked);
        }
        #[cfg(windows)]
        let working = crate::profile::WorkingProfile::open(profile)?;
        #[cfg(windows)]
        let profile = working.directory();
        #[cfg(not(windows))]
        std::fs::create_dir_all(profile).map_err(|_| Error::CommandFailed)?;
        let mut command = Command::new(&self.program);
        if let Some(script) = &self.script {
            command.arg(script);
        }
        command
            .args(args)
            .args(["--nointeraction", "--raw"])
            .env("BITWARDENCLI_APPDATA_DIR", profile)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for key in [
            "BW_SESSION",
            "BW_PASSWORD",
            "BW_CLIENTID",
            "BW_CLIENTSECRET",
            "CHALLA_BW_PASSWORD",
            "NODE_OPTIONS",
        ] {
            command.env_remove(key);
        }
        for (key, value) in env {
            command.env(key, value);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|_| Error::CliUnavailable)?;
        let stdout = child.stdout.take().ok_or(Error::CommandFailed)?;
        let reader = std::thread::spawn(move || {
            let mut bytes = Zeroizing::new(Vec::new());
            stdout
                .take(MAX_OUTPUT + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        let stderr = child.stderr.take().ok_or(Error::CommandFailed)?;
        let error_reader = std::thread::spawn(move || {
            let mut bytes = Zeroizing::new(Vec::new());
            stderr
                .take(64 * 1024)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        let started = Instant::now();
        let result = loop {
            if is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                break Err(Error::VaultLocked);
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    break if status.success() {
                        Ok(())
                    } else {
                        Err(Error::CommandFailed)
                    }
                }
                Ok(None) if started.elapsed() < Duration::from_secs(60) => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(Error::Timeout);
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(Error::CommandFailed);
                }
            }
        };
        let bytes = reader
            .join()
            .map_err(|_| Error::InvalidResponse)?
            .map_err(|_| Error::InvalidResponse)?;
        let diagnostic = error_reader
            .join()
            .map_err(|_| Error::InvalidResponse)?
            .map_err(|_| Error::InvalidResponse)?;
        #[cfg(windows)]
        if result.is_ok() {
            working.commit()?;
        }
        match result {
            Err(Error::CommandFailed) => {
                return Err(classify_cli_failure(args, &bytes, &diagnostic))
            }
            Err(error) => return Err(error),
            Ok(()) => {}
        }
        if bytes.len() as u64 > MAX_OUTPUT {
            return Err(Error::InvalidResponse);
        }
        Ok(bytes)
    }
}

// Whitelist fixed categories only. Upstream text may contain tokens, account
// details or URLs and must never reach tracing, IPC, UI or persisted logs.
fn classify_cli_failure(args: &[&str], stdout: &[u8], stderr: &[u8]) -> Error {
    let output = Zeroizing::new(
        std::str::from_utf8(stdout)
            .unwrap_or("")
            .to_ascii_lowercase(),
    );
    let diagnostic = Zeroizing::new(
        std::str::from_utf8(stderr)
            .unwrap_or("")
            .to_ascii_lowercase(),
    );
    let has = |patterns: &[&str]| {
        patterns
            .iter()
            .any(|p| output.contains(p) || diagnostic.contains(p))
    };
    if has(&[
        "certificate",
        "unable_to_verify_leaf_signature",
        "self_signed_cert",
        "cert_has_expired",
        "err_tls_cert_altname_invalid",
    ]) {
        return Error::Tls;
    }
    if has(&[
        "enotfound",
        "eai_again",
        "econnrefused",
        "econnreset",
        "etimedout",
        "fetch failed",
        "network request failed",
    ]) {
        return Error::Connection;
    }
    if has(&[
        "logout required",
        "already logged in",
        "already authenticated",
    ]) {
        return Error::AlreadyAuthenticated;
    }
    if has(&[
        "invalid_client",
        "invalid client",
        "invalid api key",
        "username or password is incorrect",
        "incorrect client",
        "invalid credentials",
    ]) {
        return Error::AuthenticationRejected;
    }
    match args.first().copied() {
        Some("config") => Error::ServerConfiguration,
        Some("login") => Error::AuthenticationFailed,
        _ => Error::CommandFailed,
    }
}

enum Cached {
    Text(Zeroizing<String>),
    Totp(TOTP),
}
struct Session {
    key: Zeroizing<String>,
    fields: HashMap<VaultReference, Cached>,
    summaries: Option<Vec<VaultItemSummary>>,
}
struct State {
    status: VaultStatus,
    session: Option<Session>,
    system_locked: bool,
    suspended_key: Option<Zeroizing<Vec<u8>>>,
}
pub struct BitwardenClient {
    cli: Option<Arc<dyn CliRunner>>,
    profile: PathBuf,
    operation: Mutex<()>,
    state: RwLock<State>,
    generation: AtomicU64,
}

#[cfg(all(test, windows))]
mod session_lock_tests {
    use super::*;

    fn unlocked_client() -> BitwardenClient {
        let client =
            BitwardenClient::with_profile(std::env::temp_dir().join("challa-session-test"));
        {
            let mut state = client.state.write().unwrap();
            state.status = VaultStatus::Unlocked;
            state.session = Some(Session {
                key: Zeroizing::new("fixture-resume-key".into()),
                fields: HashMap::new(),
                summaries: Some(Vec::new()),
            });
        }
        client
    }

    #[test]
    fn system_lock_seals_key_and_resume_invalidates_old_leases() {
        let client = unlocked_client();
        let before = client.generation();
        client.suspend_for_system_lock().unwrap();
        assert_eq!(client.status(), VaultStatus::Locked);
        assert!(!client.is_current(before));
        assert!(client.lease().is_err());
        {
            let state = client.state.read().unwrap();
            assert!(state.session.is_none());
            let sealed = state.suspended_key.as_ref().unwrap();
            assert!(!sealed
                .windows(b"fixture-resume-key".len())
                .any(|w| w == b"fixture-resume-key"));
        }
        let suspended = client.generation();
        client.suspend_for_system_lock().unwrap();
        assert_eq!(client.generation(), suspended);
        assert!(client.resume_after_system_unlock().unwrap());
        assert_eq!(client.status(), VaultStatus::Unlocked);
        assert!(!client.is_current(before));
        assert!(!client.is_current(suspended));
        let state = client.state.read().unwrap();
        let session = state.session.as_ref().unwrap();
        assert_eq!(&*session.key, "fixture-resume-key");
        assert!(session.fields.is_empty());
        assert!(session.summaries.is_none());
        assert!(state.suspended_key.is_none());
        drop(state);
        assert!(!client.resume_after_system_unlock().unwrap());
    }

    #[test]
    fn manual_lock_before_or_during_system_lock_stays_locked() {
        for lock_before in [true, false] {
            let client = unlocked_client();
            if lock_before {
                client.lock();
            }
            client.suspend_for_system_lock().unwrap();
            if !lock_before {
                client.lock();
            }
            assert!(!client.resume_after_system_unlock().unwrap());
            assert_eq!(client.status(), VaultStatus::Locked);
            assert!(client.lease().is_err());
        }
    }

    #[test]
    fn damaged_resume_key_leaves_vault_locked() {
        let client = unlocked_client();
        client.suspend_for_system_lock().unwrap();
        client.state.write().unwrap().suspended_key = Some(Zeroizing::new(vec![0; 16]));
        assert!(client.resume_after_system_unlock().is_err());
        assert_eq!(client.status(), VaultStatus::Locked);
        assert!(client.lease().is_err());
        assert!(!client.resume_after_system_unlock().unwrap());
    }
}

impl BitwardenClient {
    pub fn with_profile(profile: PathBuf) -> Self {
        let cli = Cli::discover().map(|cli| Arc::new(cli) as Arc<dyn CliRunner>);
        let status = if cli.is_some() {
            VaultStatus::Locked
        } else {
            VaultStatus::Unavailable
        };
        Self {
            cli,
            profile,
            operation: Mutex::new(()),
            state: RwLock::new(State {
                status,
                session: None,
                system_locked: false,
                suspended_key: None,
            }),
            generation: AtomicU64::new(0),
        }
    }
    pub fn status(&self) -> VaultStatus {
        self.state
            .read()
            .map(|s| s.status.clone())
            .unwrap_or(VaultStatus::Locked)
    }
    pub fn info(&self) -> VaultInfo {
        let status = self.status();
        let count = self
            .state
            .read()
            .ok()
            .and_then(|s| s.session.as_ref().map(|s| s.fields.len()))
            .unwrap_or(0);
        VaultInfo {
            status,
            cli_available: self.cli.is_some(),
            cached_fields: count,
        }
    }
    pub fn lock(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut state) = self.state.write() {
            state.session = None;
            state.suspended_key = None;
            if state.status == VaultStatus::Unlocked {
                state.status = VaultStatus::Locked;
            }
        }
    }
    /// Drop plaintext caches and keep only a user-DPAPI-protected resume key in memory.
    #[cfg(windows)]
    pub fn suspend_for_system_lock(&self) -> Result<(), Error> {
        let mut state = self.state.write().map_err(|_| Error::VaultLocked)?;
        if state.system_locked {
            return Ok(());
        }
        state.system_locked = true;
        self.generation.fetch_add(1, Ordering::AcqRel);
        let session = state.session.take();
        state.suspended_key = None;
        if state.status == VaultStatus::Unlocked {
            state.status = VaultStatus::Locked;
        }
        if let Some(session) = session {
            state.suspended_key = Some(crate::profile::crypt(session.key.as_bytes(), true)?);
        }
        Ok(())
    }
    /// Resume only a session suspended by Windows locking. Manual lock clears its key.
    #[cfg(windows)]
    pub fn resume_after_system_unlock(&self) -> Result<bool, Error> {
        let mut state = self.state.write().map_err(|_| Error::VaultLocked)?;
        if !state.system_locked {
            return Ok(false);
        }
        state.system_locked = false;
        let Some(protected) = state.suspended_key.take() else {
            return Ok(false);
        };
        let bytes = crate::profile::crypt(&protected, false)?;
        let key = Zeroizing::new(
            std::str::from_utf8(&bytes)
                .map_err(|_| Error::InvalidResponse)?
                .to_owned(),
        );
        self.generation.fetch_add(1, Ordering::AcqRel);
        state.session = Some(Session {
            key,
            fields: HashMap::new(),
            summaries: None,
        });
        state.status = VaultStatus::Unlocked;
        Ok(true)
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    pub fn is_current(&self, generation: u64) -> bool {
        self.generation() == generation && self.status() == VaultStatus::Unlocked
    }
    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Result<Zeroizing<Vec<u8>>, Error> {
        let epoch = self.generation();
        self.cli
            .as_ref()
            .ok_or(Error::CliUnavailable)?
            .run(&self.profile, args, env, &|| self.generation() != epoch)
    }
    pub fn inspect(&self) -> Result<VaultInfo, Error> {
        let _op = self.operation.try_lock().map_err(|_| Error::Busy)?;
        if self.status() == VaultStatus::Unlocked {
            return Ok(self.info());
        }
        #[derive(Deserialize)]
        struct Status {
            status: String,
        }
        let bytes = self.run(&["status"], &[])?;
        let result: Status = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
        let status = match result.status.as_str() {
            "unauthenticated" => VaultStatus::Unauthenticated,
            "locked" | "unlocked" => VaultStatus::Locked,
            _ => return Err(Error::InvalidResponse),
        };
        self.state.write().map_err(|_| Error::VaultLocked)?.status = status;
        Ok(self.info())
    }
    pub fn login(
        &self,
        server: &str,
        client_id: &str,
        client_secret: &str,
    ) -> Result<VaultInfo, Error> {
        let _op = self.operation.try_lock().map_err(|_| Error::Busy)?;
        let server = server.trim();
        let client_id = client_id.trim();
        let client_secret = client_secret.trim();
        let endpoint = url::Url::parse(server).map_err(|_| Error::InvalidServer)?;
        if endpoint.scheme() != "https"
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(Error::InvalidServer);
        }
        if !client_id.starts_with("user.") || client_secret.is_empty() {
            return Err(Error::PersonalApiKeyRequired);
        }
        if self.status() == VaultStatus::Unlocked {
            return Err(Error::Busy);
        }
        // Cloud web-vault hosts are not self-hosted API base URLs. Configure
        // their published service endpoints explicitly instead of /api, /identity.
        match endpoint.as_str().trim_end_matches('/') {
            "https://vault.bitwarden.com" | "https://bitwarden.com" => {
                self.run(
                    &[
                        "config",
                        "server",
                        "https://bitwarden.com",
                        "--web-vault",
                        "https://vault.bitwarden.com",
                        "--api",
                        "https://api.bitwarden.com",
                        "--identity",
                        "https://identity.bitwarden.com",
                        "--icons",
                        "https://icons.bitwarden.net",
                        "--notifications",
                        "https://notifications.bitwarden.com",
                        "--events",
                        "https://events.bitwarden.com",
                    ],
                    &[],
                )?;
            }
            "https://vault.bitwarden.eu" | "https://bitwarden.eu" => {
                self.run(
                    &[
                        "config",
                        "server",
                        "https://bitwarden.eu",
                        "--web-vault",
                        "https://vault.bitwarden.eu",
                        "--api",
                        "https://api.bitwarden.eu",
                        "--identity",
                        "https://identity.bitwarden.eu",
                        "--icons",
                        "https://icons.bitwarden.eu",
                        "--notifications",
                        "https://notifications.bitwarden.eu",
                        "--events",
                        "https://events.bitwarden.eu",
                    ],
                    &[],
                )?;
            }
            _ => {
                self.run(&["config", "server", server], &[])?;
            }
        }
        self.run(
            &["login", "--apikey"],
            &[
                ("BW_CLIENTID", client_id),
                ("BW_CLIENTSECRET", client_secret),
            ],
        )?;
        self.state.write().map_err(|_| Error::VaultLocked)?.status = VaultStatus::Locked;
        Ok(self.info())
    }
    pub fn unlock(&self, password: &str) -> Result<(), Error> {
        let _op = self.operation.try_lock().map_err(|_| Error::Busy)?;
        self.lock();
        let epoch = self.generation();
        let bytes = self.run(
            &["unlock", "--passwordenv", "CHALLA_BW_PASSWORD"],
            &[("CHALLA_BW_PASSWORD", password)],
        )?;
        let key = Zeroizing::new(
            std::str::from_utf8(&bytes)
                .map_err(|_| Error::InvalidResponse)?
                .trim()
                .to_owned(),
        );
        if key.is_empty() || key.len() > 1024 || key.contains(char::is_whitespace) {
            return Err(Error::InvalidResponse);
        }
        let mut state = self.state.write().map_err(|_| Error::VaultLocked)?;
        if epoch != self.generation() || state.system_locked {
            return Err(Error::VaultLocked);
        }
        state.session = Some(Session {
            key,
            fields: HashMap::new(),
            summaries: None,
        });
        state.status = VaultStatus::Unlocked;
        Ok(())
    }
    pub fn logout(&self) -> Result<VaultInfo, Error> {
        self.lock();
        let _op = self.operation.try_lock().map_err(|_| Error::Busy)?;
        self.run(&["logout"], &[])?;
        self.state.write().map_err(|_| Error::VaultLocked)?.status = VaultStatus::Unauthenticated;
        Ok(self.info())
    }
    fn lease(&self) -> Result<(u64, Zeroizing<String>), Error> {
        let state = self.state.read().map_err(|_| Error::VaultLocked)?;
        let session = state.session.as_ref().ok_or(Error::VaultLocked)?;
        Ok((self.generation(), session.key.clone()))
    }
    pub fn list_items(&self) -> Result<Vec<VaultItemSummary>, Error> {
        if let Some(rows) = self
            .state
            .read()
            .map_err(|_| Error::VaultLocked)?
            .session
            .as_ref()
            .and_then(|s| s.summaries.clone())
        {
            return Ok(rows);
        }
        // Concurrent editor and library requests share the first fetch.
        let _op = self.operation.lock().map_err(|_| Error::Busy)?;
        let (epoch, key) = self.lease()?;
        if let Some(rows) = self
            .state
            .read()
            .map_err(|_| Error::VaultLocked)?
            .session
            .as_ref()
            .and_then(|s| s.summaries.clone())
        {
            return Ok(rows);
        }
        let bytes = self.run(&["list", "items"], &[("BW_SESSION", &key)])?;
        let items: Vec<Item> =
            serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
        let mut summaries: Vec<_> = items
            .into_iter()
            .filter(|i| i.deleted_date.is_none() && i.reprompt == 0)
            .map(|i| i.summary())
            .collect();
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        let mut state = self.state.write().map_err(|_| Error::VaultLocked)?;
        if epoch != self.generation() {
            return Err(Error::VaultLocked);
        }
        state.session.as_mut().ok_or(Error::VaultLocked)?.summaries = Some(summaries.clone());
        Ok(summaries)
    }
    pub fn sync(&self) -> Result<(), Error> {
        let _op = self.operation.try_lock().map_err(|_| Error::Busy)?;
        let (_, key) = self.lease()?;
        // Drop stale values first. Failed sync cannot keep obsolete credentials enabled.
        if let Ok(mut state) = self.state.write() {
            if let Some(session) = state.session.as_mut() {
                session.fields.clear();
                session.summaries = None;
            }
        }
        self.run(&["sync"], &[("BW_SESSION", &key)])?;
        Ok(())
    }
    pub fn prepare(&self, references: &[VaultReference]) -> Result<(), Error> {
        let _op = self.operation.try_lock().map_err(|_| Error::Busy)?;
        let (epoch, key) = self.lease()?;
        let missing: Vec<_> = {
            let state = self.state.read().map_err(|_| Error::VaultLocked)?;
            let session = state.session.as_ref().ok_or(Error::VaultLocked)?;
            references
                .iter()
                .filter(|r| !session.fields.contains_key(*r))
                .collect()
        };
        if references.iter().any(|r| !valid_item_id(&r.item_id)) {
            return Err(Error::FieldNotFound);
        }
        let mut fields = HashMap::new();
        let mut items = HashMap::new();
        let bulk = missing
            .iter()
            .map(|r| &r.item_id)
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1;
        let mut summaries = None;
        if bulk {
            let bytes = self.run(&["list", "items"], &[("BW_SESSION", &key)])?;
            let all: Vec<Item> =
                serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
            let mut rows: Vec<_> = all
                .iter()
                .filter(|i| i.deleted_date.is_none() && i.reprompt == 0)
                .map(Item::summary)
                .collect();
            rows.sort_by(|a, b| a.name.cmp(&b.name));
            summaries = Some(rows);
            for item in all {
                if missing.iter().any(|r| r.item_id == item.id) {
                    items.insert(item.id.clone(), item);
                }
                // Unselected secrets are dropped here, never retained in the session.
            }
        }
        for reference in missing {
            if !items.contains_key(&reference.item_id) {
                if bulk {
                    return Err(Error::ItemNotFound(reference.item_id.clone()));
                }
                let bytes = self.run(
                    &["get", "item", &reference.item_id],
                    &[("BW_SESSION", &key)],
                )?;
                let item: Item =
                    serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
                if item.id != reference.item_id || item.deleted_date.is_some() {
                    return Err(Error::ItemNotFound(reference.item_id.clone()));
                }
                items.insert(reference.item_id.clone(), item);
            }
            let item = &items[&reference.item_id];
            if item.deleted_date.is_some() {
                return Err(Error::ItemNotFound(reference.item_id.clone()));
            }
            fields.insert(reference.clone(), item.field(&reference.field)?);
        }
        let mut state = self.state.write().map_err(|_| Error::VaultLocked)?;
        if epoch != self.generation() {
            return Err(Error::VaultLocked);
        }
        let session = state.session.as_mut().ok_or(Error::VaultLocked)?;
        session.fields.retain(|r, _| references.contains(r));
        session.fields.extend(fields);
        if let Some(rows) = summaries {
            session.summaries = Some(rows);
        }
        Ok(())
    }
    /// Hook-safe: one try_lock and bounded memory work, no process, network or disk.
    pub fn prepared_references(&self) -> Vec<VaultReference> {
        // Does not generate or read secret values.
        self.state
            .read()
            .ok()
            .and_then(|s| {
                s.session
                    .as_ref()
                    .map(|session| session.fields.keys().cloned().collect())
            })
            .unwrap_or_default()
    }
    pub fn get_field(&self, item_id: &str, field: &Field) -> Result<SecretFieldValue, Error> {
        let state = self.state.try_read().map_err(|_| Error::VaultLocked)?;
        let session = state.session.as_ref().ok_or(Error::VaultLocked)?;
        let reference = VaultReference {
            item_id: item_id.to_owned(),
            field: field.clone(),
        };
        let value = match session.fields.get(&reference).ok_or(Error::FieldNotFound)? {
            Cached::Text(text) => text.to_string(),
            Cached::Totp(totp) => totp.generate_current().map_err(|_| Error::FieldNotFound)?,
        };
        Ok(SecretFieldValue::new(value))
    }
    pub fn retain_references(&self, references: &[VaultReference]) {
        if let Ok(mut state) = self.state.write() {
            if let Some(session) = state.session.as_mut() {
                session.fields.retain(|r, _| references.contains(r));
            }
        }
    }
}

pub fn valid_item_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

// Only approved fields are retained. Raw CLI JSON and sensitive strings wipe on drop.
#[derive(Deserialize)]
struct Login {
    username: Option<Zeroizing<String>>,
    password: Option<Zeroizing<String>>,
    totp: Option<Zeroizing<String>>,
}
#[derive(Deserialize)]
struct Custom {
    name: Option<String>,
    value: Option<Zeroizing<String>>,
    r#type: u8,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    id: String,
    name: String,
    login: Option<Login>,
    notes: Option<Zeroizing<String>>,
    #[serde(default, deserialize_with = "nullable_fields")]
    fields: Vec<Custom>,
    #[serde(default)]
    reprompt: u8,
    deleted_date: Option<String>,
}
fn nullable_fields<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Custom>, D::Error> {
    Ok(Option::<Vec<Custom>>::deserialize(d)?.unwrap_or_default())
}
impl Item {
    fn summary(&self) -> VaultItemSummary {
        let mut fields = Vec::new();
        if let Some(login) = &self.login {
            if login.username.as_ref().is_some_and(|s| !s.is_empty()) {
                fields.push(Field::Username);
            }
            if login.password.as_ref().is_some_and(|s| !s.is_empty()) {
                fields.push(Field::Password);
            }
            if login.totp.as_ref().is_some_and(|s| !s.is_empty()) {
                fields.push(Field::Totp);
            }
        }
        if self.notes.as_ref().is_some_and(|s| !s.is_empty()) {
            fields.push(Field::Notes);
        }
        fields.extend(
            self.fields
                .iter()
                .filter(|f| f.r#type <= 1)
                .filter_map(|f| f.name.clone().map(Field::Custom)),
        );
        VaultItemSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            username: self
                .login
                .as_ref()
                .and_then(|l| l.username.as_ref().map(|s| s.to_string())),
            fields,
        }
    }
    fn field(&self, field: &Field) -> Result<Cached, Error> {
        if self.reprompt != 0 {
            return Err(Error::RepromptRequired);
        }
        let text = match field {
            Field::Username => self.login.as_ref().and_then(|l| l.username.as_ref()),
            Field::Password => self.login.as_ref().and_then(|l| l.password.as_ref()),
            Field::Totp => self.login.as_ref().and_then(|l| l.totp.as_ref()),
            Field::Notes => self.notes.as_ref(),
            Field::Custom(name) => self
                .fields
                .iter()
                .find(|f| f.r#type <= 1 && f.name.as_ref() == Some(name))
                .and_then(|f| f.value.as_ref()),
        }
        .filter(|s| !s.is_empty() && s.encode_utf16().count() <= 16_384 && !s.contains('\0'))
        .ok_or(Error::FieldNotFound)?;
        if *field != Field::Totp {
            return Ok(Cached::Text(text.clone()));
        }
        let totp = if text.starts_with("otpauth://") {
            TOTP::from_url_unchecked(text.as_str()).map_err(|_| Error::FieldNotFound)?
        } else {
            let secret = Secret::Encoded(text.to_string())
                .to_bytes()
                .map_err(|_| Error::FieldNotFound)?;
            TOTP::new_unchecked(Algorithm::SHA1, 6, 1, 30, secret, None, "Challa".into())
        };
        // Existing vaults can contain older 80-bit seeds. Accept those while
        // validating parameters that affect generation, including URI inputs.
        if totp.secret.is_empty() || !(6..=8).contains(&totp.digits) || totp.step == 0 {
            return Err(Error::FieldNotFound);
        }
        Ok(Cached::Totp(totp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        sync::{atomic::AtomicUsize, Barrier},
    };
    const ITEM: &str = "00000000-0000-4000-8000-000000000001";
    #[test]
    fn upstream_failures_are_classified_without_exposing_upstream_text() {
        for (command, upstream, expected) in [
            (
                "login",
                "invalid_client: fixture-token-do-not-display",
                "AuthenticationRejected",
            ),
            (
                "login",
                "ECONNRESET fixture-token-do-not-display",
                "Connection",
            ),
            (
                "login",
                "UNABLE_TO_VERIFY_LEAF_SIGNATURE fixture-token-do-not-display",
                "Tls",
            ),
            (
                "config",
                "Logout required before server config update. fixture-token-do-not-display",
                "AlreadyAuthenticated",
            ),
            (
                "config",
                "unrecognized failure fixture-token-do-not-display",
                "ServerConfiguration",
            ),
            (
                "login",
                "unrecognized failure fixture-token-do-not-display",
                "AuthenticationFailed",
            ),
        ] {
            let error = classify_cli_failure(&[command], b"", upstream.as_bytes());
            assert_eq!(format!("{error:?}"), expected);
            assert!(!error.to_string().contains("fixture-token-do-not-display"));
        }
        assert!(matches!(
            classify_cli_failure(&["login"], b"invalid_client", b""),
            Error::AuthenticationRejected
        ));
    }
    #[test]
    fn copied_api_key_whitespace_is_removed_before_cli_invocation() {
        struct CheckInput;
        impl CliRunner for CheckInput {
            fn run(
                &self,
                _: &Path,
                args: &[&str],
                env: &[(&str, &str)],
                _: &dyn Fn() -> bool,
            ) -> Result<Zeroizing<Vec<u8>>, Error> {
                if args[0] == "config" {
                    assert_eq!(args[2], "https://vault.example.com");
                }
                if args[0] == "login" {
                    assert!(env
                        .iter()
                        .any(|(key, value)| *key == "BW_CLIENTID" && *value == "user.fixture"));
                    assert!(env
                        .iter()
                        .any(|(key, value)| *key == "BW_CLIENTSECRET" && *value == "fixture-key"));
                }
                Ok(Zeroizing::new(Vec::new()))
            }
        }
        let mut client = BitwardenClient::with_profile("isolated-test-profile".into());
        client.cli = Some(Arc::new(CheckInput));
        client
            .login(
                " https://vault.example.com \n",
                " user.fixture\r\n",
                " fixture-key\t",
            )
            .unwrap();
    }
    struct MockCli {
        replies: Mutex<VecDeque<(&'static str, String)>>,
        calls: AtomicUsize,
        gate: Option<Arc<Barrier>>,
    }
    impl CliRunner for MockCli {
        fn run(
            &self,
            profile: &Path,
            args: &[&str],
            env: &[(&str, &str)],
            _is_cancelled: &dyn Fn() -> bool,
        ) -> Result<Zeroizing<Vec<u8>>, Error> {
            assert_eq!(profile, Path::new("isolated-test-profile"));
            self.calls.fetch_add(1, Ordering::Relaxed);
            let (expected, response) = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected CLI call");
            assert_eq!(args.join(" "), expected);
            for (_, value) in env {
                assert!(!args.contains(value));
            }
            if args.first() == Some(&"get") {
                if let Some(gate) = &self.gate {
                    gate.wait();
                    gate.wait();
                }
            }
            Ok(Zeroizing::new(response.into_bytes()))
        }
    }
    fn fixture() -> String {
        serde_json::json!({"id":ITEM,"name":"Test item","login":{"username":"fixture-user","password":(0..20).map(|i| char::from(b'A' + i)).collect::<String>()},"fields":[{"name":"token","value":"fixture-custom","type":1}],"notes":"fixture-note","reprompt":0}).to_string()
    }
    fn client(
        replies: Vec<(&'static str, String)>,
        gate: Option<Arc<Barrier>>,
    ) -> (Arc<BitwardenClient>, Arc<MockCli>) {
        let mock = Arc::new(MockCli {
            replies: Mutex::new(replies.into()),
            calls: AtomicUsize::new(0),
            gate,
        });
        let client = Arc::new(BitwardenClient {
            cli: Some(mock.clone()),
            profile: "isolated-test-profile".into(),
            operation: Mutex::new(()),
            state: RwLock::new(State {
                status: VaultStatus::Locked,
                session: None,
                system_locked: false,
                suspended_key: None,
            }),
            generation: AtomicU64::new(0),
        });
        (client, mock)
    }
    fn reference() -> VaultReference {
        VaultReference {
            item_id: ITEM.into(),
            field: Field::Password,
        }
    }
    #[test]
    fn preparing_many_references_uses_one_fetch_and_reuses_metadata() {
        let mut all = Vec::new();
        let mut refs = Vec::new();
        for i in 0..11 {
            let mut item: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
            let id = format!("{i:08x}-0000-0000-0000-000000000000");
            item["id"] = id.clone().into();
            all.push(item);
            if i < 10 {
                refs.push(VaultReference {
                    item_id: id,
                    field: Field::Password,
                });
            }
        }
        let (client, mock) = client(
            vec![
                (
                    "unlock --passwordenv CHALLA_BW_PASSWORD",
                    "test-session".into(),
                ),
                ("list items", serde_json::to_string(&all).unwrap()),
            ],
            None,
        );
        client.unlock("test-password").unwrap();
        client.prepare(&refs).unwrap();
        assert_eq!(client.info().cached_fields, 10);
        for _ in 0..5 {
            assert_eq!(client.list_items().unwrap().len(), 11);
            client.prepare(&refs).unwrap();
        }
        assert_eq!(mock.calls.load(Ordering::Relaxed), 2);
        assert!(client
            .get_field("0000000a-0000-0000-0000-000000000000", &Field::Password)
            .is_err());
        client.prepare(&refs[..2]).unwrap();
        assert_eq!(client.info().cached_fields, 2);
        client.lock();
        assert!(client.list_items().is_err());
    }

    #[test]
    fn sync_invalidates_the_metadata_cache() {
        let (client, mock) = client(
            vec![
                (
                    "unlock --passwordenv CHALLA_BW_PASSWORD",
                    "test-session".into(),
                ),
                ("list items", format!("[{}]", fixture())),
                ("sync", String::new()),
                ("list items", "[]".into()),
            ],
            None,
        );
        client.unlock("test-password").unwrap();
        assert_eq!(client.list_items().unwrap().len(), 1);
        assert_eq!(client.list_items().unwrap().len(), 1);
        client.sync().unwrap();
        assert!(client.list_items().unwrap().is_empty());
        assert_eq!(mock.calls.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn only_reference_metadata_reaches_the_ui_and_cached_reads_never_spawn_cli() {
        let data = fixture();
        let (client, mock) = client(
            vec![
                (
                    "unlock --passwordenv CHALLA_BW_PASSWORD",
                    "fixture-session".into(),
                ),
                ("list items", format!("[{data}]")),
                ("get item 00000000-0000-4000-8000-000000000001", data),
            ],
            None,
        );
        client.unlock("fixture-input").unwrap();
        let metadata = serde_json::to_string(&client.list_items().unwrap()).unwrap();
        assert!(!metadata.contains("fixture-note") && !metadata.contains("fixture-custom"));
        assert!(metadata.contains("Password") && metadata.contains("token"));
        client.prepare(&[reference()]).unwrap();
        for _ in 0..1000 {
            assert_eq!(
                client
                    .get_field(ITEM, &Field::Password)
                    .unwrap()
                    .expose_secret()
                    .len(),
                20
            );
        }
        assert_eq!(mock.calls.load(Ordering::Relaxed), 3);
        let epoch = client.generation();
        assert_eq!(client.prepared_references(), vec![reference()]);
        assert_eq!(client.generation(), epoch);
        let generation = client.generation();
        client.lock();
        assert!(!client.is_current(generation));
        assert!(client.get_field(ITEM, &Field::Password).is_err());
        assert_eq!(client.info().cached_fields, 0);
        let locked_epoch = client.generation();
        assert!(client.prepared_references().is_empty());
        assert_eq!(client.generation(), locked_epoch);
    }
    #[test]
    fn locking_during_slow_cli_preparation_cannot_repopulate_the_cache() {
        let gate = Arc::new(Barrier::new(2));
        let (client, _) = client(
            vec![
                (
                    "unlock --passwordenv CHALLA_BW_PASSWORD",
                    "fixture-session".into(),
                ),
                ("get item 00000000-0000-4000-8000-000000000001", fixture()),
            ],
            Some(gate.clone()),
        );
        client.unlock("fixture-input").unwrap();
        let worker = {
            let client = client.clone();
            std::thread::spawn(move || client.prepare(&[reference()]))
        };
        gate.wait();
        client.lock();
        gate.wait();
        assert!(matches!(worker.join().unwrap(), Err(Error::VaultLocked)));
        assert_eq!(client.status(), VaultStatus::Locked);
        assert_eq!(client.info().cached_fields, 0);
    }
    #[test]
    fn disabling_a_reference_removes_its_value_and_keeps_the_session_unlocked() {
        let (client, _) = client(
            vec![
                (
                    "unlock --passwordenv CHALLA_BW_PASSWORD",
                    "fixture-session".into(),
                ),
                ("get item 00000000-0000-4000-8000-000000000001", fixture()),
            ],
            None,
        );
        client.unlock("fixture-input").unwrap();
        client.prepare(&[reference()]).unwrap();
        client.retain_references(&[]);
        assert_eq!(client.info().cached_fields, 0);
        assert!(matches!(
            client.get_field(ITEM, &Field::Password),
            Err(Error::FieldNotFound)
        ));
        assert_eq!(client.status(), VaultStatus::Unlocked);
    }
    #[test]
    fn reprompt_items_and_linked_fields_require_explicit_handling() {
        let mut item: Item = serde_json::from_str(&fixture()).unwrap();
        item.reprompt = 1;
        assert!(matches!(
            item.field(&Field::Password),
            Err(Error::RepromptRequired)
        ));
        item.reprompt = 0;
        item.fields[0].r#type = 3;
        assert!(item.field(&Field::Custom("token".into())).is_err());
        assert!(!item
            .summary()
            .fields
            .contains(&Field::Custom("token".into())));
        let null_fields = serde_json::json!({"id": ITEM, "name":"Empty", "fields":null});
        assert!(serde_json::from_value::<Item>(null_fields)
            .unwrap()
            .summary()
            .fields
            .is_empty());
    }
    #[test]
    fn totp_changes_at_the_step_boundary_using_rfc6238() {
        let bytes = (0..20).map(|i| b'0' + ((i + 1) % 10) as u8).collect();
        let totp = TOTP::new(Algorithm::SHA1, 8, 1, 30, bytes, None, "test".into()).unwrap();
        assert_eq!(totp.generate(59), "94287082");
        assert_ne!(totp.generate(59), totp.generate(60));
    }
    #[test]
    fn legacy_short_totp_seed_remains_compatible_with_existing_vaults() {
        let mut item: Item = serde_json::from_str(&fixture()).unwrap();
        let encoded = Secret::Raw(vec![42; 10]).to_encoded().to_string();
        item.login.as_mut().unwrap().totp = Some(Zeroizing::new(encoded));
        let Cached::Totp(totp) = item.field(&Field::Totp).unwrap() else {
            panic!("TOTP expected");
        };
        assert_eq!(totp.generate(59).len(), 6);
    }
    #[test]
    fn item_ids_are_exact_and_secrets_have_redacted_debug() {
        assert!(valid_item_id(ITEM));
        assert!(!valid_item_id("--help") && !valid_item_id("other"));
        let secret = SecretFieldValue::new("fixture-value".into());
        assert!(!format!("{secret:?}").contains("fixture-value"));
    }
    #[test]
    fn api_login_uses_environment_credentials_and_an_isolated_profile() {
        let (client, mock) = client(
            vec![
                ("config server https://vault.example.com", String::new()),
                ("login --apikey", String::new()),
                ("status", r#"{"status":"locked"}"#.into()),
            ],
            None,
        );
        client
            .login("https://vault.example.com", "user.fixture", "fixture-key")
            .unwrap();
        assert_eq!(client.inspect().unwrap().status, VaultStatus::Locked);
        assert_eq!(mock.calls.load(Ordering::Relaxed), 3);
        assert!(client
            .login(
                "https://name:pwd@vault.example.com",
                "user.fixture",
                "fixture-key"
            )
            .is_err());
    }
    #[test]
    fn cloud_servers_use_published_api_and_identity_hosts() {
        for (server, expected) in [
            ("https://vault.bitwarden.com", "config server https://bitwarden.com --web-vault https://vault.bitwarden.com --api https://api.bitwarden.com --identity https://identity.bitwarden.com --icons https://icons.bitwarden.net --notifications https://notifications.bitwarden.com --events https://events.bitwarden.com"),
            ("https://vault.bitwarden.eu", "config server https://bitwarden.eu --web-vault https://vault.bitwarden.eu --api https://api.bitwarden.eu --identity https://identity.bitwarden.eu --icons https://icons.bitwarden.eu --notifications https://notifications.bitwarden.eu --events https://events.bitwarden.eu"),
        ] {
            let (client, _) = client(vec![(expected, String::new()), ("login --apikey", String::new())], None);
            client.login(server, "user.fixture", "fixture-key").unwrap();
        }
    }
}
