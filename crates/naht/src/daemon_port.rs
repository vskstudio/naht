use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;

const FILE: &str = "daemon.port";

pub(crate) fn path(root: &Path) -> PathBuf {
    root.join(".naht").join(FILE)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Recorded {
    pub(crate) port: u16,
    pub(crate) daemon_id: Option<String>,
}

pub(crate) fn write(root: &Path, port: u16, daemon_id: &str) -> io::Result<()> {
    let target = path(root);
    let temp = root
        .join(".naht")
        .join(format!("{FILE}.tmp-{}", process::id()));
    fs::write(&temp, format!("{port}\n{daemon_id}\n"))?;
    fs::rename(&temp, &target).inspect_err(|_| drop(fs::remove_file(&temp)))
}

pub(crate) fn read(root: &Path) -> Option<Recorded> {
    let contents = fs::read_to_string(path(root)).ok()?;
    let mut lines = contents.lines();
    let port = lines.next()?.trim().parse().ok()?;
    let daemon_id = lines
        .next()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned);
    Some(Recorded { port, daemon_id })
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
    use super::{Recorded, read, write};
    use std::fs;
    use std::process;

    #[test]
    fn a_failed_write_leaves_the_previous_port_file_intact() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".naht")).unwrap();
        write(dir.path(), 40000, "d-1").unwrap();
        let temp = dir
            .path()
            .join(".naht")
            .join(format!("daemon.port.tmp-{}", process::id()));
        fs::create_dir(&temp).unwrap();

        assert!(write(dir.path(), 40001, "d-2").is_err());
        assert_eq!(read(dir.path()), Some(recorded(40000, "d-1")));
    }

    #[test]
    fn a_port_file_reads_back_the_port_and_daemon_id_it_was_written_with() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".naht")).unwrap();
        write(dir.path(), 40000, "d-1").unwrap();

        assert_eq!(read(dir.path()), Some(recorded(40000, "d-1")));
    }

    #[test]
    fn a_port_file_written_by_an_older_naht_has_no_daemon_id() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".naht")).unwrap();
        fs::write(dir.path().join(".naht").join("daemon.port"), "40000").unwrap();

        assert_eq!(
            read(dir.path()),
            Some(Recorded {
                port: 40000,
                daemon_id: None,
            })
        );
    }

    fn recorded(port: u16, daemon_id: &str) -> Recorded {
        Recorded {
            port,
            daemon_id: Some(daemon_id.to_owned()),
        }
    }
}
