use crate::error::AgentForkError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub created_at: String,
    pub current_step_id: u64,
    pub active_branch: String,
    pub branches: Vec<String>,
}

impl SessionState {
    pub fn new(active_branch: &str) -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            current_step_id: 0,
            active_branch: active_branch.to_string(),
            branches: vec![active_branch.to_string()],
        }
    }

    pub fn load(path: &Path) -> Result<Self, AgentForkError> {
        let content = fs::read_to_string(path)?;
        let state: Self = serde_json::from_str(&content)?;
        Ok(state)
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
