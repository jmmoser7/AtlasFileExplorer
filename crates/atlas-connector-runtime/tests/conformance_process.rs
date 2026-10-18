#[cfg(windows)]
#[test]
fn public_runner_exercises_the_contract_between_separate_processes() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_connector-conformance"))
        .arg("self-test")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("PASS:"));
}
