use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;

const FILE: &str = "daemon.port";

pub(crate) fn path(root: &Path) -> PathBuf {
    root.join(".naht").join(FILE)
}

pub(crate) fn write(root: &Path, port: u16) -> io::Result<()> {
    let target = path(root);
    let temp = root
        .join(".naht")
        .join(format!("{FILE}.tmp-{}", process::id()));
    fs::write(&temp, port.to_string())?;
    fs::rename(&temp, &target).inspect_err(|_| drop(fs::remove_file(&temp)))
}

pub(crate) fn read(root: &Path) -> Option<u16> {
    fs::read_to_string(path(root)).ok()?.trim().parse().ok()
}

pub(crate) fn remove(root: &Path) -> io::Result<()> {
    match fs::remove_file(path(root)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{read, write};
    use std::fs;
    use std::process;

    #[test]
    fn a_failed_write_leaves_the_previous_port_file_intact() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".naht")).unwrap();
        write(dir.path(), 40000).unwrap();
        let temp = dir
            .path()
            .join(".naht")
            .join(format!("daemon.port.tmp-{}", process::id()));
        fs::create_dir(&temp).unwrap();

        assert!(write(dir.path(), 40001).is_err());
        assert_eq!(read(dir.path()), Some(40000));
    }
}
