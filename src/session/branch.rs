use crate::error::AgentForkError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchMetadata {
    pub name: String,
    pub base_step_id: u64,
    pub head_step_id: u64,
    pub description: String,
    pub worktree_path: String,
    pub created_at: String,
    pub is_active: bool,
}

impl BranchMetadata {
    pub fn new(name: &str, base_step_id: u64, description: &str, worktree_path: &str) -> Self {
        Self {
            name: name.to_string(),
            base_step_id,
            head_step_id: base_step_id,
            description: description.to_string(),
            worktree_path: worktree_path.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            is_active: true,
        }
    }

    pub fn load(path: &Path) -> Result<Self, AgentForkError> {
        let content = fs::read_to_string(path)?;
        let meta: Self = serde_json::from_str(&content)?;
        Ok(meta)
    }

    pub fn save(&self, path: &Path) -> Result<(), AgentForkError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}
