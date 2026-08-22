//! Direct child process construction without a shell.

use std::{collections::BTreeMap, ffi::OsString, process::ExitStatus};

#[cfg(windows)]
use process_wrap::tokio::JobObject;
#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::process::Command;

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

/// A child contained in an operating-system process group or job object.
pub type ManagedChild = Box<dyn ChildWrapper>;

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
) -> Result<ManagedChild, CoreError> {
    if program.is_empty() {
        return Err(CoreError::Configuration);
    }
    let mut command = Command::new(program);
    command.args(arguments);
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
    let mut command = CommandWrap::from(command);
    command.wrap(KillOnDrop);
    configure_process_group(&mut command);
    command.spawn().map_err(|_| CoreError::ChildProcess)
}

/// Waits for a child while forwarding a local interrupt request.
///
/// # Errors
///
/// Returns a safe process error when waiting or termination fails.
pub async fn wait_child_forwarding_interrupt(
    mut child: ManagedChild,
) -> Result<ExitStatus, CoreError> {
    tokio::select! {
        status = child.wait() => status.map_err(|_| CoreError::ChildProcess),
        interrupt = tokio::signal::ctrl_c() => {
            interrupt.map_err(|_| CoreError::ChildProcess)?;
            #[cfg(unix)]
            interrupt_child_tree(&mut child).await?;
            #[cfg(windows)]
            interrupt_child_tree(&mut child)?;
            child.wait().await.map_err(|_| CoreError::ChildProcess)
        }
    }
}

#[cfg(unix)]
fn configure_process_group(command: &mut CommandWrap) {
    command.wrap(ProcessGroup::leader());
}

#[cfg(windows)]
fn configure_process_group(command: &mut CommandWrap) {
    command.wrap(JobObject);
}

#[cfg(unix)]
async fn interrupt_child_tree(child: &mut ManagedChild) -> Result<(), CoreError> {
    use nix::sys::signal::Signal;

    child
        .signal(Signal::SIGINT as i32)
        .map_err(|_| CoreError::ChildProcess)?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    match child.start_kill() {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(nix::libc::ESRCH) => Ok(()),
        Err(_) => Err(CoreError::ChildProcess),
    }
}

#[cfg(windows)]
fn interrupt_child_tree(child: &mut ManagedChild) -> Result<(), CoreError> {
    child.start_kill().map_err(|_| CoreError::ChildProcess)
}

use super::ParsedEnvironment;

#[cfg(all(test, unix))]
mod tests {
    use std::{error::Error, path::Path, time::Duration};

    use nix::{errno::Errno, sys::signal::kill, unistd::Pid};

    use super::*;

    #[tokio::test]
    async fn killing_a_managed_child_terminates_its_process_group() -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let pid_path = directory.path().join("grandchild.pid");
        let environment = ParsedEnvironment::parse(b"")?;
        let arguments = vec![
            OsString::from("-c"),
            OsString::from("sleep 30 & echo $! > \"$1\"; wait"),
            OsString::from("envshare-process-test"),
            pid_path.as_os_str().to_owned(),
        ];
        let mut child = spawn_child(
            OsString::from("/bin/sh"),
            &arguments,
            &environment,
            EnvironmentMode::Overlay,
        )?;
        let grandchild = wait_for_pid(&pid_path).await?;

        child.start_kill()?;
        tokio::time::timeout(Duration::from_secs(5), child.wait()).await??;

        assert!(matches!(
            kill(Pid::from_raw(grandchild), None),
            Err(Errno::ESRCH)
        ));
        Ok(())
    }

    async fn wait_for_pid(path: &Path) -> Result<i32, Box<dyn Error>> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match std::fs::read_to_string(path) {
                    Ok(text) if !text.trim().is_empty() => {
                        return text.trim().parse::<i32>().map_err(Into::into);
                    }
                    Ok(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        })
        .await?
    }
}
