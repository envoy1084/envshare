//! Black-box tests for the initial direct-transfer CLI.

use std::time::Duration;
use std::{
    error::Error,
    fs,
    io::{BufRead as _, BufReader},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use network::DiscoveryProvider;
use node::{NodeConfig, NodeEvent, NodeServer};
use protocol::{PROTOCOL_VERSION, ProtocolErrorCode, ProtocolErrorResponse, TransferResponse};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_envshare")
}

fn local_node_config() -> Result<NodeConfig, Box<dyn Error>> {
    Ok(NodeConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
        discovery_allow_private_addresses: true,
        ..NodeConfig::default()
    })
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
fn completions_manpage_and_network_profiles_are_generated() -> Result<(), Box<dyn Error>> {
    let bash = Command::new(binary())
        .args(["completions", "bash"])
        .output()?;
    assert!(bash.status.success());
    assert!(String::from_utf8_lossy(&bash.stdout).contains("envshare"));
    let man = Command::new(binary())
        .args(["completions", "man"])
        .output()?;
    assert!(man.status.success());
    assert!(String::from_utf8_lossy(&man.stdout).contains(".TH envshare"));

    let directory = tempfile::tempdir()?;
    let config = directory.path().join("config.toml");
    let profile = directory.path().join("profile.toml");
    let peer = network::PeerId::random();
    fs::write(
        &profile,
        format!(
            "network_id = \"lab\"\nrequire_relay = false\nrendezvous = [\"/ip4/127.0.0.1/tcp/1/p2p/{peer}\"]\n"
        ),
    )?;
    for name in ["lab", "other"] {
        assert!(
            Command::new(binary())
                .arg("--config")
                .arg(&config)
                .args(["network", "add", name, "--file"])
                .arg(&profile)
                .status()?
                .success()
        );
    }
    assert!(
        Command::new(binary())
            .arg("--config")
            .arg(&config)
            .args(["network", "use", "other"])
            .status()?
            .success()
    );
    let list = Command::new(binary())
        .arg("--config")
        .arg(&config)
        .args(["network", "list"])
        .output()?;
    assert!(list.status.success());
    assert_eq!(String::from_utf8(list.stdout)?, "lab\nother\n");
    assert_private(&config)?;
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
    assert!(
        receiver.status.success(),
        "receiver failed: {}",
        String::from_utf8_lossy(&receiver.stderr)
    );
    assert!(!String::from_utf8_lossy(&receiver.stdout).contains("payload-sentinel"));
    assert!(!String::from_utf8_lossy(&receiver.stderr).contains("payload-sentinel"));
    assert!(sender.wait()?.success());
    assert_eq!(fs::read(&output)?, payload);
    assert_private(&output)?;
    Ok(())
}

#[test]
fn selected_key_send_reports_and_delivers_normalized_payload() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let input = directory.path().join("selected.env");
    let output = directory.path().join("selected.received.env");
    fs::write(
        &input,
        b"# omitted comment\nB=last\nA='first value'\nB='selected value'\nC=omitted\n",
    )?;

    let mut sender = Command::new(binary())
        .args(["send", input.to_str().ok_or("non-UTF-8 input path")?])
        .args(["--keys", "B,A", "--expires", "15s"])
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
    assert_eq!(
        sender_lines.next().ok_or("missing payload format")??,
        "Payload format: normalized selected keys"
    );

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
    assert_successful_payload(
        &receiver,
        &output,
        b"A=\"first value\"\nB=\"selected value\"\n",
    )?;
    assert!(sender.wait()?.success());
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
        "--json",
        "--",
    ]);
    configure_child_assertion(&mut runner);
    let receiver = runner.env("TOKEN", "inherited-value").output()?;
    if receiver.status.code() != Some(23) {
        let _ = sender.kill();
    }
    assert_eq!(receiver.status.code(), Some(23));
    let stdout = String::from_utf8_lossy(&receiver.stdout);
    assert!(stdout.contains("\"event\":\"child_started\""));
    assert!(stdout.contains("\"event\":\"child_exited\""));
    assert!(stdout.contains("\"status\":23"));
    assert!(!stdout.contains("received-value"));
    assert!(!String::from_utf8_lossy(&receiver.stderr).contains("received-value"));
    assert!(sender.wait()?.success());
    Ok(())
}

