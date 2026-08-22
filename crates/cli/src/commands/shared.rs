//! Shared direct receiver and network helpers.

use std::str::FromStr;

use app_core::{CoreError, DirectReceiver, PendingDirectOffer, ReceiverSession, read_bounded};
use code::ShareCode;
use crypto::derive_root;
use network::{Multiaddr, NetworkConfig, NetworkDriver, PeerId, identity};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::{CliFailure, ExitCode, args::ConnectionArgs};

pub(crate) struct RunningNetwork {
    pub cancellation: CancellationToken,
    pub task: JoinHandle<()>,
}

impl RunningNetwork {
    pub async fn stop(self) -> Result<(), CliFailure> {
        self.cancellation.cancel();
        self.task
            .await
            .map_err(|_| CliFailure::new(ExitCode::Internal, "network task failed"))
    }
}

pub(crate) async fn receive_direct(
    arguments: &mut ConnectionArgs,
) -> Result<(PendingDirectOffer, RunningNetwork), CliFailure> {
    let code_text = read_code(arguments)?;
    let code = ShareCode::from_str(code_text.trim())
        .map_err(|_| CliFailure::new(ExitCode::InvalidCode, "invalid share code"))?;
    let sender_peer = PeerId::from_str(&arguments.peer)
        .map_err(|_| CliFailure::new(ExitCode::Configuration, "invalid sender Peer ID"))?;
    let sender_address = Multiaddr::from_str(&arguments.address)
        .map_err(|_| CliFailure::new(ExitCode::Configuration, "invalid sender address"))?;
    let keypair = identity::Keypair::generate_ed25519();
    let receiver_peer = keypair.public().to_peer_id();
    let root =
        derive_root(code.secret(), &arguments.network).map_err(|_| CoreError::InvalidCode)?;
    let session = ReceiverSession::new(
        root,
        arguments.network.clone(),
        sender_peer.to_bytes(),
        receiver_peer.to_bytes(),
        random_receiver_nonce()?,
    )?;
    let (client, _events, driver) =
        NetworkDriver::new(keypair, &NetworkConfig::default()).map_err(|_| CoreError::Network)?;
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(driver.run(cancellation.clone()));
    let pending = DirectReceiver::new(client, session, sender_peer, sender_address)
        .receive()
        .await?;
    Ok((pending, RunningNetwork { cancellation, task }))
}

fn read_code(arguments: &mut ConnectionArgs) -> Result<Zeroizing<String>, CliFailure> {
    if let Some(code) = arguments.code.take() {
        return Ok(Zeroizing::new(code));
    }
    if arguments.code_stdin {
        let bytes = read_bounded(std::io::stdin().lock(), 128)?;
        return String::from_utf8(bytes)
            .map(Zeroizing::new)
            .map_err(|_| CliFailure::new(ExitCode::InvalidCode, "invalid share code"));
    }
    rpassword::prompt_password("Share code: ")
        .map(Zeroizing::new)
        .map_err(|_| CliFailure::new(ExitCode::InvalidCode, "could not read share code"))
}

fn random_receiver_nonce() -> Result<[u8; 32], CliFailure> {
    let mut nonce = [0_u8; 32];
    getrandom::fill(&mut nonce)
        .map_err(|_| CliFailure::new(ExitCode::Internal, "secure randomness unavailable"))?;
    Ok(nonce)
}

pub(crate) fn read_sender_input(path: &std::path::Path) -> Result<Vec<u8>, CliFailure> {
    if path == std::path::Path::new("-") {
        return read_bounded(std::io::stdin().lock(), protocol::MAX_PAYLOAD_BYTES)
            .map_err(Into::into);
    }
    let file = std::fs::File::open(path)
        .map_err(|_| CliFailure::new(ExitCode::Output, "could not open input"))?;
    let metadata = file
        .metadata()
        .map_err(|_| CliFailure::new(ExitCode::Output, "could not inspect input"))?;
    if !metadata.is_file() {
        return Err(CliFailure::new(
            ExitCode::Output,
            "input must be a regular file",
        ));
    }
    read_bounded(file, protocol::MAX_PAYLOAD_BYTES).map_err(Into::into)
}
