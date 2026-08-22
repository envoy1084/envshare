//! Initial direct-transfer command implementations.

mod receive;
mod run;
mod send;
mod shared;

use crate::{Cli, CliFailure, args::Command};

pub(crate) async fn run(cli: Cli) -> Result<i32, CliFailure> {
    match cli.command {
        Command::Send(arguments) => send::execute(arguments).await,
        Command::Receive(arguments) => receive::execute(arguments).await,
        Command::Run(arguments) => run::execute(arguments).await,
    }
}