#[test]
fn doctor_uses_only_a_disposable_namespace() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    let node_config = local_node_config()?;
    let (node_peer, mut events, node) =
        NodeServer::new(network::identity::Keypair::generate_ed25519(), &node_config)?;
    let cancellation = CancellationToken::new();
    let task = runtime.spawn(node.run(cancellation.clone()));
    let address = runtime.block_on(async {
        timeout(Duration::from_secs(5), async {
            loop {
                if let Some(NodeEvent::Listening { address }) = events.recv().await {
                    break Ok::<_, Box<dyn Error>>(address);
                }
            }
        })
        .await?
    })?;
    let directory = tempfile::tempdir()?;
    let config = directory.path().join("doctor.toml");
    let port = address
        .iter()
        .find_map(|protocol| match protocol {
            network::MultiaddrProtocol::Tcp(port) => Some(port),
            _ => None,
        })
        .ok_or("doctor node did not listen on TCP")?;
    let endpoint = format!("/dns4/localhost/tcp/{port}/p2p/{node_peer}");
    fs::write(
        &config,
        format!(
            "version = 1\ndefault_network = \"diag\"\n\n[networks.diag]\nnetwork_id = \"diag\"\nrequire_relay = true\nrendezvous = [\"{endpoint}\"]\nrelays = [\"{endpoint}\"]\n"
        ),
    )?;
    let result = Command::new(binary())
        .arg("--config")
        .arg(&config)
        .args(["doctor", "--network", "diag", "--json", "--verbose"])
        .output()?;
    assert!(
        result.status.success(),
        "doctor failed: {} / {}",
        String::from_utf8_lossy(&result.stderr),
        String::from_utf8_lossy(&result.stdout)
    );
    let output = String::from_utf8(result.stdout)?;
    assert!(output.contains("\"event\":\"doctor_complete\""));
    assert!(output.contains("\"direct_connectivity\":true"));
    assert!(output.contains("\"dns_endpoints\":2"));
    assert!(output.contains("\"relay_circuits\":1"));
    assert!(output.contains(&node_peer.to_string()));
    assert!(!output.contains("envshare-v1-"));

    cancellation.cancel();
    runtime.block_on(task)??;
    Ok(())
}

