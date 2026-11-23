use std::fs;
use std::path::Path;

use super::owner_only::protection_and_aces;

#[cfg(windows)]
pub(super) fn owner_sid_text() -> String {
    use windows_permissions::utilities::current_process_sid;
    use windows_permissions::wrappers::ConvertSidToStringSid;

    let sid = current_process_sid().unwrap();
    ConvertSidToStringSid(&sid)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[cfg(windows)]
pub(super) fn set_dacl(file: &Path, sddl: &str, protected: bool) {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_permissions::constants::{AccessRights, SeObjectType, SecurityInformation};
    use windows_permissions::wrappers::{
        ConvertStringSecurityDescriptorToSecurityDescriptor, SetSecurityInfo,
    };

    let descriptor = ConvertStringSecurityDescriptorToSecurityDescriptor(sddl).unwrap();
    let mut handle = fs::OpenOptions::new()
        .access_mode((AccessRights::ReadControl | AccessRights::WriteDac).bits())
        .custom_flags(super::owner_only::FILE_FLAG_BACKUP_SEMANTICS)
        .open(file)
        .unwrap();
    SetSecurityInfo(
        &mut handle,
        SeObjectType::SE_FILE_OBJECT,
        if protected {
            SecurityInformation::Dacl | SecurityInformation::ProtectedDacl
        } else {
            SecurityInformation::Dacl
        },
        None,
        None,
        descriptor.dacl(),
        None,
    )
    .unwrap();
}

#[cfg(windows)]
pub(super) fn assert_dacl_is(file: &Path, sddl: &str, protected: bool) {
    use windows_permissions::constants::{SeObjectType, SecurityInformation};
    use windows_permissions::wrappers::{
        ConvertStringSecurityDescriptorToSecurityDescriptor, GetNamedSecurityInfo,
    };

    let read = GetNamedSecurityInfo(
        file,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl,
    )
    .unwrap()
    .as_sddl()
    .unwrap()
    .to_string_lossy()
    .into_owned();
    let expected = ConvertStringSecurityDescriptorToSecurityDescriptor(sddl)
        .unwrap()
        .as_sddl()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        protection_and_aces(&read).map(|shape| shape.0),
        Some(protected),
        "{read}"
    );
    assert_eq!(explicit_aces(&read), explicit_aces(&expected), "{read}");
}

#[cfg(windows)]
fn explicit_aces(sddl: &str) -> Vec<&str> {
    let Some((_, aces)) = protection_and_aces(sddl) else {
        return Vec::new();
    };
    aces.split(')')
        .filter(|ace| !ace.is_empty() && !ace.contains(";ID;"))
        .collect()
}
