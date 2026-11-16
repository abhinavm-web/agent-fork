use crate::error::AgentForkError;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CasStore {
    pub objects_dir: PathBuf,
    pub tmp_dir: PathBuf,
}

impl CasStore {
    pub fn init(storage_root: &Path) -> Result<Self, AgentForkError> {
        let objects_dir = storage_root.join("objects");
        let tmp_dir = storage_root.join("tmp");

        fs::create_dir_all(&objects_dir).map_err(|e| AgentForkError::IoWithPath {
            path: objects_dir.display().to_string(),
            source: e,
        })?;

        fs::create_dir_all(&tmp_dir).map_err(|e| AgentForkError::IoWithPath {
            path: tmp_dir.display().to_string(),
            source: e,
        })?;

        Ok(Self {
            objects_dir,
            tmp_dir,
        })
    }

    pub fn object_path(&self, hash: &str) -> PathBuf {
        if hash.len() < 2 {
            return self.objects_dir.join(hash);
        }
        let prefix = &hash[0..2];
        let file_name = &hash[2..];
        self.objects_dir.join(prefix).join(file_name)
    }

    pub fn object_exists(&self, hash: &str) -> bool {
        self.object_path(hash).is_file()
    }

    pub fn read_bytes(&self, hash: &str) -> Result<Vec<u8>, AgentForkError> {
        let path = self.object_path(hash);
        fs::read(&path).map_err(|e| AgentForkError::IoWithPath {
            path: path.display().to_string(),
            source: e,
        })
    }

    pub fn store_bytes(&self, bytes: &[u8], hash: &str) -> Result<PathBuf, AgentForkError> {
        let target_path = self.object_path(hash);

        if target_path.is_file() {
            return Ok(target_path);
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| AgentForkError::IoWithPath {
                path: parent.display().to_string(),
                source: e,
            })?;
        }

        let tmp_name = format!("{}.tmp", uuid::Uuid::new_v4());
        let tmp_path = self.tmp_dir.join(tmp_name);

        fs::write(&tmp_path, bytes).map_err(|e| AgentForkError::IoWithPath {
            path: tmp_path.display().to_string(),
            source: e,
        })?;

        // Atomic move into partitioned directory
        match fs::rename(&tmp_path, &target_path) {
            Ok(_) => Ok(target_path),
            Err(e) => {
                if target_path.is_file() {
                    let _ = fs::remove_file(&tmp_path);
                    Ok(target_path)
                } else {
                    let _ = fs::remove_file(&tmp_path);
                    Err(AgentForkError::IoWithPath {
                        path: target_path.display().to_string(),
                        source: e,
                    })
                }
            }
        }
    }

    pub fn store_file(&self, source_path: &Path, hash: &str) -> Result<PathBuf, AgentForkError> {
        let target_path = self.object_path(hash);
        if target_path.is_file() {
            return Ok(target_path);
        }

        let bytes = fs::read(source_path).map_err(|e| AgentForkError::IoWithPath {
            path: source_path.display().to_string(),
            source: e,
        })?;

        self.store_bytes(&bytes, hash)
    }
}
