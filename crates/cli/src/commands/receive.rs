//! Private file receiver command.

use app_core::{PrivateOutputOptions, write_private_atomic};

use crate::{CliFailure, ExitCode, args::ReceiveArgs};

use super::shared::receive_direct;

pub(crate) async fn execute(mut arguments: ReceiveArgs) -> Result<i32, CliFailure> {
    let (pending, network) = receive_direct(&mut arguments.connection).await?;
    write_private_atomic(
        &arguments.output,
        pending.envelope().payload(),
        PrivateOutputOptions {
            replace: arguments.force,
            durable: arguments.durable,
        },
    )?;
    let acknowledgement = pending.acknowledge().await;
    network.stop().await?;
    acknowledgement.map_err(|_| {
        CliFailure::new(
            ExitCode::Transfer,
            "output succeeded, but sender acknowledgement was not confirmed; do not retry elsewhere",
        )
    })?;
    if arguments.json {
        println!("{}", serde_json::json!({ "event": "received" }));
    } else {
        println!("Environment written successfully.");
    }
    Ok(ExitCode::Success.as_i32())
}
