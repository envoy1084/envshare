//! Stable CLI process contract for Envshare.

#![forbid(unsafe_code)]

mod args;
mod commands;
mod failure;

pub use args::Cli;
pub use failure::CliFailure;

/// Stable process exit codes used by human and JSON workflows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ExitCode {
    /// Command completed successfully.
    Success = 0,
    /// Command-line arguments were invalid.
    Usage = 2,
    /// The capability text failed local validation.
    InvalidCode = 10,
    /// No sender could authenticate the capability.
    NotFoundOrUnauthorized = 11,
    /// The share expired or cannot be claimed.
    ShareUnavailable = 12,
    /// Discovery or connectivity failed.
    Network = 13,
    /// Protocol authentication or transfer failed.
    Transfer = 14,
    /// Private atomic output failed.
    Output = 15,
    /// A requested child process could not start.
    ChildStart = 16,
    /// Local configuration was invalid.
    Configuration = 20,
    /// An internal invariant failed.
    Internal = 70,
    /// The command was interrupted.
    Interrupted = 130,
}

impl ExitCode {
    /// Returns the numeric status passed to the operating system.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

/// Executes the parsed command and returns the desired process status.
///
/// # Errors
///
/// Returns a secret-safe CLI failure with a stable exit classification.
pub async fn run(cli: Cli) -> Result<i32, CliFailure> {
    commands::run(cli).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_exit_codes_match_the_cli_contract() {
        assert_eq!(ExitCode::Success.as_i32(), 0);
        assert_eq!(ExitCode::InvalidCode.as_i32(), 10);
        assert_eq!(ExitCode::Interrupted.as_i32(), 130);
    }
}
