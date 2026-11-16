use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    /// Normalized relative path with forward slashes, e.g. "src/lib.rs"
    pub path: String,
    /// 64-character lowercase hex BLAKE3 hash
    pub hash: String,
    /// File size in bytes
    pub size: u64,
    /// Modification timestamp in milliseconds since UNIX epoch
    pub mtime_ms: u64,
    /// Executable bit (POSIX 0o111 != 0, false on Windows)
    pub is_executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StepDelta {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepManifest {
    pub step_id: u64,
    pub session_id: String,
    pub branch: String,
    pub parent_step_id: Option<u64>,
    pub timestamp: String,
    pub command: Option<String>,
    pub exit_code: Option<i32>,
    pub description: String,
    pub delta: StepDelta,
    pub files: BTreeMap<String, FileEntry>,
}

impl StepManifest {
    pub fn new(
        step_id: u64,
        session_id: String,
        branch: String,
        parent_step_id: Option<u64>,
        description: String,
        command: Option<String>,
        exit_code: Option<i32>,
    ) -> Self {
        Self {
            step_id,
            session_id,
            branch,
            parent_step_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
            command,
            exit_code,
            description,
            delta: StepDelta::default(),
            files: BTreeMap::new(),
        }
    }
}
