use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;

use windows_permissions::constants::{AccessRights, SeObjectType, SecurityInformation};
use windows_permissions::utilities::current_process_sid;
use windows_permissions::wrappers::{
    ConvertSidToStringSid, ConvertStringSecurityDescriptorToSecurityDescriptor, GetSecurityInfo,
    SetSecurityInfo,
};
use windows_permissions::{LocalBox, SecurityDescriptor};

const FILE_DACL: &str = "D:P(A;;FA;;;{sid})";
const DIR_DACL: &str = "D:P(A;OICI;FA;;;{sid})";
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

pub(super) fn is_file_owner_only(path: &Path) -> io::Result<bool> {
    matches_dacl(path, FILE_DACL)
}

pub(super) fn is_dir_owner_only(path: &Path) -> io::Result<bool> {
    matches_dacl(path, DIR_DACL)
}

fn matches_dacl(path: &Path, template: &str) -> io::Result<bool> {
    let file = open_for_acl(path)?;
    let current = GetSecurityInfo(
        &file,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl,
    )?
    .as_sddl()?
    .to_string_lossy()
    .into_owned();
    let expected = owner_only_descriptor(template)?
        .as_sddl()?
        .to_string_lossy()
        .into_owned();
    let current_shape = protection_and_aces(&current);
    Ok(current_shape.is_some() && current_shape == protection_and_aces(&expected))
}

fn protection_and_aces(sddl: &str) -> Option<(bool, &str)> {
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
    let mut file = open_for_acl(path)?;
    apply_owner_only(&mut file, &descriptor)
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

fn remove_unfinished(path: &Path) {
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
