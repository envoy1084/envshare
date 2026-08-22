//! Private atomic file output.

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use crate::CoreError;

/// Safety options for private output persistence.
#[derive(Clone, Copy, Debug, Default)]
pub struct PrivateOutputOptions {
    /// Atomically replace an existing regular file when true.
    pub replace: bool,
    /// Flush file content to stable storage before persistence.
    pub durable: bool,
}

/// Writes bytes through a private temporary file in the destination directory.
///
/// The destination must be a direct child of an existing directory. Existing
/// symlinks, directories, and special files are always refused. Without
/// `replace`, persistence uses an atomic no-clobber rename.
///
/// # Errors
///
/// Returns a secret-safe output error for validation, permission, write, flush,
/// or persistence failure.
pub fn write_private_atomic(
    destination: &Path,
    payload: &[u8],
    options: PrivateOutputOptions,
) -> Result<(), CoreError> {
    let file_name = destination.file_name().ok_or(CoreError::Output)?;
    if file_name.is_empty() {
        return Err(CoreError::Output);
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let canonical_parent = parent.canonicalize().map_err(|_| CoreError::Output)?;
    let safe_destination: PathBuf = canonical_parent.join(file_name);
    validate_destination(&safe_destination, options.replace)?;

    let mut temporary =
        tempfile::NamedTempFile::new_in(&canonical_parent).map_err(|_| CoreError::Output)?;
    make_private(temporary.as_file()).map_err(|_| CoreError::Output)?;
    temporary
        .write_all(payload)
        .and_then(|()| temporary.flush())
        .map_err(|_| CoreError::Output)?;
    if options.durable {
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| CoreError::Output)?;
    }
    if options.replace {
        temporary
            .persist(&safe_destination)
            .map_err(|_| CoreError::Output)?;
    } else {
        temporary
            .persist_noclobber(&safe_destination)
            .map_err(|_| CoreError::Output)?;
    }
    if options.durable {
        sync_parent(&canonical_parent)?;
    }
    Ok(())
}

fn validate_destination(destination: &Path, replace: bool) -> Result<(), CoreError> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) => {
            if !replace || !metadata.file_type().is_file() || is_reparse_or_symlink(&metadata) {
                return Err(CoreError::Output);
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(CoreError::Output),
    }
}

#[cfg(unix)]
fn make_private(file: &File) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(windows)]
fn make_private(file: &File) -> std::io::Result<()> {
    use windows_permissions::{LocalBox, SecurityDescriptor, WindowsSecure};

    let current_user = windows_permissions::utilities::current_process_sid()?;
    let descriptor =
        format!("D:P(A;;FA;;;{current_user})").parse::<LocalBox<SecurityDescriptor>>()?;
    let dacl = descriptor.dacl().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, "private DACL missing")
    })?;
    file.try_clone()?.set_dacl(dacl)
}

#[cfg(unix)]
fn is_reparse_or_symlink(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(windows)]
fn is_reparse_or_symlink(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), CoreError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| CoreError::Output)
}

#[cfg(windows)]
fn sync_parent(_parent: &Path) -> Result<(), CoreError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_clobber_and_explicit_replacement_are_atomic() -> Result<(), CoreError> {
        let directory = tempfile::tempdir().map_err(|_| CoreError::Output)?;
        let destination = directory.path().join("received.env");
        write_private_atomic(&destination, b"A=1\n", PrivateOutputOptions::default())?;
        assert!(
            write_private_atomic(&destination, b"A=2\n", PrivateOutputOptions::default()).is_err()
        );
        write_private_atomic(
            &destination,
            b"A=2\n",
            PrivateOutputOptions {
                replace: true,
                durable: true,
            },
        )?;
        assert_eq!(
            fs::read(destination).map_err(|_| CoreError::Output)?,
            b"A=2\n"
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn unix_output_is_owner_read_write_only() -> Result<(), CoreError> {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().map_err(|_| CoreError::Output)?;
        let destination = directory.path().join("private.env");
        write_private_atomic(
            &destination,
            b"SECRET=sentinel\n",
            PrivateOutputOptions::default(),
        )?;
        let mode = fs::metadata(destination)
            .map_err(|_| CoreError::Output)?
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        Ok(())
    }
}
