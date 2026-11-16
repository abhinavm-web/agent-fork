use crate::cassette::types::{EditRecord, EditType, StepCassette};
use crate::error::AgentForkError;
use chrono::Utc;
use serde_json::Value;

pub fn override_prompt(
    cassette: &mut StepCassette,
    turn_index: usize,
    new_prompt: &str,
) -> Result<(), AgentForkError> {
    let entry = cassette.entries.get_mut(turn_index).ok_or_else(|| {
        AgentForkError::Cassette(format!("Turn index {} out of range", turn_index))
    })?;

    let old_sig = entry.signature.clone();

    // Mutate the request body JSON messages array
    if let Some(Value::Object(ref mut map)) = entry.request.body_json {
        if let Some(Value::Array(ref mut messages)) = map.get_mut("messages") {
            if let Some(last_user) = messages
                .iter_mut()
                .rev()
                .find(|m| m.get("role") == Some(&Value::String("user".to_string())))
            {
                last_user["content"] = Value::String(new_prompt.to_string());
            }
        }
    }

    // Recompute signature
    entry.signature = crate::cassette::signature::compute_request_signature(
        &entry.request.method,
        &entry.request.path,
        entry.request.body_json.as_ref(),
        None,
    );
    entry.is_edited = true;

    let record = EditRecord {
        edited_at: Utc::now(),
        edit_type: EditType::PromptOverride,
        description: format!("Prompt overridden at turn {}", turn_index),
        original_signature: old_sig,
        new_signature: entry.signature.clone(),
    };

    entry.edit_history.get_or_insert_with(Vec::new).push(record);
    Ok(())
}

pub fn override_tool_output(
    cassette: &mut StepCassette,
    turn_index: usize,
    tool_use_id: &str,
    mock_output: &str,
) -> Result<(), AgentForkError> {
    let entry = cassette.entries.get_mut(turn_index).ok_or_else(|| {
        AgentForkError::Cassette(format!("Turn index {} out of range", turn_index))
    })?;

    let old_sig = entry.signature.clone();

    // For Anthropic messages, tool_result blocks are inside user messages
    if let Some(Value::Object(ref mut map)) = entry.request.body_json {
        if let Some(Value::Array(ref mut messages)) = map.get_mut("messages") {
            for msg in messages.iter_mut() {
                if let Some(Value::Array(ref mut content_blocks)) = msg.get_mut("content") {
                    for block in content_blocks.iter_mut() {
                        if block.get("type") == Some(&Value::String("tool_result".to_string()))
                            && block.get("tool_use_id")
                                == Some(&Value::String(tool_use_id.to_string()))
                        {
                            block["content"] = Value::String(mock_output.to_string());
                        }
                    }
                }
            }
        }
    }

    entry.signature = crate::cassette::signature::compute_request_signature(
        &entry.request.method,
        &entry.request.path,
        entry.request.body_json.as_ref(),
        None,
    );
    entry.is_edited = true;

    let record = EditRecord {
        edited_at: Utc::now(),
        edit_type: EditType::ToolOutputOverride,
        description: format!("Tool output overridden for ID {}", tool_use_id),
        original_signature: old_sig,
        new_signature: entry.signature.clone(),
    };

    entry.edit_history.get_or_insert_with(Vec::new).push(record);
    Ok(())
}
