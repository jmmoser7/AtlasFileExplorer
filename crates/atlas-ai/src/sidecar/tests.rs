use super::runtime::*;
use super::*;

#[test]
fn sidecar_next_turn_policy_keeps_read_only_and_relay_boundaries() {
    let own = atlas_agent::AgentRequest::default();
    let relayed = atlas_agent::AgentRequest {
        relayed_from: Some("other agent".into()),
        ..Default::default()
    };
    let read_only = atlas_agent::AgentRequest {
        policy: atlas_agent::TurnPolicy::ReadOnly,
        ..Default::default()
    };
    assert_eq!(
        SidecarPolicy::from_grants(&own, true, false).full_access,
        Some(true)
    );
    assert_eq!(
        SidecarPolicy::from_grants(&relayed, true, false).full_access,
        Some(false)
    );
    assert_eq!(
        SidecarPolicy::from_grants(&relayed, true, true).full_access,
        Some(true)
    );
    let restricted = SidecarPolicy::from_grants(&read_only, true, true);
    assert!(restricted.read_only);
    assert_eq!(restricted.full_access, Some(false));
}
#[test]
fn well_known_windows_node_is_a_candidate_without_env() {
    let hits = node_candidates();
    #[cfg(windows)]
    {
        assert!(
            hits.iter()
                .any(|p| p == Path::new(r"C:\Program Files\nodejs\node.exe")),
            "hardcoded Program Files path must not depend on ProgramFiles: {hits:?}"
        );
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let bundled = PathBuf::from(local)
                .join("Programs")
                .join("cursor")
                .join("resources")
                .join("app")
                .join("resources")
                .join("helpers")
                .join("node.exe");
            assert!(
                hits.iter().any(|p| p == &bundled),
                "Cursor's bundled node must be a candidate: {hits:?}"
            );
        }
    }
    #[cfg(not(windows))]
    {
        assert!(hits.iter().any(|p| p.ends_with("node")));
    }
}

#[test]
fn this_machine_program_files_node_is_found() {
    let found = resolve_node();
    #[cfg(windows)]
    {
        let well_known = PathBuf::from(r"C:\Program Files\nodejs\node.exe");
        if well_known.is_file() {
            let path = found.expect("Program Files node.exe exists — discovery must find it");
            assert!(
                path.is_file(),
                "resolved node must exist: {}",
                path.display()
            );
            return;
        }
    }
    match found {
        Ok(path) => assert!(
            path.is_file(),
            "resolved node must exist: {}",
            path.display()
        ),
        Err(e) => {
            #[cfg(windows)]
            {
                assert!(
                    !PathBuf::from(r"C:\Program Files\nodejs\node.exe").is_file(),
                    "node.exe is on disk but resolve_node failed: {e}"
                );
            }
            assert!(
                e.contains("Looked in:"),
                "a miss must list the paths we tried: {e}"
            );
        }
    }
}

#[test]
fn without_node_cursor_runs_the_sidecar_as_node() {
    let cursor = PathBuf::from("Cursor/Cursor.exe");
    let node = PathBuf::from("nodejs/node.exe");
    let found = runtime_from(Ok(node.clone()), Some(cursor.clone())).unwrap();
    assert_eq!(
        found,
        NodeRuntime {
            exe: node,
            electron: false
        }
    );
    let fallback = runtime_from(Err("no node".into()), Some(cursor.clone())).unwrap();
    assert_eq!(
        fallback,
        NodeRuntime {
            exe: cursor,
            electron: true
        }
    );
    let cmd = fallback.command();
    assert!(cmd
        .get_envs()
        .any(|(k, v)| k == "ELECTRON_RUN_AS_NODE" && v == Some("1".as_ref())));
    assert!(fallback
        .shell_line("install.mjs")
        .starts_with("$env:ELECTRON_RUN_AS_NODE=1; & \""));
    assert_eq!(
        runtime_from(Err("Looked in: x".into()), None),
        Err("Looked in: x".into())
    );
}

#[test]
fn this_machine_cursor_runs_scripts_as_a_new_enough_node() {
    let Some(exe) = crate::launch::cursor_exe() else {
        return;
    };
    let out = NodeRuntime {
        exe,
        electron: true,
    }
    .command()
    .args(["-e", "process.stdout.write(process.versions.node)"])
    .output()
    .unwrap();
    let version = String::from_utf8_lossy(&out.stdout).into_owned();
    let parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    assert!(
        parts.len() == 3 && (parts[0], parts[1]) >= (22, 13),
        "@cursor/sdk needs Node 22.13 or newer, Cursor runs {version:?}"
    );
}

#[test]
fn sidecar_js_tests_pass() {
    let Ok(node) = resolve_runtime() else {
        return;
    };
    let script = sidecar_script().expect("docs/agent/cursor-sidecar/index.mjs must ship");
    let dir = script.parent().unwrap();
    let mut tests: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.ends_with(".test.mjs"))
        .collect();
    tests.sort();
    assert!(tests.iter().any(|t| t == "install.test.mjs"), "{tests:?}");
    let out = node
        .command()
        .arg("--test")
        .args(&tests)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn watchers_are_node_or_cursor_processes() {
    assert_eq!(watcher_image("\"node.exe\""), Some(Watcher::Node));
    assert_eq!(watcher_image("node"), Some(Watcher::Node));
    assert_eq!(watcher_image("\"Cursor.exe\""), Some(Watcher::Electron));
    assert_eq!(watcher_image("\"explorer.exe\""), None);
}

