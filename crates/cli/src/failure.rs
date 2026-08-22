//! Secret-safe command failures.

use app_core::CoreError;

use crate::ExitCode;

/// Safe user-facing command failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CliFailure {
    exit_code: ExitCode,
    message: &'static str,
}

impl CliFailure {
    pub(crate) const fn new(exit_code: ExitCode, message: &'static str) -> Self {
        Self { exit_code, message }
    }

    /// Returns the stable process classification.
    #[must_use]
    pub const fn exit_code(self) -> ExitCode {
        self.exit_code
    }
}

impl From<CoreError> for CliFailure {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::InvalidCode => Self::new(ExitCode::InvalidCode, "invalid share code"),
            CoreError::NotFoundOrUnauthorized => Self::new(
                ExitCode::NotFoundOrUnauthorized,
                "share not found or unauthorized",
            ),
            CoreError::ShareUnavailable => {
                Self::new(ExitCode::ShareUnavailable, "share is unavailable")
            }
            CoreError::Network => Self::new(ExitCode::Network, "network operation failed"),
            CoreError::Transfer => Self::new(ExitCode::Transfer, "secure transfer failed"),
            CoreError::Output => Self::new(ExitCode::Output, "private output failed"),
            CoreError::ChildProcess => {
                Self::new(ExitCode::ChildStart, "child process operation failed")
            }
            CoreError::Configuration => {
                Self::new(ExitCode::Configuration, "configuration is invalid")
            }
            CoreError::Internal => Self::new(ExitCode::Internal, "internal software error"),
        }
    }
}

impl std::fmt::Display for CliFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for CliFailure {}
