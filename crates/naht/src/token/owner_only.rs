use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;

use windows_permissions::constants::{AccessRights, SeObjectType, SecurityInformation};
use windows_permissions::utilities::current_process_sid;
use windows_permissions::wrappers::{
    ConvertSidToStringSid, ConvertStringSecurityDescriptorToSecurityDescriptor,
    GetNamedSecurityInfo, GetSecurityInfo, SetNamedSecurityInfo, SetSecurityInfo,
};
use windows_permissions::{LocalBox, SecurityDescriptor};

const FILE_DACL: &str = "D:P(A;;FA;;;{sid})";
const DIR_DACL: &str = "D:P(A;OICI;FA;;;{sid})";
pub(super) const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

pub(super) fn is_file_owner_only(path: &Path) -> io::Result<bool> {
    matches_dacl(path, FILE_DACL)
}

pub(super) fn is_dir_owner_only(path: &Path) -> io::Result<bool> {
    matches_dacl(path, DIR_DACL)
}

fn matches_dacl(path: &Path, template: &str) -> io::Result<bool> {
    let current = read_dacl(path)?.as_sddl()?.to_string_lossy().into_owned();
    let expected = owner_only_descriptor(template)?
        .as_sddl()?
        .to_string_lossy()
        .into_owned();
    let current_shape = protection_and_aces(&current);
    Ok(current_shape.is_some() && current_shape == protection_and_aces(&expected))
}

pub(super) fn protection_and_aces(sddl: &str) -> Option<(bool, &str)> {
    let body = sddl.strip_prefix("D:")?;
    let (flags, aces) = body.split_at_checked(body.find('(')?)?;
    Some((flags.contains('P'), aces))
}

pub(super) fn restrict_file(path: &Path) -> io::Result<()> {
    restrict(path, FILE_DACL)
}

pub(super) fn restrict_dir(path: &Path) -> io::Result<()> {
    restrict(path, DIR_DACL)
}

fn restrict(path: &Path, template: &str) -> io::Result<()> {
    let descriptor = owner_only_descriptor(template)?;
    match open_for_acl(path) {
        Ok(mut file) => apply_owner_only(&mut file, &descriptor),
        Err(error) if is_denied(&error) => {
            apply_owner_only_by_name(path, &descriptor).map_err(|_| error)
        }
        Err(error) => Err(error),
    }
}

fn read_dacl(path: &Path) -> io::Result<LocalBox<SecurityDescriptor>> {
    match open_for_acl(path) {
        Ok(file) => GetSecurityInfo(
            &file,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Dacl,
        ),
        Err(error) if is_denied(&error) => GetNamedSecurityInfo(
            path.as_os_str(),
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Dacl,
        )
        .map_err(|_| error),
        Err(error) => Err(error),
    }
}

/// Whether an open was denied. A deny rule can refuse the synchronize and read-attributes rights
/// every handle open asks for while the read-control and write-DAC rights the owner holds remain,
/// so a denied open is retried through the path, and only a refusal there stands.
fn is_denied(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied
}

fn apply_owner_only_by_name(path: &Path, descriptor: &SecurityDescriptor) -> io::Result<()> {
    let dacl = descriptor
        .dacl()
        .ok_or_else(|| io::Error::other("the owner-only descriptor has no DACL"))?;
    SetNamedSecurityInfo(
        path.as_os_str(),
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
        None,
        None,
        Some(dacl),
        None,
    )
}

pub(super) fn write_new(path: &Path, text: &str) -> io::Result<()> {
    let mut file = create(path)?;
    let written = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = written {
        remove_unfinished(path);
        return Err(error);
    }
    Ok(())
}

pub(super) fn create(path: &Path) -> io::Result<File> {
    let descriptor = owner_only_descriptor(FILE_DACL)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .access_mode((AccessRights::FileGenericWrite | AccessRights::WriteDac).bits())
        .open(path)?;
    match apply_owner_only(&mut file, &descriptor) {
        Ok(()) => Ok(file),
        Err(error) => {
            drop(file);
            remove_unfinished(path);
            Err(error)
        }
    }
}

pub(super) fn remove_unfinished(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        tracing::warn!(
            target: "naht::token",
            path = %path.display(),
            %error,
            "could not remove an unfinished token file"
        );
    }
}

fn open_for_acl(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .access_mode((AccessRights::ReadControl | AccessRights::WriteDac).bits())
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

fn apply_owner_only(file: &mut File, descriptor: &SecurityDescriptor) -> io::Result<()> {
    let dacl = descriptor
        .dacl()
        .ok_or_else(|| io::Error::other("the owner-only descriptor has no DACL"))?;
    SetSecurityInfo(
        file,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
        None,
        None,
        Some(dacl),
        None,
    )
}

fn owner_only_descriptor(template: &str) -> io::Result<LocalBox<SecurityDescriptor>> {
    let owner = current_process_sid()?;
    let sid = ConvertSidToStringSid(&owner)?;
    let sddl = template.replace("{sid}", &sid.to_string_lossy());
    ConvertStringSecurityDescriptorToSecurityDescriptor(&sddl)
}
