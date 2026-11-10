use super::*;
use std::collections::HashMap;

#[test]
fn native_candidates_follow_current_user_not_developer_machine() {
    let values = HashMap::from([
        (
            "ATLAS_CURSOR_AGENT",
            OsString::from(r"Q:\portable Ω folder\agent.exe"),
        ),
        ("USERPROFILE", OsString::from(r"Q:\People\Zoë Example")),
        (
            "LOCALAPPDATA",
            OsString::from(r"Q:\People\Zoë Example\AppData\Local"),
        ),
        (
            "APPDATA",
            OsString::from(r"Q:\People\Zoë Example\AppData\Roaming"),
        ),
    ]);
    let paths = candidates(Provider::Cursor, &|key| values.get(key).cloned());
    assert_eq!(paths[0], PathBuf::from(r"Q:\portable Ω folder\agent.exe"));
    let name = if cfg!(windows) {
        "cursor-agent.exe"
    } else {
        "cursor-agent"
    };
    assert!(paths.contains(
        &PathBuf::from(r"Q:\People\Zoë Example\AppData\Local")
            .join("cursor-agent")
            .join(name)
    ));
    assert!(!paths
        .iter()
        .any(|path| path.to_string_lossy().contains("jeffr")));
}

#[test]
fn discovery_with_no_environment_has_no_machine_specific_fallback() {
    assert!(candidates(Provider::ClaudeCode, &|_| None).is_empty());
}

#[test]
fn subscription_auth_does_not_confuse_api_billing_with_oauth() {
    for method in ["api_key", "api_key_helper", "third_party"] {
        let status = format!(
            r#"{{"loggedIn":true,"authMethod":"{method}","email":"must-not-escape@example.invalid"}}"#
        );
        assert_eq!(
            auth_status(Provider::ClaudeCode, true, &status, "").0,
            AuthStatus::LoginRequired
        );
    }
    for method in ["claude.ai", "oauth_token"] {
        let status = format!(r#"{{"loggedIn":true,"authMethod":"{method}"}}"#);
        assert_eq!(
            auth_status(Provider::ClaudeCode, true, &status, "").0,
            AuthStatus::LoggedIn
        );
        assert_eq!(
            auth_status(Provider::ClaudeCode, false, &status, "").0,
            AuthStatus::Unknown
        );
    }
    assert_eq!(
        auth_status(Provider::ClaudeCode, true, r#"{"loggedIn":true}"#, "").0,
        AuthStatus::Unknown
    );
    assert_eq!(
        auth_status(
            Provider::ClaudeCode,
            false,
            r#"{"loggedIn":false,"authMethod":"none"}"#,
            ""
        )
        .0,
        AuthStatus::LoginRequired
    );
}

#[test]
fn cursor_negative_auth_text_wins_over_authenticated_substring() {
    assert_eq!(
        auth_status(Provider::Cursor, true, "Not authenticated", "").0,
        AuthStatus::LoginRequired
    );
    assert_eq!(
        auth_status(
            Provider::Cursor,
            true,
            "Logged in as private@example.invalid",
            ""
        ),
        (AuthStatus::LoggedIn, Some("cursor_login".into()))
    );
    assert_eq!(
        auth_status(Provider::Cursor, true, "Unknown account state", "").0,
        AuthStatus::Unknown
    );
    assert_eq!(
        auth_status(Provider::Cursor, true, "Authenticated with API key", "").0,
        AuthStatus::LoginRequired
    );
}

#[test]
fn subscription_commands_remove_api_selectors_only_in_child() {
    for provider in [Provider::Cursor, Provider::ClaudeCode] {
        let command = provider.command(Path::new("native-provider.exe"));
        let changes: Vec<_> = command.get_envs().collect();
        for key in api_environment(provider) {
            assert!(changes
                .iter()
                .any(|(name, value)| *name == std::ffi::OsStr::new(key) && value.is_none()));
        }
        for preserved in [
            "USERPROFILE",
            "HOME",
            "CURSOR_CONFIG_DIR",
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_CODE_OAUTH_TOKEN",
        ] {
            assert!(!changes
                .iter()
                .any(|(name, _)| *name == std::ffi::OsStr::new(preserved)));
        }
    }
}

#[test]
fn bundled_cursor_descriptor_keeps_unicode_and_shell_characters_as_arguments() {
    let root = std::env::temp_dir().join(format!("slate-cli Ω $' {}", atlas_agent::request_id()));
    std::fs::create_dir(&root).unwrap();
    for file in ["node.exe", "index.js", "agent.cmd"] {
        std::fs::write(root.join(file), []).unwrap();
    }
    let command = cursor_native_command(&root.join("agent.cmd")).unwrap();
    assert_eq!(command.get_program(), root.join("node.exe"));
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        vec![root.join("index.js").as_os_str()]
    );
    for file in ["node.exe", "index.js", "agent.cmd"] {
        std::fs::remove_file(root.join(file)).unwrap();
    }
    std::fs::remove_dir(root).unwrap();
}
