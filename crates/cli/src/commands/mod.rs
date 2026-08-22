//! Initial direct-transfer command implementations.

mod completions;
mod doctor;
mod network;
mod receive;
mod run;
mod send;
mod shared;

use crate::{Cli, CliFailure, args::Command, config::LoadedConfig};

pub(crate) async fn run(cli: Cli) -> Result<i32, CliFailure> {
    match cli.command {
        Command::Completions(arguments) => completions::execute(arguments),
        Command::Send(mut arguments) => {
            let config = LoadedConfig::load(cli.config)?;
            config.value.apply_send(&mut arguments)?;
            send::execute(arguments).await
        }
        Command::Receive(mut arguments) => {
            let config = LoadedConfig::load(cli.config)?;
            config.value.apply_connection(&mut arguments.connection)?;
            receive::execute(arguments).await
        }
        Command::Run(mut arguments) => {
            let config = LoadedConfig::load(cli.config)?;
            config.value.apply_connection(&mut arguments.connection)?;
            run::execute(arguments).await
        }
        Command::Doctor(arguments) => {
            let config = LoadedConfig::load(cli.config)?;
            doctor::execute(arguments, &config.value).await
        }
        Command::Network(arguments) => {
            let config = LoadedConfig::load_for_management(cli.config)?;
            network::execute(arguments, config)
        }
    }
}
