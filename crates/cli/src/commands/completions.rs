//! Generated shell completions and manual page.

use std::io;

use clap::CommandFactory as _;

use crate::{
    Cli, CliFailure, ExitCode,
    args::{CompletionArgs, CompletionTarget},
};

pub(crate) fn execute(arguments: CompletionArgs) -> Result<i32, CliFailure> {
    let mut command = Cli::command();
    let mut output = io::stdout().lock();
    match arguments.target {
        CompletionTarget::Bash => {
            clap_complete::generate(
                clap_complete::Shell::Bash,
                &mut command,
                "envshare",
                &mut output,
            );
        }
        CompletionTarget::Zsh => {
            clap_complete::generate(
                clap_complete::Shell::Zsh,
                &mut command,
                "envshare",
                &mut output,
            );
        }
        CompletionTarget::Fish => {
            clap_complete::generate(
                clap_complete::Shell::Fish,
                &mut command,
                "envshare",
                &mut output,
            );
        }
        CompletionTarget::PowerShell => {
            clap_complete::generate(
                clap_complete::Shell::PowerShell,
                &mut command,
                "envshare",
                &mut output,
            );
        }
        CompletionTarget::Elvish => {
            clap_complete::generate(
                clap_complete::Shell::Elvish,
                &mut command,
                "envshare",
                &mut output,
            );
        }
        CompletionTarget::Man => clap_mangen::Man::new(command)
            .render(&mut output)
            .map_err(|_| CliFailure::new(ExitCode::Output, "manual generation failed"))?,
    }
    Ok(ExitCode::Success.as_i32())
}
