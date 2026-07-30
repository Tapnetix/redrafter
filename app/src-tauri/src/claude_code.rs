// Importing the Claude Code login as a provider connection.
//
// Claude Code stores an OAuth credential for the signed-in account. Its access
// token carries a `user:inference` scope and authenticates the Anthropic
// Messages API as `Authorization: Bearer` (verified against the live API: the
// same token sent as `x-api-key` is rejected 401; `GET /v1/models` with Bearer
// returns 200). So redrafter can refine through the account's subscription
// instead of asking for a Console API key — the manual key remains the default
// and this is an explicit opt-in, per the Connections screen's button.
//
// ## Read-only, deliberately
//
// This module never writes to Claude Code's credential store, and never
// performs the `refresh_token` grant.
//
// OAuth refresh rotates the refresh token: the response carries a new one, and
// the old one is consumed. A second process refreshing without writing the new
// token back leaves Claude Code holding a spent token, which fails with
// `invalid_grant` and logs the user out of their CLI. Writing back avoids that
// but means mutating another application's keychain entry and credentials
// file, where a partial write costs the user their session — a failure mode the
// prior art (Claude-Usage-Tracker) hit and documents.
//
// Claude Code refreshes the token itself during normal use, and the access
// token is short-lived, so reading is enough in practice. When it has gone
// stale we say so and point at the one-line fix rather than reaching for the
// rotation.

use serde::{Deserialize, Serialize};

/// Filename Claude Code writes inside its config directory.
pub const CREDENTIALS_FILE: &str = ".credentials.json";

/// The provider kind stored on connections that authenticate this way. Routed
/// to the Anthropic provider with OAuth auth by `provider_for`.
pub const PROVIDER_KIND: &str = "claude-code";

/// Base URL such a connection targets.
pub const BASE_URL: &str = "https://api.anthropic.com";

/// The scope that must be present for the token to be usable for refining.
/// Without it the credential can read profile/usage data but not run
/// inference, and we would rather say so up front than fail on first refine.
pub const INFERENCE_SCOPE: &str = "user:inference";

/// Treat a token as stale slightly before it actually expires, so a refine
/// starting now doesn't die mid-flight.
const EXPIRY_SKEW_MS: i64 = 60_000;

/// The credential Claude Code stores, as far as redrafter cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub access_token: String,
    pub expires_at_ms: Option<i64>,
    pub subscription_type: Option<String>,
    pub scopes: Vec<String>,
}

impl Credentials {
    /// Whether the token is licensed to run inference.
    pub fn can_infer(&self) -> bool {
        self.scopes.iter().any(|s| s == INFERENCE_SCOPE)
    }

    /// Whether the token is expired (or close enough that a refine starting
    /// now would likely outlive it).
    pub fn is_stale(&self, now_ms: i64) -> bool {
        match self.expires_at_ms {
            Some(expiry) => now_ms + EXPIRY_SKEW_MS >= expiry,
            // No expiry recorded: assume usable rather than block the user.
            None => false,
        }
    }
}

/// What the UI shows after an import attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    /// e.g. "max" / "pro" — shown so the user can confirm which account.
    pub subscription_type: Option<String>,
    /// Whether the credential can actually be used for refining.
    pub can_infer: bool,
}

/// The on-disk shape: `{"claudeAiOauth": {...}}`.
#[derive(Debug, Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OauthBlock>,
}

#[derive(Debug, Deserialize)]
struct OauthBlock {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
}

/// Parses the credential JSON Claude Code writes.
///
/// Pure, so every shape this has to survive — a missing block, a null token,
/// an absent expiry, extra fields added by a future Claude Code — is covered
/// by tests without touching the filesystem.
pub fn parse_credentials(json: &str) -> Result<Credentials, String> {
    let parsed: CredentialsFile =
        serde_json::from_str(json).map_err(|e| format!("credentials are not valid JSON: {e}"))?;
    let block = parsed
        .claude_ai_oauth
        .ok_or_else(|| "credentials have no claudeAiOauth block".to_string())?;
    let access_token = block
        .access_token
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| "credentials have no access token".to_string())?;

    Ok(Credentials {
        access_token: access_token.trim().to_string(),
        expires_at_ms: block.expires_at,
        subscription_type: block.subscription_type,
        scopes: block.scopes,
    })
}

/// Picks the fresher of two credential sources (macOS keeps them in both the
/// keychain and a mirrored file, and either can be the newer one).
pub fn fresher(a: Option<Credentials>, b: Option<Credentials>) -> Option<Credentials> {
    match (a, b) {
        (Some(a), Some(b)) => {
            // An unknown expiry loses to a known one — a credential we can
            // reason about is worth more than one we can't.
            if b.expires_at_ms.unwrap_or(i64::MIN) > a.expires_at_ms.unwrap_or(i64::MIN) {
                Some(b)
            } else {
                Some(a)
            }
        }
        (some, None) | (None, some) => some,
    }
}

