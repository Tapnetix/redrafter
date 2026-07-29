// Opening an external URL in the user's real browser.
//
// A plain `<a href="https://…" target="_blank">` does nothing inside a Tauri
// webview: there is no browser chrome to open a tab in, and the webview
// refuses to navigate away from the app's own origin. Every "Get an API key
// ↗" affordance (Connections' add/edit sheet, FirstRun's cloud panel) was
// therefore dead on click. The frontend now routes those through this
// command instead.
//
// Deliberately implemented with `std::process::Command` per platform --
// mirroring `permission.rs`'s `open_settings_platform` -- rather than adding
// `tauri-plugin-opener`: it's the same three lines, keeps the dependency
// tree (and the updater's signed-bundle surface) unchanged, and the URL is
// passed as a single argv entry, never through a shell.
//
// (Plain `//` rather than `//!` for the same `include!`-into-tests reason as
// `connections.rs`.)

/// Rejects anything that isn't a plain `http(s)` URL before it reaches a
/// platform opener. The frontend only ever passes provider console links,
/// but this command is reachable from any page script, so an `file://`,
/// `javascript:`, or argument-injecting value must not get through.
pub fn validate_external_url(url: &str) -> Result<(), String> {
    if url.len() > 2048 {
        return Err("url too long".to_string());
    }
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("only http(s) urls can be opened".to_string());
    }
    // A leading `-` can't occur after the scheme check, but whitespace and
    // control characters still have no business in a URL and are exactly what
    // an opener could misread as a second argument.
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("url contains whitespace or control characters".to_string());
    }
    Ok(())
}

/// Tauri command: opens `url` in the user's default browser. Registered by
/// `lib.rs`'s `invoke_handler`.
#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    validate_external_url(&url)?;
    open_external_platform(&url)
}

#[cfg(target_os = "macos")]
fn open_external_platform(url: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn open_external_platform(url: &str) -> Result<(), String> {
    // `url.dll,FileProtocolHandler` is the documented shell-free way to hand
    // a URL to the default browser -- unlike `cmd /C start`, nothing
    // re-parses the argument.
    std::process::Command::new("rundll32.exe")
        .arg("url.dll,FileProtocolHandler")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn open_external_platform(url: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_https_and_http_urls() {
        assert_eq!(
            validate_external_url("https://console.anthropic.com/settings/keys"),
            Ok(())
        );
        assert_eq!(validate_external_url("http://localhost:8080/keys"), Ok(()));
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(validate_external_url("file:///etc/passwd").is_err());
        assert!(validate_external_url("javascript:alert(1)").is_err());
        assert!(validate_external_url("x-apple.systempreferences:foo").is_err());
        assert!(validate_external_url("").is_err());
    }

    #[test]
    fn rejects_whitespace_and_control_characters() {
        assert!(validate_external_url("https://example.com/a b").is_err());
        assert!(validate_external_url("https://example.com/a\nb").is_err());
        assert!(validate_external_url("https://example.com/a\0b").is_err());
    }

    #[test]
    fn rejects_absurdly_long_urls() {
        let long = format!("https://example.com/{}", "a".repeat(4096));
        assert!(validate_external_url(&long).is_err());
    }
}
