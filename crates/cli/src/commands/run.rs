//! Direct child execution receiver command.

use app_core::{EnvironmentMode, ParsedEnvironment, spawn_child, wait_child_forwarding_interrupt};
use protocol::ContentType;

use crate::{CliFailure, ExitCode, args::RunArgs};

use super::shared::receive_direct;

pub(crate) async fn execute(mut arguments: RunArgs) -> Result<i32, CliFailure> {
    let (program, child_arguments) = arguments
        .command
        .split_first()
        .ok_or_else(|| CliFailure::new(ExitCode::Usage, "a child program is required"))?;
    let (pending, network) = receive_direct(&mut arguments.connection).await?;
    if !matches!(
        pending.envelope().content_type(),
        ContentType::DotenvRaw | ContentType::DotenvNormalized
    ) {
        return Err(CliFailure::new(
            ExitCode::Transfer,
            "received payload is not dotenv-compatible",
        ));
    }
    let environment = ParsedEnvironment::parse(pending.envelope().payload())?;
    let mode = if arguments.clean_env {
        EnvironmentMode::Clean
    } else if arguments.r#override {
        EnvironmentMode::Override
    } else {
        EnvironmentMode::Overlay
    };
    let child = spawn_child(program.clone(), child_arguments, &environment, mode)?;
    let received = pending.acknowledge().await;
    if received.is_err() {
        return Err(CliFailure::new(
            ExitCode::Transfer,
            "child started, but sender acknowledgement was not confirmed; do not retry elsewhere",
        ));
    }
    let status = wait_child_forwarding_interrupt(child).await?;
    network.stop().await?;
    Ok(exit_status(status))
}

fn exit_status(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    signal_status(status)
}

#[cfg(unix)]
fn signal_status(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;

    status.signal().map_or(1, |signal| 128 + signal)
}

#[cfg(windows)]
fn signal_status(_status: std::process::ExitStatus) -> i32 {
    1
}