/// The directory Claude Code keeps its config in.
fn claude_dir() -> Option<std::path::PathBuf> {
    // `CLAUDE_CONFIG_DIR` is how Claude Code itself allows relocation; honour
    // it so a user with a non-default setup isn't told they aren't logged in.
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        if !dir.trim().is_empty() {
            return Some(std::path::PathBuf::from(dir));
        }
    }
    std::env::var("HOME").ok().map(|home| {
        let mut path = std::path::PathBuf::from(home);
        path.push(".claude");
        path
    })
}

fn read_credentials_file() -> Option<Credentials> {
    let path = claude_dir()?.join(CREDENTIALS_FILE);
    let raw = std::fs::read_to_string(path).ok()?;
    parse_credentials(&raw).ok()
}

/// Reads the macOS keychain entry Claude Code writes. On macOS the keychain is
/// authoritative and the file is a mirror, so both are consulted.
#[cfg(target_os = "macos")]
fn read_keychain_credentials() -> Option<Credentials> {
    let output = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_credentials(String::from_utf8_lossy(&output.stdout).trim()).ok()
}

#[cfg(not(target_os = "macos"))]
fn read_keychain_credentials() -> Option<Credentials> {
    None
}

/// Milliseconds since the Unix epoch.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Loads the freshest available credential, or explains what is missing.
pub fn load() -> Result<Credentials, String> {
    fresher(read_keychain_credentials(), read_credentials_file()).ok_or_else(|| {
        "no Claude Code login found — sign in with the `claude` CLI first".to_string()
    })
}

/// The access token to authenticate a refine with, or an actionable reason.
pub fn access_token() -> Result<String, String> {
    let credentials = load()?;
    check_usable(&credentials, now_ms())?;
    Ok(credentials.access_token)
}

/// The guard behind [`access_token`], split out so its messages are testable
/// without a real credential on disk.
pub fn check_usable(credentials: &Credentials, now_ms: i64) -> Result<(), String> {
    if !credentials.can_infer() {
        return Err(format!(
            "this Claude Code login is not permitted to run inference (no {INFERENCE_SCOPE} scope)"
        ));
    }
    if credentials.is_stale(now_ms) {
        // Deliberately not refreshed here — see this module's header. Running
        // any `claude` command rotates it, which is a one-liner for the user
        // and cannot cost them their CLI session.
        return Err(
            "your Claude Code login has expired — run any `claude` command to refresh it, \
             then try again"
                .to_string(),
        );
    }
    Ok(())
}

