//! Black-box tests for the initial direct-transfer CLI.

use std::{
    error::Error,
    fs,
    io::{BufRead as _, BufReader},
    process::{Command, Stdio},
};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_envshare")
}

#[test]
fn invalid_code_never_echoes_the_supplied_value() -> Result<(), Box<dyn Error>> {
    let sentinel = "SECRET-SENTINEL-NOT-A-CODE";
    let output = Command::new(binary())
        .args([
            "receive",
            "--code",
            sentinel,
            "--peer",
            "invalid-peer",
            "--address",
            "/ip4/127.0.0.1/tcp/1",
        ])
        .output()?;
    assert_eq!(output.status.code(), Some(10));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(sentinel));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(sentinel));
    Ok(())
}

#[test]
fn direct_send_and_receive_preserve_exact_private_payload() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let input = directory.path().join("source.env");
    let output = directory.path().join("received.env");
    let payload = b"# exact bytes\r\nTOKEN=payload-sentinel\r\nEMPTY=\r\n";
    fs::write(&input, payload)?;

    let mut sender = Command::new(binary())
        .args(["send", input.to_str().ok_or("non-UTF-8 input path")?])
        .arg("--expires")
        .arg("15s")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let sender_stdout = sender.stdout.take().ok_or("missing sender stdout")?;
    let mut sender_lines = BufReader::new(sender_stdout).lines();
    let code = value_after(
        &sender_lines.next().ok_or("missing share code")??,
        "Share code: ",
    )?;
    let peer = value_after(
        &sender_lines.next().ok_or("missing sender peer")??,
        "Sender peer: ",
    )?;
    let address = value_after(
        &sender_lines.next().ok_or("missing sender address")??,
        "Direct address: ",
    )?;

    let receiver = Command::new(binary())
        .args([
            "receive",
            "--code",
            &code,
            "--peer",
            &peer,
            "--address",
            &address,
        ])
        .arg("--output")
        .arg(&output)
        .output()?;
    if !receiver.status.success() {
        let _ = sender.kill();
    }
    assert!(receiver.status.success());
    assert!(!String::from_utf8_lossy(&receiver.stdout).contains("payload-sentinel"));
    assert!(!String::from_utf8_lossy(&receiver.stderr).contains("payload-sentinel"));
    assert!(sender.wait()?.success());
    assert_eq!(fs::read(&output)?, payload);
    assert_private(&output)?;
    Ok(())
}

#[test]
fn direct_run_overrides_environment_and_propagates_exit_status() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let input = directory.path().join("run.env");
    fs::write(&input, b"TOKEN=received-value\n")?;
    let mut sender = Command::new(binary())
        .args(["send", input.to_str().ok_or("non-UTF-8 input path")?])
        .arg("--expires")
        .arg("15s")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let sender_stdout = sender.stdout.take().ok_or("missing sender stdout")?;
    let mut sender_lines = BufReader::new(sender_stdout).lines();
    let code = value_after(
        &sender_lines.next().ok_or("missing share code")??,
        "Share code: ",
    )?;
    let peer = value_after(
        &sender_lines.next().ok_or("missing sender peer")??,
        "Sender peer: ",
    )?;
    let address = value_after(
        &sender_lines.next().ok_or("missing sender address")??,
        "Direct address: ",
    )?;

    let mut runner = Command::new(binary());
    runner.args([
        "run",
        "--code",
        &code,
        "--peer",
        &peer,
        "--address",
        &address,
        "--override",
        "--",
    ]);
    configure_child_assertion(&mut runner);
    let receiver = runner.env("TOKEN", "inherited-value").output()?;
    if receiver.status.code() != Some(23) {
        let _ = sender.kill();
    }
    assert_eq!(receiver.status.code(), Some(23));
    assert!(!String::from_utf8_lossy(&receiver.stdout).contains("received-value"));
    assert!(!String::from_utf8_lossy(&receiver.stderr).contains("received-value"));
    assert!(sender.wait()?.success());
    Ok(())
}

#[cfg(unix)]
fn configure_child_assertion(command: &mut Command) {
    command.args(["/bin/sh", "-c", "test \"$TOKEN\" = received-value; exit 23"]);
}

#[cfg(windows)]
fn configure_child_assertion(command: &mut Command) {
    command.args([
        "cmd.exe",
        "/C",
        "if not \"%TOKEN%\"==\"received-value\" exit /b 24 & exit /b 23",
    ]);
}

fn value_after(line: &str, prefix: &str) -> Result<String, Box<dyn Error>> {
    line.strip_prefix(prefix)
        .map(str::to_owned)
        .ok_or_else(|| format!("unexpected sender output classification: {prefix}").into())
}

#[cfg(unix)]
fn assert_private(path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
    Ok(())
}

#[cfg(windows)]
fn assert_private(_path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    Ok(())
}
