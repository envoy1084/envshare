//! Private file receiver command.

use std::{fs::File, path::PathBuf, str::FromStr as _};

use app_core::{
    DotenvMergeMode, DotenvMergeSummary, PrivateOutputOptions, merge_dotenv, read_bounded,
    write_private_atomic,
};

use crate::{
    CliFailure, ExitCode,
    args::{ReceiveArgs, ReceiveMode},
    presentation::{self, ExistingDestinationChoice},
};

use super::shared::{read_code, receive_direct_with_code};
use zeroize::Zeroizing;

enum OutputAction {
    Create,
    Replace,
    Merge(DotenvMergeMode),
}

struct OutputPlan {
    destination: PathBuf,
    action: OutputAction,
}

pub(crate) async fn execute(mut arguments: ReceiveArgs) -> Result<i32, CliFailure> {
    let code = read_code(&mut arguments.connection)?;
    code::ShareCode::from_str(code.trim())
        .map_err(|_| CliFailure::new(ExitCode::InvalidCode, "invalid share code"))?;
    let output = resolve_output_plan(&arguments)?;
    let (pending, network) = receive_direct_with_code(&arguments.connection, &code).await?;
    let summary = persist_received(&output, pending.envelope().payload(), arguments.durable)?;
    let acknowledgement = pending.acknowledge().await;
    network.stop().await?;
    acknowledgement.map_err(|_| {
        CliFailure::new(
            ExitCode::Transfer,
            "output succeeded, but sender acknowledgement was not confirmed; do not retry elsewhere",
        )
    })?;
    emit_success(&arguments, &output, summary)?;
    Ok(ExitCode::Success.as_i32())
}

fn resolve_output_plan(arguments: &ReceiveArgs) -> Result<OutputPlan, CliFailure> {
    let interactive = presentation::is_interactive() && !arguments.json;
    let destination = arguments.output.clone().unwrap_or_else(|| {
        if interactive {
            PathBuf::from(".env")
        } else {
            PathBuf::from(".env.shared")
        }
    });
    let requested_mode = if arguments.force {
        Some(ReceiveMode::Replace)
    } else {
        arguments.mode
    };
    resolve_destination(destination, requested_mode, interactive)
}

fn resolve_destination(
    mut destination: PathBuf,
    requested_mode: Option<ReceiveMode>,
    interactive: bool,
) -> Result<OutputPlan, CliFailure> {
    loop {
        let exists = destination.try_exists().map_err(|_| {
            CliFailure::new(ExitCode::Output, "could not inspect output destination")
        })?;
        if !exists {
            return Ok(OutputPlan {
                destination,
                action: OutputAction::Create,
            });
        }

        validate_existing_destination(&destination)?;

        if let Some(mode) = requested_mode {
            let action = match mode {
                ReceiveMode::Create => {
                    return Err(CliFailure::new(
                        ExitCode::Output,
                        "destination already exists; choose --mode merge, --mode append-missing, or --mode replace",
                    ));
                }
                ReceiveMode::Replace => OutputAction::Replace,
                ReceiveMode::Merge => OutputAction::Merge(DotenvMergeMode::ReplaceExisting),
                ReceiveMode::AppendMissing => OutputAction::Merge(DotenvMergeMode::KeepExisting),
            };
            return Ok(OutputPlan {
                destination,
                action,
            });
        }

        if !interactive {
            return Err(CliFailure::new(
                ExitCode::Output,
                "destination already exists; choose --mode merge, --mode append-missing, or --mode replace",
            ));
        }

        match presentation::choose_existing_destination(&destination)? {
            ExistingDestinationChoice::Merge => {
                return Ok(OutputPlan {
                    destination,
                    action: OutputAction::Merge(DotenvMergeMode::ReplaceExisting),
                });
            }
            ExistingDestinationChoice::AppendMissing => {
                return Ok(OutputPlan {
                    destination,
                    action: OutputAction::Merge(DotenvMergeMode::KeepExisting),
                });
            }
            ExistingDestinationChoice::Replace => {
                return Ok(OutputPlan {
                    destination,
                    action: OutputAction::Replace,
                });
            }
            ExistingDestinationChoice::SaveElsewhere => {
                destination = presentation::prompt_new_destination()?;
            }
            ExistingDestinationChoice::Cancel => {
                return Err(CliFailure::new(ExitCode::Interrupted, "cancelled"));
            }
        }
    }
}

fn validate_existing_destination(destination: &std::path::Path) -> Result<(), CliFailure> {
    let metadata = std::fs::symlink_metadata(destination)
        .map_err(|_| CliFailure::new(ExitCode::Output, "could not inspect output destination"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(CliFailure::new(
            ExitCode::Output,
            "output destination must be a regular file, not a symlink or special file",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(CliFailure::new(
                ExitCode::Output,
                "output destination must not be a reparse point",
            ));
        }
    }
    Ok(())
}

fn persist_received(
    output: &OutputPlan,
    payload: &[u8],
    durable: bool,
) -> Result<Option<DotenvMergeSummary>, CliFailure> {
    let (contents, replace, summary) = match output.action {
        OutputAction::Create => (Zeroizing::new(payload.to_vec()), false, None),
        OutputAction::Replace => (Zeroizing::new(payload.to_vec()), true, None),
        OutputAction::Merge(mode) => {
            let existing = Zeroizing::new(read_bounded(
                File::open(&output.destination).map_err(|_| {
                    CliFailure::new(ExitCode::Output, "could not read output destination")
                })?,
                protocol::MAX_PAYLOAD_BYTES,
            )?);
            let (merged, summary) = merge_dotenv(&existing, payload, mode)?;
            (Zeroizing::new(merged), true, Some(summary))
        }
    };
    write_private_atomic(
        &output.destination,
        &contents,
        PrivateOutputOptions { replace, durable },
    )?;
    Ok(summary)
}

fn emit_success(
    arguments: &ReceiveArgs,
    output: &OutputPlan,
    summary: Option<DotenvMergeSummary>,
) -> Result<(), CliFailure> {
    if arguments.json {
        println!("{}", serde_json::json!({ "event": "received" }));
        return Ok(());
    }

    let message = match (&output.action, summary) {
        (OutputAction::Merge(DotenvMergeMode::ReplaceExisting), Some(summary)) => format!(
            "Environment updated · {} added · {} replaced",
            summary.added, summary.updated
        ),
        (OutputAction::Merge(DotenvMergeMode::KeepExisting), Some(summary)) => format!(
            "Environment updated · {} added · {} kept",
            summary.added, summary.kept
        ),
        _ => format!("Environment saved to {}", output.destination.display()),
    };
    presentation::show_receive_success(&message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_create_refuses_an_existing_file() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let destination = directory.path().join(".env");
        std::fs::write(&destination, b"EXISTING=1\n")?;

        let result = resolve_destination(destination, Some(ReceiveMode::Create), false);

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn requested_merge_is_selected_for_an_existing_file() -> Result<(), Box<dyn std::error::Error>>
    {
        let directory = tempfile::tempdir()?;
        let destination = directory.path().join(".env");
        std::fs::write(&destination, b"EXISTING=1\n")?;

        let output = resolve_destination(destination, Some(ReceiveMode::Merge), false)?;

        assert!(matches!(
            output.action,
            OutputAction::Merge(DotenvMergeMode::ReplaceExisting)
        ));
        Ok(())
    }
}
