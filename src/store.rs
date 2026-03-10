use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::state::CrewGlueState;

const DEFAULT_FILE_NAME: &str = "crew_glue_state.v1.json";

#[derive(Debug, thiserror::Error)]
pub enum StateStoreError {
    #[error("failed to create state directory '{0}'")]
    CreateDir(PathBuf, #[source] io::Error),
    #[error("failed to read state file '{0}'")]
    ReadFile(PathBuf, #[source] io::Error),
    #[error("failed to write state file '{0}'")]
    WriteFile(PathBuf, #[source] io::Error),
    #[error("failed to parse state file '{0}'")]
    ParseFile(PathBuf, #[source] serde_json::Error),
    #[error("failed to serialize state file '{0}'")]
    SerializeFile(PathBuf, #[source] serde_json::Error),
}

/// File-backed storage for a user's glue state.
#[derive(Clone, Debug)]
pub struct StateStore {
    path: PathBuf,
}

impl StateStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn in_dir(dir: impl AsRef<Path>) -> Self {
        Self::new(dir.as_ref().join(DEFAULT_FILE_NAME))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_or_default(&self) -> Result<CrewGlueState, StateStoreError> {
        match fs::read_to_string(&self.path) {
            Ok(content) => serde_json::from_str(&content)
                .map_err(|error| StateStoreError::ParseFile(self.path.clone(), error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(CrewGlueState::default()),
            Err(error) => Err(StateStoreError::ReadFile(self.path.clone(), error)),
        }
    }

    pub fn save(&self, state: &CrewGlueState) -> Result<(), StateStoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| StateStoreError::CreateDir(parent.to_path_buf(), error))?;
        }
        let bytes = serde_json::to_vec_pretty(state)
            .map_err(|error| StateStoreError::SerializeFile(self.path.clone(), error))?;
        fs::write(&self.path, bytes)
            .map_err(|error| StateStoreError::WriteFile(self.path.clone(), error))
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::StateStore;

    #[test]
    fn missing_file_returns_default_state() {
        let temp = TempDir::new().unwrap();
        let store = StateStore::in_dir(temp.path());
        let state = store.load_or_default().unwrap();
        assert!(state.providers.is_empty());
    }
}
