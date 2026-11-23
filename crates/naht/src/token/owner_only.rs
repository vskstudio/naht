use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process;

use windows_permissions::constants::{AccessRights, SeObjectType, SecurityInformation};
use windows_permissions::utilities::current_process_sid;
use windows_permissions::wrappers::{
    ConvertSidToStringSid, ConvertStringSecurityDescriptorToSecurityDescriptor, GetSecurityInfo,
    SetSecurityInfo,
};
use windows_permissions::{LocalBox, SecurityDescriptor};

const OWNER_ONLY_DACL: &str = "D:P(A;;FA;;;{sid})";

pub(super) fn is_owner_only(path: &Path) -> io::Result<bool> {
    let file = open_for_acl(path)?;
    let current = GetSecurityInfo(
        &file,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl,
    )?;
    Ok(current.as_sddl()? == owner_only_descriptor()?.as_sddl()?)
}

pub(super) fn restrict(path: &Path) -> io::Result<()> {
    let descriptor = owner_only_descriptor()?;
    let mut file = open_for_acl(path)?;
    apply_owner_only(&mut file, &descriptor)
}

pub(super) fn write_new(path: &Path, text: &str) -> io::Result<()> {
    let descriptor = owner_only_descriptor()?;
    let staging = staging_path(path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .access_mode((AccessRights::FileGenericWrite | AccessRights::WriteDac).bits())
        .open(&staging)?;
    let written = apply_owner_only(&mut file, &descriptor)
        .and_then(|()| file.write_all(text.as_bytes()))
        .and_then(|()| file.sync_all());
    drop(file);
    match written.and_then(|()| fs::rename(&staging, path)) {
        Ok(()) => Ok(()),
        Err(error) => {
            discard(&staging);
            Err(error)
        }
    }
}

fn staging_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(".token.{}.tmp", process::id()))
}

fn discard(staging: &Path) {
    if let Err(error) = fs::remove_file(staging) {
        tracing::warn!(
            target: "naht::token",
            path = %staging.display(),
            %error,
            "could not remove a staged token file"
        );
    }
}

fn open_for_acl(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .access_mode((AccessRights::ReadControl | AccessRights::WriteDac).bits())
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

fn owner_only_descriptor() -> io::Result<LocalBox<SecurityDescriptor>> {
    let owner = current_process_sid()?;
    let sid = ConvertSidToStringSid(&owner)?;
    let sddl = OWNER_ONLY_DACL.replace("{sid}", &sid.to_string_lossy());
    ConvertStringSecurityDescriptorToSecurityDescriptor(&sddl)
}
