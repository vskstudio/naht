use std::process::ExitCode;

use naht::commands::CommandError;
use naht_core::reconciler::ReconcileError;

use crate::failure::Failure;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Exit {
    Success,
    DataError,
    NoInput,
    Unavailable,
    Software,
    OsError,
    CannotCreate,
    IoError,
    Config,
}

impl Exit {
    const fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::DataError => 65,
            Self::NoInput => 66,
            Self::Unavailable => 69,
            Self::Software => 70,
            Self::OsError => 71,
            Self::CannotCreate => 73,
            Self::IoError => 74,
            Self::Config => 78,
        }
    }
}

impl From<&Failure> for Exit {
    fn from(failure: &Failure) -> Self {
        match failure {
            Failure::Config(_) => Self::Config,
            Failure::Logging(_) => Self::Software,
            Failure::Terminal(_) => Self::IoError,
            Failure::Command(command) => Self::from(command),
        }
    }
}

impl From<&CommandError> for Exit {
    fn from(command: &CommandError) -> Self {
        match command {
            CommandError::ProjectExists { .. } | CommandError::Create { .. } => Self::CannotCreate,
            CommandError::Read { .. } | CommandError::ProjectDir { .. } => Self::NoInput,
            CommandError::Tree { .. }
            | CommandError::RojoParse { .. }
            | CommandError::Build(_)
            | CommandError::ReleaseMismatch { .. } => Self::DataError,
            CommandError::RenderConfig(_) => Self::Software,
            CommandError::Reconcile(ReconcileError::NonUtf8(_))
            | CommandError::Reconcile(ReconcileError::NotConflicted(_))
            | CommandError::Reconcile(ReconcileError::MarkersRemain(_)) => Self::DataError,
            CommandError::Reconcile(ReconcileError::State(_) | ReconcileError::Vfs(_))
            | CommandError::CreateDir { .. }
            | CommandError::Write { .. }
            | CommandError::StateOpen { .. }
            | CommandError::State(_)
            | CommandError::Serve(_)
            | CommandError::Output(_) => Self::IoError,
            CommandError::Config(_) | CommandError::Uploader(_) => Self::Config,
            CommandError::Watch(_) => Self::OsError,
            CommandError::DaemonUnreachable { .. }
            | CommandError::DaemonStatus { .. }
            | CommandError::Bind { .. } => Self::Unavailable,
        }
    }
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit.code())
    }
}