#[test]
fn sidecar_sdk_is_present_when_node_modules_exists() {
    let script = sidecar_script().expect("docs/agent/cursor-sidecar/index.mjs must ship");
    let dir = script.parent().unwrap();
    if !dir.join("node_modules").is_dir() {
        return;
    }
    assert!(
        sidecar_sdk_dir(dir).is_dir(),
        "@cursor/sdk must be installed under {}",
        dir.display()
    );
    // `node_modules` can outlive the Node that installed it — it is checked
    // in on one machine and cloned onto another.
    let Ok(node) = resolve_runtime() else {
        return;
    };
    ensure_sidecar_deps(&node, dir).expect("second prepare is a no-op");
}

#[test]
fn a_missing_sdk_installs_without_npm() {
    let Ok(node) = resolve_runtime() else {
        return;
    };
    let script = sidecar_script().expect("docs/agent/cursor-sidecar/index.mjs must ship");
    let dir = std::env::temp_dir().join(format!("atlas-sidecar-install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        script.with_file_name("install.mjs"),
        dir.join("install.mjs"),
    )
    .unwrap();
    std::fs::write(
        dir.join("package-lock.json"),
        r#"{"lockfileVersion":3,"packages":{"":{},"node_modules/@cursor/sdk":{
            "version":"1.0.28",
            "resolved":"http://127.0.0.1:1/@cursor/sdk/-/sdk-1.0.28.tgz",
            "integrity":"sha512-AAAA"}}}"#,
    )
    .unwrap();
    let err = ensure_sidecar_deps(&node, &dir).unwrap_err();
    assert!(
        err.contains("Could not install the Cursor sidecar packages"),
        "{err}"
    );
    assert!(err.contains("127.0.0.1:1"), "names the download: {err}");
    assert!(
        err.contains("install.mjs") && !err.contains("npm"),
        "manual steps need no npm: {err}"
    );
    assert!(!dir.join("node_modules").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sidecar_script_resolves_from_this_crate() {
    let script = sidecar_script().expect("docs/agent/cursor-sidecar/index.mjs must ship");
    assert!(script.is_file(), "{}", script.display());
    assert!(
        script.file_name().is_some_and(|n| n == "index.mjs"),
        "{}",
        script.display()
    );
}

#[test]
fn sidecar_guide_twin_matches_the_rust_guide() {
    let script = sidecar_script().expect("docs/agent/cursor-sidecar/index.mjs must ship");
    let js = std::fs::read_to_string(script.with_file_name("artifacts.mjs")).unwrap();
    let guide = atlas_agent::artifact_guide(Some("{DIR}"));
    let (head, rest) = guide.split_once("Save new files").unwrap();
    let (folder, rest) = rest.split_once("Edit existing").unwrap();
    let (_, tail) = rest.split_once("return.json beside session.json").unwrap();
    for part in [head, folder.split_once("{DIR}").unwrap().1, tail] {
        assert!(
            js.contains(part),
            "artifacts.mjs drifted from artifact_guide: {part}"
        );
    }
}

#[test]
fn a_read_only_sidecar_never_starts_with_full_access() {
    let env = |read_only, granted| {
        access_env(
            SidecarPolicy {
                read_only,
                ..Default::default()
            },
            granted,
        )
    };
    assert_eq!(
        env(false, true),
        [
            ("ATLAS_AGENT_FULL_ACCESS", "1"),
            ("ATLAS_AGENT_READ_ONLY", "0")
        ]
    );
    assert_eq!(env(false, false)[0], ("ATLAS_AGENT_FULL_ACCESS", "0"));
    for granted in [false, true] {
        assert_eq!(
            env(true, granted),
            [
                ("ATLAS_AGENT_FULL_ACCESS", "0"),
                ("ATLAS_AGENT_READ_ONLY", "1")
            ],
            "{granted}"
        );
    }
    assert!(!SidecarPolicy::default().read_only);
}

#[test]
fn sidecar_goal_guide_twin_matches_the_rust_guide() {
    let script = sidecar_script().expect("docs/agent/cursor-sidecar/index.mjs must ship");
    let js = std::fs::read_to_string(script.with_file_name("artifacts.mjs")).unwrap();
    for (claims, in_reply) in [(true, false), (false, false), (true, true), (false, true)] {
        let goal = atlas_agent::GoalTurn {
            goal: "{GOAL}".into(),
            claims,
            in_reply,
        };
        let guide = atlas_agent::goal_guide_in(&goal, None);
        for part in guide
            .split(['\n'])
            .flat_map(|l| l.split("{GOAL}"))
            .flat_map(|l| l.split("return.json beside session.json"))
            .filter(|p| !p.is_empty())
        {
            assert!(
                js.contains(part),
                "artifacts.mjs drifted from goal_guide_in: {part}"
            );
        }
    }
    assert!(js.contains(&format!(
        "'{}'",
        atlas_agent::GOAL_MARKER.replace('\n', "\\n")
    )));
}

#[test]
fn stop_leaves_stale_or_foreign_pids_alone() {
    let dir = std::env::temp_dir().join(format!("atlas-sidecar-stop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(stop(&dir), Ok(false), "no record");
    for text in ["not a pid", &std::process::id().to_string(), "4294967"] {
        std::fs::write(dir.join("sidecar.pid"), text).unwrap();
        assert_eq!(stop(&dir), Ok(false), "{text}");
        assert!(!dir.join("sidecar.pid").exists(), "{text}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn explicit_override_is_retained_even_when_missing() {
    let missing = "missing-atlas-runtime/atlas-node.exe";
    let mut hits = Vec::new();
    push_atlas_node(&mut hits, Some(missing));
    assert_eq!(hits, vec![PathBuf::from(missing)]);
}
