//! Process-level failure, race, and interruption coverage.

use std::{
    error::Error,
    fs,
    io::{BufRead as _, BufReader, Read as _},
    path::Path,
    process::{Child, ChildStdout, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_envshare")
}

struct DirectSender {
    child: Child,
    _stdout: BufReader<ChildStdout>,
    code: String,
    peer: String,
    address: String,
}

impl DirectSender {
    fn start(directory: &Path, payload: &[u8]) -> Result<Self, Box<dyn Error>> {
        let input = directory.join("sender.env");
        fs::write(&input, payload)?;
        let mut child = Command::new(binary())
            .args(["send", input.to_str().ok_or("non-UTF-8 input path")?])
            .arg("--verbose")
            .args(["--expires", "15s"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().ok_or("missing sender stdout")?;
        let mut stdout = BufReader::new(stdout);
        let code = read_value(&mut stdout, "Share code: ")?;
        let peer = read_value(&mut stdout, "Sender peer: ")?;
        let address = read_value(&mut stdout, "Direct address: ")?;
        Ok(Self {
            child,
            _stdout: stdout,
            code,
            peer,
            address,
        })
    }

    fn receiver(&self, output: &Path) -> Command {
        let mut command = Command::new(binary());
        command
            .args([
                "receive",
                "--code",
                &self.code,
                "--peer",
                &self.peer,
                "--address",
                &self.address,
            ])
            .arg("--output")
            .arg(output)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn wait(&mut self, deadline: Duration) -> Result<(ExitStatus, String), Box<dyn Error>> {
        let started = Instant::now();
        let status = loop {
            if let Some(status) = self.child.try_wait()? {
                break status;
            }
            if started.elapsed() >= deadline {
                self.child.kill()?;
                return Err("sender did not stop before the test deadline".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut stderr = String::new();
        if let Some(mut reader) = self.child.stderr.take() {
            reader.read_to_string(&mut stderr)?;
        }
        Ok((status, stderr))
    }
}

impl Drop for DirectSender {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn two_simultaneous_receivers_create_exactly_one_output() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let payload = b"RACE_SECRET=one-winner-only\n";
    let mut sender = DirectSender::start(directory.path(), payload)?;
    let first_path = directory.path().join("first.env");
    let second_path = directory.path().join("second.env");
    let first = sender.receiver(&first_path).spawn()?;
    let second = sender.receiver(&second_path).spawn()?;

    let first = first.wait_with_output()?;
    let second = second.wait_with_output()?;
    let successes = usize::from(first.status.success()) + usize::from(second.status.success());
    assert_eq!(successes, 1);
    assert_eq!(
        usize::from(first_path.exists()) + usize::from(second_path.exists()),
        1
    );
    let winner = if first_path.exists() {
        &first_path
    } else {
        &second_path
    };
    assert_eq!(fs::read(winner)?, payload);
    assert_process_output_is_secret_safe(&first, &sender.code, "one-winner-only");
    assert_process_output_is_secret_safe(&second, &sender.code, "one-winner-only");
    let (status, stderr) = sender.wait(Duration::from_secs(5))?;
    assert!(status.success());
    assert!(!stderr.contains("one-winner-only"));
    Ok(())
}

#[test]
fn existing_output_is_not_replaced_without_force() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let payload = b"OVERWRITE_SECRET=must-not-leak\n";
    let mut sender = DirectSender::start(directory.path(), payload)?;
    let output = directory.path().join("existing.env");
    fs::write(&output, b"ORIGINAL=preserved\n")?;

    let result = sender.receiver(&output).output()?;

    assert_eq!(result.status.code(), Some(15));
    assert_eq!(fs::read(&output)?, b"ORIGINAL=preserved\n");
    assert_process_output_is_secret_safe(&result, &sender.code, "must-not-leak");
    sender.child.kill()?;
    let _ = sender.child.wait()?;
    Ok(())
}

#[test]
fn unavailable_discovery_node_fails_without_creating_output() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("missing.env");
    let code = code::ShareCode::generate()?.to_string();
    let peer = network::PeerId::random();
    let endpoint = format!("/ip4/127.0.0.1/tcp/1/p2p/{peer}");

    let result = Command::new(binary())
        .args(["receive", "--code", &code, "--discovery-node", &endpoint])
        .arg("--output")
        .arg(&output)
        .output()?;

    assert!(!result.status.success());
    assert!(!output.exists());
    assert_process_output_is_secret_safe(&result, &code, "missing-node-secret");
    Ok(())
}

#[cfg(unix)]
#[test]
fn interrupt_stops_a_waiting_sender_with_stable_status() -> Result<(), Box<dyn Error>> {
    use nix::{
        sys::signal::{Signal, kill},
        unistd::Pid,
    };

    let directory = tempfile::tempdir()?;
    let mut sender = DirectSender::start(directory.path(), b"INTERRUPT_SECRET=private\n")?;
    std::thread::sleep(Duration::from_millis(100));

    kill(
        Pid::from_raw(i32::try_from(sender.child.id())?),
        Signal::SIGINT,
    )?;
    let (status, stderr) = sender.wait(Duration::from_secs(5))?;

    assert_eq!(status.code(), Some(130));
    assert!(!stderr.contains(&sender.code));
    assert!(!stderr.contains("private"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn interrupt_reaches_child_group_and_leaves_no_grandchild() -> Result<(), Box<dyn Error>> {
    use nix::{
        errno::Errno,
        sys::signal::{Signal, kill},
        unistd::Pid,
    };

    let directory = tempfile::tempdir()?;
    let mut sender = DirectSender::start(directory.path(), b"CHILD_SECRET=private\n")?;
    let pid_path = directory.path().join("grandchild.pid");
    let mut runner = Command::new(binary())
        .args([
            "run",
            "--code",
            &sender.code,
            "--peer",
            &sender.peer,
            "--address",
            &sender.address,
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "sleep 30 & echo $! > \"$1\"; wait",
            "envshare-interrupt-test",
        ])
        .arg(&pid_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let runner_stdout = runner.stdout.take().ok_or("missing runner stdout")?;
    let mut runner_stdout = BufReader::new(runner_stdout);
    let mut event = String::new();
    runner_stdout.read_line(&mut event)?;
    assert!(event.contains("\"event\":\"child_started\""));
    let grandchild = wait_for_pid(&pid_path)?;

    kill(Pid::from_raw(i32::try_from(runner.id())?), Signal::SIGINT)?;
    let status = runner.wait()?;

    assert!(status.code().is_some_and(|code| code >= 128));
    assert!(matches!(
        kill(Pid::from_raw(grandchild), None),
        Err(Errno::ESRCH)
    ));
    let (sender_status, sender_stderr) = sender.wait(Duration::from_secs(5))?;
    assert!(sender_status.success());
    assert!(!sender_stderr.contains("private"));
    Ok(())
}

fn read_value(reader: &mut BufReader<ChildStdout>, prefix: &str) -> Result<String, Box<dyn Error>> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    line.trim_end()
        .strip_prefix(prefix)
        .map(str::to_owned)
        .ok_or_else(|| format!("unexpected sender output; expected {prefix}").into())
}

fn assert_process_output_is_secret_safe(
    result: &std::process::Output,
    code: &str,
    payload_sentinel: &str,
) {
    for output in [&result.stdout, &result.stderr] {
        let output = String::from_utf8_lossy(output);
        assert!(!output.contains(code));
        assert!(!output.contains(payload_sentinel));
    }
}

#[cfg(unix)]
fn wait_for_pid(path: &Path) -> Result<i32, Box<dyn Error>> {
    let started = Instant::now();
    loop {
        match fs::read_to_string(path) {
            Ok(value) if !value.trim().is_empty() => return Ok(value.trim().parse()?),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if started.elapsed() >= Duration::from_secs(5) {
            return Err("child did not report its grandchild PID".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