/// Reports whether a usable Claude Code login exists, for the settings UI.
pub fn summary() -> Result<ImportSummary, String> {
    let credentials = load()?;
    check_usable(&credentials, now_ms())?;
    Ok(ImportSummary {
        subscription_type: credentials.subscription_type.clone(),
        can_infer: credentials.can_infer(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact shape observed in a real `~/.claude/.credentials.json`.
    const REAL_SHAPE: &str = r#"{
      "claudeAiOauth": {
        "accessToken": "sk-ant-oat-example",
        "refreshToken": "sk-ant-ort-example",
        "expiresAt": 1785446002716,
        "refreshTokenExpiresAt": 1786894457716,
        "scopes": ["user:file_upload", "user:inference", "user:mcp_servers", "user:profile", "user:sessions:claude_code"],
        "subscriptionType": "max",
        "rateLimitTier": "default_claude_max_20x"
      }
    }"#;

    fn creds(scopes: &[&str], expires_at_ms: Option<i64>) -> Credentials {
        Credentials {
            access_token: "t".to_string(),
            expires_at_ms,
            subscription_type: Some("max".to_string()),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn parses_the_real_credential_shape() {
        let c = parse_credentials(REAL_SHAPE).expect("should parse");
        assert_eq!(c.access_token, "sk-ant-oat-example");
        assert_eq!(c.expires_at_ms, Some(1785446002716));
        assert_eq!(c.subscription_type.as_deref(), Some("max"));
        assert!(c.can_infer(), "the real credential carries user:inference");
    }

    #[test]
    fn tolerates_unknown_fields_a_future_claude_code_might_add() {
        let json = r#"{"claudeAiOauth":{"accessToken":"t","brandNewField":42},"somethingElse":1}"#;
        let c = parse_credentials(json).expect("should parse");
        assert_eq!(c.access_token, "t");
        assert_eq!(c.expires_at_ms, None);
        assert!(c.scopes.is_empty());
    }

    #[test]
    fn rejects_credentials_with_nothing_usable_in_them() {
        assert!(parse_credentials("not json").is_err());
        assert!(parse_credentials("{}").is_err());
        assert!(parse_credentials(r#"{"claudeAiOauth":{}}"#).is_err());
        assert!(parse_credentials(r#"{"claudeAiOauth":{"accessToken":"  "}}"#).is_err());
    }

    #[test]
    fn trims_a_token_so_a_stray_newline_cannot_break_the_header() {
        let c = parse_credentials(r#"{"claudeAiOauth":{"accessToken":" t \n"}}"#).unwrap();
        assert_eq!(c.access_token, "t");
    }

    #[test]
    fn a_token_without_the_inference_scope_is_refused_with_a_reason() {
        let c = creds(&["user:profile"], Some(i64::MAX));
        let err = check_usable(&c, 0).unwrap_err();
        assert!(err.contains("user:inference"), "got: {err}");
    }

    #[test]
    fn an_expired_token_is_refused_with_the_one_line_fix() {
        let c = creds(&[INFERENCE_SCOPE], Some(1_000));
        let err = check_usable(&c, 2_000).unwrap_err();
        assert!(err.contains("expired"), "got: {err}");
        assert!(err.contains("claude"), "should name the fix; got: {err}");
    }

    #[test]
    fn a_token_expiring_within_the_skew_counts_as_stale() {
        // A refine that starts now would outlive it.
        let c = creds(&[INFERENCE_SCOPE], Some(100_000));
        assert!(c.is_stale(100_000 - EXPIRY_SKEW_MS + 1));
        assert!(!c.is_stale(100_000 - EXPIRY_SKEW_MS - 1));
    }

    #[test]
    fn a_credential_with_no_expiry_is_not_treated_as_stale() {
        let c = creds(&[INFERENCE_SCOPE], None);
        assert!(!c.is_stale(i64::MAX));
        assert!(check_usable(&c, i64::MAX).is_ok());
    }

    #[test]
    fn a_valid_scoped_token_is_usable() {
        assert!(check_usable(&creds(&[INFERENCE_SCOPE], Some(i64::MAX)), 0).is_ok());
    }

    #[test]
    fn fresher_prefers_the_later_expiry() {
        let older = creds(&[INFERENCE_SCOPE], Some(100));
        let newer = creds(&[INFERENCE_SCOPE], Some(900));
        assert_eq!(
            fresher(Some(older.clone()), Some(newer.clone())).unwrap().expires_at_ms,
            Some(900)
        );
        assert_eq!(
            fresher(Some(newer), Some(older)).unwrap().expires_at_ms,
            Some(900)
        );
    }

    #[test]
    fn fresher_prefers_a_known_expiry_over_an_unknown_one() {
        let known = creds(&[INFERENCE_SCOPE], Some(100));
        let unknown = creds(&[INFERENCE_SCOPE], None);
        assert_eq!(
            fresher(Some(unknown), Some(known)).unwrap().expires_at_ms,
            Some(100)
        );
    }

    #[test]
    fn fresher_passes_through_whichever_source_exists() {
        let only = creds(&[INFERENCE_SCOPE], Some(5));
        assert_eq!(fresher(Some(only.clone()), None), Some(only.clone()));
        assert_eq!(fresher(None, Some(only.clone())), Some(only));
        assert_eq!(fresher(None, None), None);
    }
}

/// Tauri command: reports whether a usable Claude Code login exists, so the
/// Connections screen can show the button's state before the user commits.
#[tauri::command]
pub fn claude_code_status() -> Result<ImportSummary, String> {
    summary()
}

/// Tauri command: adds (or refreshes) the connection that refines through the
/// Claude Code login, and discovers the models it can reach.
///
/// Stores no token — the connection records only that it authenticates this
/// way; `build_provider` resolves the credential per call.
#[tauri::command]
pub async fn claude_code_connect(
    state: tauri::State<'_, crate::connections::ConnectionStore>,
) -> Result<crate::connections::Connection, String> {
    let token = access_token()?;

    // Reuse an existing Claude Code connection rather than stacking duplicates
    // every time the button is pressed.
    let existing = state
        .list()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|c| c.provider_kind == PROVIDER_KIND);

    let provider = crate::connections::provider_for(PROVIDER_KIND, BASE_URL, &token);
    match existing {
        Some(connection) => {
            crate::connections::connection_refresh_models_impl(
                &state,
                provider.as_ref(),
                &connection.id,
            )
            .await
            .map_err(|e| e.to_string())?
            .map_err(|reason| format!("connected, but model discovery failed: {reason}"))
        }
        None => crate::connections::connect_and_store(
            &state,
            provider.as_ref(),
            PROVIDER_KIND,
            BASE_URL,
            None,
        )
        .await
        .map_err(|e| e.to_string()),
    }
}
