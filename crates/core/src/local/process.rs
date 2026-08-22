//! Direct child process construction without a shell.

use std::{collections::BTreeMap, ffi::OsString};

use std::process::ExitStatus;

use tokio::process::{Child, Command};

use crate::CoreError;

/// How received variables interact with the inherited process environment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EnvironmentMode {
    /// Inherit the current environment and add only variables that are absent.
    #[default]
    Overlay,
    /// Inherit the current environment and replace matching variables.
    Override,
    /// Clear the inherited environment and use only received variables.
    Clean,
}

/// Spawns a program directly with a bounded parsed environment.
///
/// No shell is involved. On Unix the child starts in its own process group, and
/// dropping the returned handle requests termination on all platforms.
///
/// # Errors
///
/// Returns a safe child-start error for an empty program, invalid environment,
/// or operating-system spawn failure.
pub fn spawn_child(
    program: OsString,
    arguments: &[OsString],
    environment: &ParsedEnvironment,
    mode: EnvironmentMode,
) -> Result<Child, CoreError> {
    if program.is_empty() {
        return Err(CoreError::Configuration);
    }
    let mut command = Command::new(program);
    command.args(arguments).kill_on_drop(true);
    match mode {
        EnvironmentMode::Overlay => {
            let inherited: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
            for (key, value) in environment.variables() {
                if !inherited.contains_key(&OsString::from(key)) {
                    command.env(key, value);
                }
            }
        }
        EnvironmentMode::Override => {
            command.envs(environment.variables());
        }
        EnvironmentMode::Clean => {
            command.env_clear().envs(environment.variables());
        }
    }
    configure_process_group(&mut command);
    command.spawn().map_err(|_| CoreError::ChildProcess)
}

/// Waits for a child while forwarding a local interrupt request.
///
/// # Errors
///
/// Returns a safe process error when waiting or termination fails.
pub async fn wait_child_forwarding_interrupt(mut child: Child) -> Result<ExitStatus, CoreError> {
    tokio::select! {
        status = child.wait() => status.map_err(|_| CoreError::ChildProcess),
        interrupt = tokio::signal::ctrl_c() => {
            interrupt.map_err(|_| CoreError::ChildProcess)?;
            interrupt_child(&mut child)?;
            child.wait().await.map_err(|_| CoreError::ChildProcess)
        }
    }
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    command.process_group(0);
}

#[cfg(windows)]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn interrupt_child(child: &mut Child) -> Result<(), CoreError> {
    use nix::{
        sys::signal::{Signal, killpg},
        unistd::Pid,
    };

    let process_id = child.id().ok_or(CoreError::ChildProcess)?;
    let process_id = i32::try_from(process_id).map_err(|_| CoreError::ChildProcess)?;
    killpg(Pid::from_raw(process_id), Signal::SIGINT).map_err(|_| CoreError::ChildProcess)
}

#[cfg(windows)]
fn interrupt_child(child: &mut Child) -> Result<(), CoreError> {
    child.start_kill().map_err(|_| CoreError::ChildProcess)
}

use super::ParsedEnvironment;