#[test]
fn relay_only_profile_transfers_without_a_direct_listener() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    let node_config = local_node_config()?;
    let (node_peer, mut events, node) =
        NodeServer::new(network::identity::Keypair::generate_ed25519(), &node_config)?;
    let cancellation = CancellationToken::new();
    let task = runtime.spawn(node.run(cancellation.clone()));
    let address = runtime.block_on(async {
        timeout(Duration::from_secs(5), async {
            loop {
                if let Some(NodeEvent::Listening { address }) = events.recv().await {
                    break Ok::<_, Box<dyn Error>>(address);
                }
            }
        })
        .await?
    })?;
    let directory = tempfile::tempdir()?;
    let config = directory.path().join("relay.toml");
    let endpoint = format!("{address}/p2p/{node_peer}");
    fs::write(
        &config,
        format!(
            "version = 1\ndefault_network = \"relay-test\"\n\n[networks.relay-test]\nnetwork_id = \"relay-test\"\nrequire_relay = true\nrendezvous = [\"{endpoint}\"]\nrelays = [\"{endpoint}\"]\n"
        ),
    )?;
    let input = directory.path().join("relay.env");
    let output = directory.path().join("relay.received.env");
    let payload = b"RELAY_ONLY=private\n";
    fs::write(&input, payload)?;
    let mut sender = Command::new(binary())
        .arg("--config")
        .arg(&config)
        .args(["send", input.to_str().ok_or("non-UTF-8 input path")?])
        .args(["--expires", "15s", "--relay-only"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let sender_stdout = sender.stdout.take().ok_or("missing sender stdout")?;
    let mut sender_lines = BufReader::new(sender_stdout).lines();
    let code = value_after(
        &sender_lines.next().ok_or("missing share code")??,
        "Share code: ",
    )?;
    let _sender_peer = sender_lines.next().ok_or("missing sender peer")??;
    let relay_address = sender_lines.next().ok_or("missing relay address")??;
    assert!(relay_address.starts_with("Relay address: "));

    let receiver = Command::new(binary())
        .arg("--config")
        .arg(&config)
        .args(["receive", "--code", &code, "--relay-only"])
        .arg("--output")
        .arg(&output)
        .output()?;
    if !receiver.status.success() {
        let _ = sender.kill();
    }
    assert_successful_payload(&receiver, &output, payload)?;
    assert!(sender.wait()?.success());

    cancellation.cancel();
    runtime.block_on(task)??;
    Ok(())
}

#[test]
fn federated_send_waits_for_registration_and_receives_without_route_arguments()
-> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    let config = local_node_config()?;
    let (node_peer, mut node_events, node) =
        NodeServer::new(network::identity::Keypair::generate_ed25519(), &config)?;
    let node_cancel = CancellationToken::new();
    let node_task = runtime.spawn(node.run(node_cancel.clone()));
    let node_address = runtime.block_on(async {
        timeout(Duration::from_secs(5), async {
            loop {
                if let Some(NodeEvent::Listening { address }) = node_events.recv().await {
                    break Ok::<_, Box<dyn Error>>(address);
                }
            }
        })
        .await?
    })?;
    let endpoint = format!("{node_address}/p2p/{node_peer}");
    let unavailable_peer_one = network::PeerId::random();
    let unavailable_peer_two = network::PeerId::random();
    let unavailable_endpoint_one = format!("/ip4/127.0.0.1/tcp/1/p2p/{unavailable_peer_one}");
    let unavailable_endpoint_two = format!("/ip4/127.0.0.1/tcp/2/p2p/{unavailable_peer_two}");

    let directory = tempfile::tempdir()?;
    let input = directory.path().join("public.env");
    let output = directory.path().join("public.received.env");
    let payload = b"PUBLIC_TEST=capability-authenticated\n";
    fs::write(&input, payload)?;
    let mut sender = Command::new(binary())
        .args(["send", input.to_str().ok_or("non-UTF-8 input path")?])
        .args([
            "--expires",
            "15s",
            "--discovery-node",
            &endpoint,
            "--discovery-node",
            &unavailable_endpoint_one,
            "--discovery-node",
            &unavailable_endpoint_two,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let sender_stdout = sender.stdout.take().ok_or("missing sender stdout")?;
    let mut sender_lines = BufReader::new(sender_stdout).lines();
    let code = value_after(
        &sender_lines.next().ok_or("missing share code")??,
        "Share code: ",
    )?;
    let sender_peer = value_after(
        &sender_lines.next().ok_or("missing sender peer")??,
        "Sender peer: ",
    )?
    .parse()?;
    let _sender_address = sender_lines.next().ok_or("missing sender address")??;
    runtime.block_on(async {
        timeout(Duration::from_secs(5), async {
            loop {
                if matches!(
                    node_events.recv().await,
                    Some(NodeEvent::DiscoveryRegistered { .. })
                ) {
                    break;
                }
            }
        })
        .await
    })?;
    let malicious = start_malicious_registration(
        &runtime,
        sender_peer,
        node_peer,
        node_address.clone(),
        &code,
    )?;

    let wrong_network_output = directory.path().join("wrong-network.env");
    assert_wrong_network_isolated(&code, &endpoint, &wrong_network_output)?;

    let receiver = Command::new(binary())
        .args([
            "receive",
            "--code",
            &code,
            "--discovery-node",
            &endpoint,
            "--discovery-node",
            &unavailable_endpoint_one,
            "--discovery-node",
            &unavailable_endpoint_two,
            "--lan",
        ])
        .arg("--output")
        .arg(&output)
        .output()?;
    if !receiver.status.success() {
        let _ = sender.kill();
    }
    assert_successful_payload(&receiver, &output, payload)?;
    assert!(malicious.challenged.load(Ordering::SeqCst));
    assert!(sender.wait()?.success());

    malicious.cancellation.cancel();
    runtime.block_on(malicious.driver_task)?;
    runtime.block_on(malicious.responder_task)?;
    node_cancel.cancel();
    runtime.block_on(node_task)??;
    Ok(())
}

fn assert_successful_payload(
    result: &std::process::Output,
    output: &std::path::Path,
    expected: &[u8],
) -> Result<(), Box<dyn Error>> {
    assert!(result.status.success());
    assert_eq!(fs::read(output)?, expected);
    Ok(())
}

fn assert_wrong_network_isolated(
    code: &str,
    endpoint: &str,
    output: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    let result = Command::new(binary())
        .args([
            "receive",
            "--code",
            code,
            "--network",
            "isolated-test-network",
            "--discovery-node",
            endpoint,
            "--lan",
        ])
        .arg("--output")
        .arg(output)
        .output()?;
    assert_eq!(result.status.code(), Some(11));
    assert!(!output.exists());
    Ok(())
}

struct MaliciousClient {
    challenged: Arc<AtomicBool>,
    cancellation: CancellationToken,
    driver_task: tokio::task::JoinHandle<()>,
    responder_task: tokio::task::JoinHandle<()>,
}

fn start_malicious_registration(
    runtime: &tokio::runtime::Runtime,
    sender_peer: network::PeerId,
    node_peer: network::PeerId,
    node_address: network::Multiaddr,
    code_text: &str,
) -> Result<MaliciousClient, Box<dyn Error>> {
    let keypair = loop {
        let candidate = network::identity::Keypair::generate_ed25519();
        if candidate.public().to_peer_id().to_bytes() < sender_peer.to_bytes() {
            break candidate;
        }
    };
    let (client, mut events, driver) =
        network::NetworkDriver::new(keypair, &network::NetworkConfig::default())?;
    let cancellation = CancellationToken::new();
    let driver_task = runtime.spawn(driver.run(cancellation.clone()));
    runtime.block_on(client.listen("/ip4/127.0.0.1/tcp/0".parse()?))?;
    let address = runtime.block_on(async {
        timeout(Duration::from_secs(5), async {
            loop {
                if let Some(network::NetworkEvent::Listening { address }) = events.recv().await {
                    break Ok::<_, Box<dyn Error>>(address);
                }
            }
        })
        .await?
    })?;
    runtime.block_on(client.add_discovery_address(address))?;
    let code: code::ShareCode = code_text.parse()?;
    let root = crypto::derive_root(code.secret(), "public-v1")?;
    let namespace = network::DiscoveryNamespace::from_room_id(*root.room_id().as_bytes());
    runtime.block_on(client.register(node_peer, node_address, namespace, 30))?;
    runtime.block_on(async {
        timeout(Duration::from_secs(5), async {
            loop {
                if matches!(
                    events.recv().await,
                    Some(network::NetworkEvent::DiscoveryRegistered { .. })
                ) {
                    break;
                }
            }
        })
        .await
    })?;
    let challenged = Arc::new(AtomicBool::new(false));
    let observed = challenged.clone();
    let responder = client.clone();
    let responder_task = runtime.spawn(async move {
        while let Some(event) = events.recv().await {
            if let network::NetworkEvent::InboundRequest { request_id, .. } = event {
                observed.store(true, Ordering::SeqCst);
                let _ = responder
                    .respond(
                        request_id,
                        TransferResponse::Error(ProtocolErrorResponse {
                            protocol_version: PROTOCOL_VERSION,
                            code: ProtocolErrorCode::NotFoundOrUnauthorized,
                        }),
                    )
                    .await;
            }
        }
    });
    Ok(MaliciousClient {
        challenged,
        cancellation,
        driver_task,
        responder_task,
    })
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
