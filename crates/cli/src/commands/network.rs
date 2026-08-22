//! Named discovery-network profile management.

use crate::{
    CliFailure, ExitCode,
    args::{NetworkArgs, NetworkCommand},
    config::{LoadedConfig, NetworkProfile},
};

pub(crate) fn execute(arguments: NetworkArgs, mut config: LoadedConfig) -> Result<i32, CliFailure> {
    match arguments.command {
        NetworkCommand::List => {
            for name in config.value.profile_names() {
                println!("{name}");
            }
        }
        NetworkCommand::Show { name } => {
            let profile = config.value.profile(&name).ok_or_else(invalid_profile)?;
            let output = toml::to_string_pretty(profile).map_err(|_| invalid_profile())?;
            print!("{output}");
        }
        NetworkCommand::Add { name, file } => {
            let profile = NetworkProfile::from_file(&file)?;
            config.value.add_profile(name, profile)?;
            config.save()?;
        }
        NetworkCommand::Remove { name } => {
            config.value.remove_profile(&name)?;
            config.save()?;
        }
        NetworkCommand::Use { name } => {
            config.value.use_profile(&name)?;
            config.save()?;
        }
    }
    Ok(ExitCode::Success.as_i32())
}

const fn invalid_profile() -> CliFailure {
    CliFailure::new(ExitCode::Configuration, "network profile is invalid")
}
