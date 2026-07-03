//! Secure key storage (B10).
//!
//! Persists provider API keys at rest, addressed by connection id
//! (`connections.rs`'s `Connection::key_ref`). Two backends, selected by a
//! `storage_backend` setting (`STORAGE_BACKEND_SETTING_KEY`, read through
//! `settings.rs`):
//!
//!   - [`EncryptedFileBackend`] (default): AES-256-GCM (via the `ring`
//!     crate) encrypts each key before it ever touches disk. The symmetric
//!     "master key" is 32 random bytes generated on first use and stored in
//!     a sibling file (`secrets.key`) with owner-only permissions (`0600`
//!     on unix). This keeps keys out of the plaintext config/db (no more
//!     `grep`-able secrets, no plaintext copies in ad-hoc backups of just
//!     the config file) but -- like most keyless-KMS desktop schemes -- it
//!     does not defend against an attacker with full access to the same OS
//!     user account, since the master key sits unencrypted next to the
//!     ciphertext it protects. Users who want a stronger guarantee opt into
//!     the OS keychain backend instead.
//!   - [`KeychainBackend`] (opt-in, `storage_backend = "keychain"`):
//!     delegates to the macOS Keychain via the `security-framework` crate's
//!     `passwords` module (`set_generic_password`/`get_generic_password`/
//!     `delete_generic_password`). cfg-gated to macOS; every other target
//!     compiles a stub that reports the backend unsupported rather than
//!     failing the build, so the crate (and this module's tests) still
//!     compile and run off macOS. The real keychain path is verified on
//!     macOS hardware separately, per the plan.
//!
//! [`secrets_get`] is deliberately a plain function, never a
//! `#[tauri::command]` -- there is no IPC path that returns a raw key to
//! the frontend. Only backend-internal code (`connections.rs`'s
//! `provider_for`, once B23 reconciles the two key-storage call sites) may
//! call it.

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::settings::SettingsStore;

/// Settings key (`SettingsStore`) the storage-backend choice is persisted
/// under. Unset/unrecognized values default to [`StorageBackend::EncryptedFile`].
pub const STORAGE_BACKEND_SETTING_KEY: &str = "secrets.storage_backend";

/// AES-256 key length in bytes.
const MASTER_KEY_LEN: usize = 32;

/// Which backend [`SecretStore`] currently reads/writes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBackend {
    EncryptedFile,
    Keychain,
}

impl StorageBackend {
    /// The persisted string form (`settings.rs`'s value column).
    pub fn as_str(self) -> &'static str {
        match self {
            StorageBackend::EncryptedFile => "encrypted_file",
            StorageBackend::Keychain => "keychain",
        }
    }

    /// Parses a persisted setting value. Anything other than `"keychain"`
    /// (including unset/unrecognized values) is the safe default.
    pub fn parse(value: &str) -> Self {
        if value == "keychain" {
            StorageBackend::Keychain
        } else {
            StorageBackend::EncryptedFile
        }
    }
}

/// The seam behind both storage backends. Lets [`SecretStore`]'s backend
/// *selection* be unit-tested with fakes, independent of either backend's
/// real storage mechanics, and both real backends be round-trip tested
/// identically.
pub trait SecretBackend: Send + Sync {
    fn set(&self, connection_id: &str, key: &str) -> Result<()>;
    fn get(&self, connection_id: &str) -> Result<Option<String>>;
    fn delete(&self, connection_id: &str) -> Result<()>;
}

/// On-disk shape of the encrypted secrets file: connection id -> base64
/// (nonce || ciphertext+tag).
type EncryptedMap = HashMap<String, String>;

/// Default backend: an encrypted app-config file (see module docs for the
/// scheme and its threat model).
pub struct EncryptedFileBackend {
    secrets_path: PathBuf,
    master_key: [u8; MASTER_KEY_LEN],
    // Guards the read-modify-write cycle in `set`/`delete` (SQLite's own
    // stores use a `Mutex<Connection>` for the same reason; this file has
    // no built-in transaction, so the mutex plays that role here).
    lock: Mutex<()>,
}

