//! Interactive prompts and human-oriented terminal presentation.

use std::{
    fs,
    io::{self, IsTerminal as _},
    path::{Path, PathBuf},
};

use crate::{CliFailure, ExitCode};

#[derive(Clone, Debug, Eq, PartialEq)]
enum InputChoice {
    File(PathBuf),
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExistingDestinationChoice {
    Merge,
    AppendMissing,
    Replace,
    SaveElsewhere,
    Cancel,
}

pub(crate) struct SendView {
    spinner: Option<cliclack::ProgressBar>,
}

pub(crate) fn is_interactive() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

pub(crate) fn choose_sender_input() -> Result<PathBuf, CliFailure> {
    if !is_interactive() {
        return Err(CliFailure::new(
            ExitCode::Usage,
            "input file is required when not running in an interactive terminal",
        ));
    }

    let mut candidates = dotenv_candidates(Path::new("."))?;
    if candidates.is_empty() {
        return prompt_path("Path to environment file", ".env");
    }

    let mut prompt = cliclack::select("Choose an environment file");
    for path in candidates.drain(..) {
        let label = path.display().to_string();
        prompt = prompt.item(InputChoice::File(path), label, "");
    }
    prompt = prompt.item(InputChoice::Other, "Enter another path", "");
    match prompt
        .filter_mode()
        .max_rows(8)
        .interact()
        .map_err(|error| prompt_error(&error))?
    {
        InputChoice::File(path) => Ok(path),
        InputChoice::Other => prompt_path("Path to environment file", ".env"),
    }
}

pub(crate) fn prompt_share_code() -> Result<String, CliFailure> {
    cliclack::password("Enter share code")
        .mask('•')
        .validate(|value: &String| {
            value
                .trim()
                .parse::<code::ShareCode>()
                .map(|_| ())
                .map_err(|_| "enter a valid share code")
        })
        .interact()
        .map_err(|error| prompt_error(&error))
}

pub(crate) fn choose_existing_destination(
    destination: &Path,
) -> Result<ExistingDestinationChoice, CliFailure> {
    let prompt = format!("{} already exists", destination.display());
    cliclack::select(prompt)
        .item(
            ExistingDestinationChoice::Merge,
            "Merge",
            "update matching keys and add new keys",
        )
        .item(
            ExistingDestinationChoice::AppendMissing,
            "Add missing keys",
            "keep all existing values",
        )
        .item(
            ExistingDestinationChoice::SaveElsewhere,
            "Create a new file",
            "choose another destination",
        )
        .item(
            ExistingDestinationChoice::Replace,
            "Replace file",
            "overwrite the complete file",
        )
        .item(
            ExistingDestinationChoice::Cancel,
            "Cancel",
            "leave the share unclaimed",
        )
        .interact()
        .map_err(|error| prompt_error(&error))
}

pub(crate) fn prompt_new_destination() -> Result<PathBuf, CliFailure> {
    prompt_path("Save as", ".env.shared")
}

pub(crate) fn show_share(code: &str) -> Result<SendView, CliFailure> {
    if !io::stderr().is_terminal() {
        println!("Share code: {code}");
        return Ok(SendView { spinner: None });
    }

    cliclack::intro("Envshare").map_err(output_error)?;
    cliclack::note("Share code", code).map_err(output_error)?;
    let spinner = cliclack::spinner();
    spinner.start("Waiting for receiver…");
    Ok(SendView {
        spinner: Some(spinner),
    })
}

impl SendView {
    pub(crate) fn consumed(self) -> Result<(), CliFailure> {
        if let Some(spinner) = self.spinner {
            spinner.stop("Environment received");
            cliclack::outro("Share completed").map_err(output_error)?;
        } else {
            println!("Share consumed.");
        }
        Ok(())
    }

    pub(crate) fn cancel(self, message: &str) {
        if let Some(spinner) = self.spinner {
            spinner.cancel(message);
        }
    }
}

pub(crate) fn show_receive_success(message: &str) -> Result<(), CliFailure> {
    if io::stderr().is_terminal() {
        cliclack::outro(message).map_err(output_error)
    } else {
        println!("{message}");
        Ok(())
    }
}

fn dotenv_candidates(directory: &Path) -> Result<Vec<PathBuf>, CliFailure> {
    let entries = fs::read_dir(directory).map_err(|_| {
        CliFailure::new(ExitCode::Output, "could not inspect the current directory")
    })?;
    let mut candidates = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| PathBuf::from(entry.file_name()))
        .filter(|path| looks_like_dotenv(path))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|path| dotenv_rank(path));
    Ok(candidates)
}

fn looks_like_dotenv(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name == ".env"
        || name.starts_with(".env.")
        || Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("env"))
}

fn dotenv_rank(path: &Path) -> (u8, String) {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let rank = match name {
        ".env" => 0,
        ".env.local" => 1,
        _ => 2,
    };
    (rank, name.to_owned())
}

fn prompt_path(prompt: &str, default: &str) -> Result<PathBuf, CliFailure> {
    let value: String = cliclack::input(prompt)
        .default_input(default)
        .validate(|value: &String| {
            if value.trim().is_empty() {
                Err("enter a file path")
            } else {
                Ok(())
            }
        })
        .interact()
        .map_err(|error| prompt_error(&error))?;
    Ok(PathBuf::from(value.trim()))
}

fn prompt_error(error: &io::Error) -> CliFailure {
    if error.kind() == io::ErrorKind::Interrupted {
        CliFailure::new(ExitCode::Interrupted, "interrupted")
    } else {
        CliFailure::new(ExitCode::Usage, "interactive prompt failed")
    }
}

fn output_error(_: io::Error) -> CliFailure {
    CliFailure::new(ExitCode::Output, "terminal output failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_common_dotenv_names() {
        assert!(looks_like_dotenv(Path::new(".env")));
        assert!(looks_like_dotenv(Path::new(".env.production")));
        assert!(looks_like_dotenv(Path::new("service.env")));
        assert!(!looks_like_dotenv(Path::new("env.txt")));
    }

    #[test]
    fn ranks_default_files_first() {
        assert!(dotenv_rank(Path::new(".env")) < dotenv_rank(Path::new(".env.local")));
        assert!(dotenv_rank(Path::new(".env.local")) < dotenv_rank(Path::new("other.env")));
    }
}
