use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FILE: &str = "daemon.port";

pub(crate) fn path(root: &Path) -> PathBuf {
    root.join(".naht").join(FILE)
}

pub(crate) fn write(root: &Path, port: u16) -> io::Result<()> {
    fs::write(path(root), port.to_string())
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
