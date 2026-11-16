use crate::cassette::types::StepCassette;
use crate::error::AgentForkError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRecord {
    pub id: String,
    pub request_prompt: String,
    pub response_body: String,
    pub is_edited: bool,
}

#[derive(Debug, Clone)]
pub struct CassetteStore {
    pub root_dir: PathBuf,
}

impl CassetteStore {
    pub fn new(root_dir: &Path) -> Self {
        Self {
            root_dir: root_dir.to_path_buf(),
        }
    }

    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        self.root_dir.join(session_id)
    }

    pub fn step_file(&self, session_id: &str, step_id: u64) -> PathBuf {
        self.session_dir(session_id).join(format!("{}.json", step_id))
    }

    pub fn save_step_cassette(&self, cassette: &StepCassette) -> Result<(), AgentForkError> {
        let path = self.step_file(&cassette.session_id, cassette.step_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
        let json_data = serde_json::to_string_pretty(cassette)?;
        fs::write(&tmp_path, json_data)?;
        fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    pub fn load_step_cassette(
        &self,
        session_id: &str,
        step_id: u64,
    ) -> Result<Option<StepCassette>, AgentForkError> {
        let path = self.step_file(session_id, step_id);
        if !path.is_file() {
            return Ok(None);
        }
        let data = fs::read_to_string(&path)?;
        let cassette: StepCassette = serde_json::from_str(&data)?;
        Ok(Some(cassette))
    }

    pub fn list_cassettes(
        &self,
        session_id: &str,
        step_filter: Option<u64>,
    ) -> Result<Vec<StepCassette>, AgentForkError> {
        let mut list = Vec::new();
        let sdir = self.session_dir(session_id);
        if sdir.is_dir() {
            for entry in fs::read_dir(sdir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(data) = fs::read_to_string(&path) {
                        if let Ok(cassette) = serde_json::from_str::<StepCassette>(&data) {
                            if let Some(step) = step_filter {
                                if cassette.step_id != step {
                                    continue;
                                }
                            }
                            list.push(cassette);
                        }
                    }
                }
            }
        }
        list.sort_by_key(|c| c.step_id);
        Ok(list)
    }

    pub fn clear_all(&self, session_id: &str) -> Result<(), AgentForkError> {
        let sdir = self.session_dir(session_id);
        if sdir.is_dir() {
            fs::remove_dir_all(&sdir)?;
        }
        Ok(())
    }

    // Turn-level helpers for quick inspection and turn editing
    pub fn save_turn(
        &self,
        id: &str,
        prompt: &str,
        response: &str,
    ) -> Result<(), AgentForkError> {
        let turns_dir = self.root_dir.join("turns");
        fs::create_dir_all(&turns_dir)?;
        let path = turns_dir.join(format!("{}.json", id));
        let record = TurnRecord {
            id: id.to_string(),
            request_prompt: prompt.to_string(),
            response_body: response.to_string(),
            is_edited: false,
        };
        let json = serde_json::to_string_pretty(&record)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_turn(&self, id: &str) -> Result<TurnRecord, AgentForkError> {
        let path = self.root_dir.join("turns").join(format!("{}.json", id));
        if !path.is_file() {
            return Err(AgentForkError::CassetteNotFound(id.to_string()));
        }
        let data = fs::read_to_string(path)?;
        let record: TurnRecord = serde_json::from_str(&data)?;
        Ok(record)
    }

    pub fn edit_turn(
        &self,
        id: &str,
        prompt: Option<&str>,
        response: Option<&str>,
    ) -> Result<(), AgentForkError> {
        let mut turn = self.load_turn(id)?;
        if let Some(p) = prompt {
            turn.request_prompt = p.to_string();
            turn.is_edited = true;
        }
        if let Some(r) = response {
            turn.response_body = r.to_string();
            turn.is_edited = true;
        }
        let path = self.root_dir.join("turns").join(format!("{}.json", id));
        let json = serde_json::to_string_pretty(&turn)?;
        fs::write(path, json)?;
        Ok(())
    }
}
