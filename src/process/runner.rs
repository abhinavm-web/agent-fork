use crate::error::AgentForkError;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ProcessRunnerConfig {
    pub command: Vec<String>,
    pub proxy_port: u16,
    pub session_id: String,
    pub working_dir: PathBuf,
    pub capture_output: bool,
}

#[derive(Debug, Clone)]
pub struct ProcessExecutionResult {
    pub exit_code: i32,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

pub async fn execute_agent_child(
    config: &ProcessRunnerConfig,
) -> Result<ProcessExecutionResult, AgentForkError> {
    if config.command.is_empty() {
        return Err(AgentForkError::InvalidArgument(
            "No command provided to execute".to_string(),
        ));
    }

    let program = &config.command[0];
    let args = &config.command[1..];

    let resolved_program = resolve_executable(program);

    let mut cmd = Command::new(&resolved_program);
    cmd.args(args)
        .current_dir(&config.working_dir)
        .envs(std::env::vars())
        .env("AGENT_FORK_ACTIVE", "1")
        .env("AGENT_FORK_SESSION_ID", &config.session_id)
        .env("AGENT_FORK_PROXY_PORT", config.proxy_port.to_string());

    if config.proxy_port > 0 {
        cmd.env(
            "ANTHROPIC_BASE_URL",
            format!("http://127.0.0.1:{}", config.proxy_port),
        )
        .env(
            "OPENAI_BASE_URL",
            format!("http://127.0.0.1:{}/v1", config.proxy_port),
        );
    }

    if config.capture_output {
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::inherit());

        let output = cmd.output().await.map_err(|e| AgentForkError::ProcessSpawnFailed {
            command: config.command.join(" "),
            source: e,
        })?;

        Ok(ProcessExecutionResult {
            exit_code: output.status.code().unwrap_or(1),
            stdout: Some(String::from_utf8_lossy(&output.stdout).to_string()),
            stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
        })
    } else {
        cmd.stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .stdin(Stdio::inherit());

        let mut child = cmd.spawn().map_err(|e| AgentForkError::ProcessSpawnFailed {
            command: config.command.join(" "),
            source: e,
        })?;

        let status = child.wait().await.map_err(|e| AgentForkError::ProcessSpawnFailed {
            command: config.command.join(" "),
            source: e,
        })?;

        Ok(ProcessExecutionResult {
            exit_code: status.code().unwrap_or(1),
            stdout: None,
            stderr: None,
        })
    }
}

pub fn resolve_executable(name: &str) -> String {
    #[cfg(windows)]
    {
        use std::path::Path;
        if Path::new(name).extension().is_some() {
            return name.to_string();
        }
        if let Ok(pathext) = std::env::var("PATHEXT") {
            for ext in pathext.split(';') {
                let candidate = format!("{}{}", name, ext.to_lowercase());
                if which_probe(&candidate) {
                    return candidate;
                }
            }
        }
    }
    name.to_string()
}

#[cfg(windows)]
fn which_probe(candidate: &str) -> bool {
    if std::path::Path::new(candidate).is_file() {
        return true;
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if dir.join(candidate).is_file() {
                return true;
            }
        }
    }
    false
}