impl EncryptedFileBackend {
    /// Opens (creating on first use) the encrypted secrets file and its
    /// master key file under `dir`.
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)
            .with_context(|| format!("failed to create secrets dir {}", dir.display()))?;
        let master_key = Self::load_or_create_master_key(&dir.join("secrets.key"))?;
        Ok(Self {
            secrets_path: dir.join("secrets.enc.json"),
            master_key,
            lock: Mutex::new(()),
        })
    }

    fn load_or_create_master_key(path: &Path) -> Result<[u8; MASTER_KEY_LEN]> {
        if let Ok(bytes) = fs::read(path) {
            if bytes.len() == MASTER_KEY_LEN {
                let mut key = [0u8; MASTER_KEY_LEN];
                key.copy_from_slice(&bytes);
                return Ok(key);
            }
        }
        let mut key = [0u8; MASTER_KEY_LEN];
        SystemRandom::new()
            .fill(&mut key)
            .map_err(|_| anyhow::anyhow!("failed to generate a master key"))?;
        // A stale file at this point (the `fs::read` above fell through
        // without returning) is corrupt/wrong-length -- clear it so the
        // owner-only `create_new` below doesn't fail with "already exists".
        let _ = fs::remove_file(path);
        Self::write_master_key(path, &key)?;
        Ok(key)
    }

    /// Creates `path` owner-only (unix: `0600`) from the moment it comes
    /// into existence -- no separate `fs::write` + `set_permissions` step,
    /// which would leave the 32-byte AES master key briefly group/world
    /// readable between the two syscalls.
    #[cfg(unix)]
    fn write_master_key(path: &Path, key: &[u8; MASTER_KEY_LEN]) -> Result<()> {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        file.write_all(key)
            .with_context(|| format!("failed to write {}", path.display()))
    }

    #[cfg(not(unix))]
    fn write_master_key(path: &Path, key: &[u8; MASTER_KEY_LEN]) -> Result<()> {
        fs::write(path, key).with_context(|| format!("failed to write {}", path.display()))
    }

    fn load_map(&self) -> Result<EncryptedMap> {
        match fs::read_to_string(&self.secrets_path) {
            // An empty file is still legitimately "no secrets yet" (e.g.
            // truncated by a prior crash right after creation) -- only a
            // NON-EMPTY unparseable file is corruption. Propagating that
            // as an error (rather than `unwrap_or_default`'s silent empty
            // map) matters because the next `set` would otherwise persist
            // that empty map and destroy every previously stored secret.
            Ok(contents) if contents.trim().is_empty() => Ok(EncryptedMap::new()),
            Ok(contents) => serde_json::from_str(&contents)
                .with_context(|| format!("corrupt secrets file {}", self.secrets_path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(EncryptedMap::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn save_map(&self, map: &EncryptedMap) -> Result<()> {
        let json = serde_json::to_string(map)?;
        fs::write(&self.secrets_path, json)
            .with_context(|| format!("failed to write {}", self.secrets_path.display()))
    }

    fn encrypt(&self, plaintext: &str) -> Result<String> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        SystemRandom::new()
            .fill(&mut nonce_bytes)
            .map_err(|_| anyhow::anyhow!("failed to generate a nonce"))?;
        let unbound = UnboundKey::new(&AES_256_GCM, &self.master_key)
            .map_err(|_| anyhow::anyhow!("invalid master key"))?;
        let key = LessSafeKey::new(unbound);
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut in_out = plaintext.as_bytes().to_vec();
        key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| anyhow::anyhow!("encryption failed"))?;
        let mut out = nonce_bytes.to_vec();
        out.extend_from_slice(&in_out);
        Ok(base64::engine::general_purpose::STANDARD.encode(out))
    }

    fn decrypt(&self, encoded: &str) -> Result<String> {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .context("corrupt secret (invalid base64)")?;
        if raw.len() < NONCE_LEN {
            bail!("corrupt secret (too short)");
        }
        let (nonce_bytes, ciphertext) = raw.split_at(NONCE_LEN);
        let mut in_out = ciphertext.to_vec();
        let unbound = UnboundKey::new(&AES_256_GCM, &self.master_key)
            .map_err(|_| anyhow::anyhow!("invalid master key"))?;
        let key = LessSafeKey::new(unbound);
        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| anyhow::anyhow!("corrupt secret (invalid nonce)"))?;
        let plaintext = key
            .open_in_place(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| anyhow::anyhow!("failed to decrypt secret (wrong key or corrupt data)"))?;
        Ok(String::from_utf8(plaintext.to_vec())?)
    }
}

impl SecretBackend for EncryptedFileBackend {
    fn set(&self, connection_id: &str, key: &str) -> Result<()> {
        let _guard = self.lock.lock().unwrap();
        let mut map = self.load_map()?;
        map.insert(connection_id.to_string(), self.encrypt(key)?);
        self.save_map(&map)
    }

    fn get(&self, connection_id: &str) -> Result<Option<String>> {
        let _guard = self.lock.lock().unwrap();
        let map = self.load_map()?;
        match map.get(connection_id) {
            Some(encoded) => Ok(Some(self.decrypt(encoded)?)),
            None => Ok(None),
        }
    }

    fn delete(&self, connection_id: &str) -> Result<()> {
        let _guard = self.lock.lock().unwrap();
        let mut map = self.load_map()?;
        map.remove(connection_id);
        self.save_map(&map)
    }
}

/// `errSecItemNotFound`'s `OSStatus` value, duplicated here (rather than
/// depending on `security-framework-sys` directly for one constant) --
/// see <https://developer.apple.com/documentation/security/errsecitemnotfound>.
#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

/// The keychain "service" every entry is grouped under (namespaces this
/// app's entries the same way other desktop apps do).
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "com.redrafter.app.secrets";

/// Opt-in backend: the OS keychain. macOS-only -- see module docs.
pub struct KeychainBackend;

impl KeychainBackend {
    pub fn new() -> Self {
        KeychainBackend
    }
}

impl Default for KeychainBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl SecretBackend for KeychainBackend {
    fn set(&self, connection_id: &str, key: &str) -> Result<()> {
        // `set_generic_password` overwrites an existing entry for the same
        // service+account, so this doubles as add-or-update.
        security_framework::passwords::set_generic_password(
            KEYCHAIN_SERVICE,
            connection_id,
            key.as_bytes(),
        )
        .map_err(|e| anyhow::anyhow!("keychain set failed: {e}"))
    }

    fn get(&self, connection_id: &str) -> Result<Option<String>> {
        match security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, connection_id) {
            Ok(bytes) => Ok(Some(String::from_utf8(bytes)?)),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
            Err(e) => Err(anyhow::anyhow!("keychain get failed: {e}")),
        }
    }

    fn delete(&self, connection_id: &str) -> Result<()> {
        match security_framework::passwords::delete_generic_password(
            KEYCHAIN_SERVICE,
            connection_id,
        ) {
            Ok(()) => Ok(()),
            // Idempotent, mirroring `EncryptedFileBackend::delete` and
            // `ConnectionStore::remove`.
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(anyhow::anyhow!("keychain delete failed: {e}")),
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl SecretBackend for KeychainBackend {
    fn set(&self, _connection_id: &str, _key: &str) -> Result<()> {
        bail!("OS keychain storage is only supported on macOS")
    }

    fn get(&self, _connection_id: &str) -> Result<Option<String>> {
        bail!("OS keychain storage is only supported on macOS")
    }

    fn delete(&self, _connection_id: &str) -> Result<()> {
        bail!("OS keychain storage is only supported on macOS")
    }
}

/// Picks which backend is active. Pulled out as a free function (rather
/// than inlined into `SecretStore::active`) so backend *selection* is
/// unit-testable with two fakes, independent of either real backend's
/// storage mechanics.
fn select_backend<'a>(
    encrypted_file: &'a dyn SecretBackend,
    keychain: &'a dyn SecretBackend,
    choice: StorageBackend,
) -> &'a dyn SecretBackend {
    match choice {
        StorageBackend::EncryptedFile => encrypted_file,
        StorageBackend::Keychain => keychain,
    }
}

/// Owns both real backends and dispatches to whichever is currently
/// selected. Managed as Tauri state (mirrors `SettingsStore`/
/// `ConnectionStore`); wiring it into `lib.rs`'s app setup and registering
/// `secrets_set`/`secrets_delete` in the invoke handler is B23's job (see
/// module docs).
pub struct SecretStore {
    encrypted_file: EncryptedFileBackend,
    keychain: KeychainBackend,
    backend: Mutex<StorageBackend>,
}

impl SecretStore {
    /// Opens the encrypted-file backend under `dir` and defaults to it as
    /// the active backend (matching the plan: "encrypted-file by default,
    /// keychain opt-in").
    pub fn open(dir: &Path) -> Result<Self> {
        Ok(Self {
            encrypted_file: EncryptedFileBackend::open(dir)?,
            keychain: KeychainBackend::new(),
            backend: Mutex::new(StorageBackend::EncryptedFile),
        })
    }

    /// Selects which backend subsequent `set`/`get`/`delete` calls use.
    /// Does not migrate existing secrets between backends -- switching
    /// backends is a forward-looking choice, not a bulk re-encrypt.
    pub fn set_storage_backend(&self, backend: StorageBackend) {
        *self.backend.lock().unwrap() = backend;
    }

    pub fn storage_backend(&self) -> StorageBackend {
        *self.backend.lock().unwrap()
    }

    fn active(&self) -> &dyn SecretBackend {
        select_backend(&self.encrypted_file, &self.keychain, self.storage_backend())
    }

    pub fn set(&self, connection_id: &str, key: &str) -> Result<()> {
        self.active().set(connection_id, key)
    }

    pub fn get(&self, connection_id: &str) -> Result<Option<String>> {
        self.active().get(connection_id)
    }

    pub fn delete(&self, connection_id: &str) -> Result<()> {
        self.active().delete(connection_id)
    }
}

/// Reads the persisted storage-backend choice from `settings`
/// ([`STORAGE_BACKEND_SETTING_KEY`]), defaulting to
/// [`StorageBackend::EncryptedFile`] when unset.
pub fn storage_backend_from_settings(settings: &SettingsStore) -> StorageBackend {
    settings
        .get(STORAGE_BACKEND_SETTING_KEY)
        .ok()
        .flatten()
        .map(|v| StorageBackend::parse(&v))
        .unwrap_or(StorageBackend::EncryptedFile)
}

/// Persists the storage-backend choice to `settings` and applies it to
/// `store` immediately.
pub fn set_storage_backend(
    store: &SecretStore,
    settings: &SettingsStore,
    backend: StorageBackend,
) -> Result<()> {
    settings.set(STORAGE_BACKEND_SETTING_KEY, backend.as_str())?;
    store.set_storage_backend(backend);
    Ok(())
}

/// Tauri command: stores `key` for `connection_id` through whichever
/// backend is currently active. Never echoes the key back -- the frontend
/// gets an `Ok(())`/error, nothing else (see module docs).
///
/// Not yet in the production invoke handler -- B23 registers it (see
/// module docs).
#[tauri::command]
pub fn secrets_set(
    state: tauri::State<'_, SecretStore>,
    connection_id: String,
    key: String,
) -> Result<(), String> {
    state.set(&connection_id, &key).map_err(|e| e.to_string())
}

/// Tauri command: deletes the stored key for `connection_id`, if any
/// (idempotent -- succeeds even if none was stored). Not yet in the
/// production invoke handler -- B23 registers it (see module docs).
#[tauri::command]
pub fn secrets_delete(
    state: tauri::State<'_, SecretStore>,
    connection_id: String,
) -> Result<(), String> {
    state.delete(&connection_id).map_err(|e| e.to_string())
}

/// Fetches the stored key for `connection_id`, if any. Deliberately *not* a
/// `#[tauri::command]` -- the only callers are backend-internal (see module
/// docs); there is no IPC path that hands a raw key back to the frontend.
pub fn secrets_get(store: &SecretStore, connection_id: &str) -> Result<Option<String>> {
    store.get(connection_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    /// Minimal RAII temp-dir guard (mirrors `connections.rs`'s
    /// `TempDbPath`): a process- and test-unique directory under the OS
    /// temp dir, removed (recursively) on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "redrafter_secrets_{label}_{}_{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn new_backend(label: &str) -> (EncryptedFileBackend, TempDir) {
        let dir = TempDir::new(label);
        let backend = EncryptedFileBackend::open(&dir.0).expect("failed to open backend");
        (backend, dir)
    }

    // ── EncryptedFileBackend ──

    #[test]
    fn roundtrip_set_then_get_returns_the_key() {
        let (backend, _dir) = new_backend("roundtrip");
        backend.set("conn-1", "sk-secret-key").unwrap();
        assert_eq!(
            backend.get("conn-1").unwrap(),
            Some("sk-secret-key".to_string())
        );
    }

    #[test]
    fn get_of_an_unknown_connection_returns_none() {
        let (backend, _dir) = new_backend("missing");
        assert_eq!(backend.get("nonexistent").unwrap(), None);
    }

    #[test]
    fn delete_removes_the_key() {
        let (backend, _dir) = new_backend("delete");
        backend.set("conn-1", "sk-secret").unwrap();
        backend.delete("conn-1").unwrap();
        assert_eq!(backend.get("conn-1").unwrap(), None);
    }

    #[test]
    fn delete_of_an_unknown_connection_is_a_no_op() {
        let (backend, _dir) = new_backend("delete-missing");
        assert!(backend.delete("nonexistent").is_ok());
    }

    #[test]
    fn set_overwrites_an_existing_key() {
        let (backend, _dir) = new_backend("overwrite");
        backend.set("conn-1", "sk-old").unwrap();
        backend.set("conn-1", "sk-new").unwrap();
        assert_eq!(backend.get("conn-1").unwrap(), Some("sk-new".to_string()));
    }

    #[test]
    fn does_not_store_the_key_in_plaintext_on_disk() {
        let (backend, dir) = new_backend("plaintext");
        let secret = "sk-super-secret-plaintext-marker-0123456789";
        backend.set("conn-1", secret).unwrap();

        let raw = fs::read(dir.0.join("secrets.enc.json")).unwrap();
        assert!(
            !raw.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "the encrypted file must not contain the plaintext key"
        );
        // Sanity: the same backend still decrypts it correctly.
        assert_eq!(backend.get("conn-1").unwrap(), Some(secret.to_string()));
    }

    #[test]
    fn master_key_file_is_owner_only_readable_on_unix() {
        let (_backend, dir) = new_backend("perms");
        let meta = fs::metadata(dir.0.join("secrets.key")).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn reopening_reuses_the_same_master_key_so_existing_secrets_still_decrypt() {
        let dir = TempDir::new("reopen");
        {
            let backend = EncryptedFileBackend::open(&dir.0).unwrap();
            backend.set("conn-1", "sk-persist").unwrap();
        }
        let reopened = EncryptedFileBackend::open(&dir.0).unwrap();
        assert_eq!(
            reopened.get("conn-1").unwrap(),
            Some("sk-persist".to_string())
        );
    }

    #[test]
    fn corrupt_non_empty_secrets_file_fails_loudly_instead_of_silently_wiping_keys() {
        let (backend, dir) = new_backend("corrupt-store");
        // Establish a real file first, then corrupt it -- an absent/empty
        // file is still legitimately an empty map (first run), but a
        // NON-EMPTY unparseable file must never be treated as "no secrets".
        backend.set("conn-1", "sk-secret").unwrap();
        let path = dir.0.join("secrets.enc.json");
        fs::write(&path, "not valid json {{{").unwrap();

        assert!(
            backend.get("conn-1").is_err(),
            "a corrupt secrets file must error, not silently look empty"
        );
    }

    #[test]
    fn tampered_ciphertext_fails_to_decrypt_rather_than_returning_garbage() {
        let (backend, dir) = new_backend("tamper");
        backend.set("conn-1", "sk-secret").unwrap();

        // Deterministically flip a byte in the ciphertext+tag region (just
        // past the nonce) of the persisted (base64) blob -- decryption must
        // fail loudly (AEAD tag mismatch) rather than silently returning
        // corrupted plaintext. (A previous version of this test tampered by
        // string-replacing 'a'/'A' in the base64 text, which was a no-op --
        // and the test spuriously green -- whenever the random nonce/
        // ciphertext for that run happened to contain neither letter.)
        let path = dir.0.join("secrets.enc.json");
        let mut map: EncryptedMap =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let before = map.get("conn-1").unwrap().clone();
        let mut raw = base64::engine::general_purpose::STANDARD
            .decode(&before)
            .unwrap();
        assert!(NONCE_LEN < raw.len(), "ciphertext too short to tamper with");
        raw[NONCE_LEN] ^= 0x01;
        let after = base64::engine::general_purpose::STANDARD.encode(&raw);
        assert_ne!(before, after, "tampering must actually mutate the blob");
        map.insert("conn-1".to_string(), after);
        fs::write(&path, serde_json::to_string(&map).unwrap()).unwrap();

        assert!(backend.get("conn-1").is_err());
    }

    // ── KeychainBackend (macOS: real API smoke-tested on hardware
    // separately; off macOS: the stub must degrade gracefully rather than
    // fail to compile/run) ──

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn keychain_backend_stub_reports_unsupported_off_macos() {
        let backend = KeychainBackend::new();
        assert!(backend.set("conn-1", "sk-x").is_err());
        assert!(backend.get("conn-1").is_err());
        assert!(backend.delete("conn-1").is_err());
    }

    // ── backend selection (trait seam) ──

    #[derive(Default)]
    struct FakeBackend {
        stored: Mutex<HashMap<String, String>>,
    }

    impl SecretBackend for FakeBackend {
        fn set(&self, connection_id: &str, key: &str) -> Result<()> {
            self.stored
                .lock()
                .unwrap()
                .insert(connection_id.to_string(), key.to_string());
            Ok(())
        }

        fn get(&self, connection_id: &str) -> Result<Option<String>> {
            Ok(self.stored.lock().unwrap().get(connection_id).cloned())
        }

        fn delete(&self, connection_id: &str) -> Result<()> {
            self.stored.lock().unwrap().remove(connection_id);
            Ok(())
        }
    }

    #[test]
    fn select_backend_picks_the_encrypted_file_fake_for_that_choice() {
        let encrypted_file = FakeBackend::default();
        let keychain = FakeBackend::default();

        select_backend(&encrypted_file, &keychain, StorageBackend::EncryptedFile)
            .set("conn-1", "sk-a")
            .unwrap();

        assert_eq!(
            encrypted_file.get("conn-1").unwrap(),
            Some("sk-a".to_string())
        );
        assert_eq!(keychain.get("conn-1").unwrap(), None);
    }

    #[test]
    fn select_backend_picks_the_keychain_fake_for_that_choice() {
        let encrypted_file = FakeBackend::default();
        let keychain = FakeBackend::default();

        select_backend(&encrypted_file, &keychain, StorageBackend::Keychain)
            .set("conn-1", "sk-b")
            .unwrap();

        assert_eq!(keychain.get("conn-1").unwrap(), Some("sk-b".to_string()));
        assert_eq!(encrypted_file.get("conn-1").unwrap(), None);
    }

    // ── SecretStore ──

    #[test]
    fn secret_store_defaults_to_the_encrypted_file_backend() {
        let dir = TempDir::new("store-default");
        let store = SecretStore::open(&dir.0).unwrap();
        assert_eq!(store.storage_backend(), StorageBackend::EncryptedFile);

        store.set("conn-1", "sk-store").unwrap();
        assert_eq!(store.get("conn-1").unwrap(), Some("sk-store".to_string()));
    }

    #[test]
    fn secret_store_delete_roundtrips() {
        let dir = TempDir::new("store-delete");
        let store = SecretStore::open(&dir.0).unwrap();
        store.set("conn-1", "sk-store").unwrap();

        store.delete("conn-1").unwrap();

        assert_eq!(store.get("conn-1").unwrap(), None);
    }

    #[test]
    fn secret_store_set_storage_backend_switches_the_active_backend() {
        let dir = TempDir::new("store-switch");
        let store = SecretStore::open(&dir.0).unwrap();
        store.set("conn-1", "sk-file").unwrap();

        store.set_storage_backend(StorageBackend::Keychain);
        assert_eq!(store.storage_backend(), StorageBackend::Keychain);

        // Switching backends means the encrypted-file-only key is no
        // longer visible through the (now active) keychain backend --
        // off macOS that surfaces as the stub's error, on macOS as a
        // real (and correct) miss.
        assert!(store.get("conn-1").is_err() || store.get("conn-1").unwrap().is_none());
    }

    #[test]
    fn secrets_get_free_function_delegates_to_the_store() {
        let dir = TempDir::new("free-fn-get");
        let store = SecretStore::open(&dir.0).unwrap();
        store.set("conn-1", "sk-free-fn").unwrap();

        assert_eq!(
            secrets_get(&store, "conn-1").unwrap(),
            Some("sk-free-fn".to_string())
        );
    }

    // ── storage_backend setting plumbing ──

    #[test]
    fn storage_backend_parse_defaults_unknown_values_to_encrypted_file() {
        assert_eq!(
            StorageBackend::parse("encrypted_file"),
            StorageBackend::EncryptedFile
        );
        assert_eq!(StorageBackend::parse("keychain"), StorageBackend::Keychain);
        assert_eq!(
            StorageBackend::parse("bogus"),
            StorageBackend::EncryptedFile
        );
    }

    #[test]
    fn storage_backend_from_settings_defaults_when_unset() {
        let settings = SettingsStore::open_in_memory().unwrap();
        assert_eq!(
            storage_backend_from_settings(&settings),
            StorageBackend::EncryptedFile
        );
    }

    #[test]
    fn storage_backend_from_settings_reads_the_persisted_choice() {
        let settings = SettingsStore::open_in_memory().unwrap();
        settings
            .set(STORAGE_BACKEND_SETTING_KEY, "keychain")
            .unwrap();

        assert_eq!(
            storage_backend_from_settings(&settings),
            StorageBackend::Keychain
        );
    }

    #[test]
    fn set_storage_backend_persists_the_choice_and_applies_it_to_the_store() {
        let dir = TempDir::new("store-persist");
        let store = SecretStore::open(&dir.0).unwrap();
        let settings = SettingsStore::open_in_memory().unwrap();

        set_storage_backend(&store, &settings, StorageBackend::Keychain).unwrap();

        assert_eq!(store.storage_backend(), StorageBackend::Keychain);
        assert_eq!(
            settings.get(STORAGE_BACKEND_SETTING_KEY).unwrap(),
            Some("keychain".to_string())
        );
    }
}
