pub mod cassette;
pub mod cli;
pub mod cow;
pub mod error;
pub mod process;
pub mod session;

pub use cli::{CassetteAction, Cli, Commands};
pub use cow::CowEngine;
pub use error::AgentForkError;

use colored::Colorize;

pub async fn run_app(cli: Cli) -> Result<i32, AgentForkError> {
    let workspace_root = match cli.workspace {
        Some(w) => w,
        None => std::env::current_dir()?,
    };

    match cli.command {
        Commands::Run {
            message,
            no_proxy,
            proxy_port,
            upstream_anthropic,
            upstream_openai: _,
            command,
        } => {
            let mut engine = match CowEngine::open(&workspace_root) {
                Ok(e) => e,
                Err(_) => CowEngine::init(&workspace_root)?,
            };

            let session_id = engine.session.session_id.clone();
            let next_step_id = engine.session.current_step_id + 1;
            let active_branch = engine.session.active_branch.clone();

            let (proxy, effective_port) = if !no_proxy {
                let cassettes_dir = engine.agent_fork_dir.join("cassettes");
                let store = cassette::store::CassetteStore::new(&cassettes_dir);
                let p = cassette::proxy::CassetteProxy::start_with_config(
                    &session_id,
                    next_step_id,
                    &active_branch,
                    cassette::proxy::ProxyMode::Record,
                    proxy_port,
                    &upstream_anthropic,
                    &store,
                )
                .await?;
                let port = p.port();
                (Some(p), port)
            } else {
                (None, 0)
            };

            let runner_config = process::runner::ProcessRunnerConfig {
                command: command.clone(),
                proxy_port: effective_port,
                session_id,
                working_dir: workspace_root.clone(),
                capture_output: false,
            };

            let result = process::runner::execute_agent_child(&runner_config).await;

            if let Some(p) = proxy {
                p.shutdown().await;
            }

            let exec_result = result?;

            let desc = message.unwrap_or_else(|| format!("Run: {}", command.join(" ")));
            let manifest = engine.checkpoint_with_cmd(
                &desc,
                Some(command.join(" ")),
                Some(exec_result.exit_code),
            )?;

            if !cli.quiet {
                println!(
                    "{} step {}: {} (+{} ~{} -{})",
                    "Checkpointed".bold().green(),
                    manifest.step_id.to_string().cyan(),
                    manifest.description,
                    manifest.delta.added.len().to_string().green(),
                    manifest.delta.modified.len().to_string().yellow(),
                    manifest.delta.deleted.len().to_string().red()
                );
            }

            Ok(exec_result.exit_code)
        }

        Commands::Checkpoint { message } => {
            let mut engine = match CowEngine::open(&workspace_root) {
                Ok(e) => e,
                Err(_) => CowEngine::init(&workspace_root)?,
            };

            let manifest = engine.checkpoint(&message)?;

            if !cli.quiet {
                println!(
                    "{} step {}: {} (+{} ~{} -{})",
                    "Checkpointed".bold().green(),
                    manifest.step_id.to_string().cyan(),
                    manifest.description,
                    manifest.delta.added.len().to_string().green(),
                    manifest.delta.modified.len().to_string().yellow(),
                    manifest.delta.deleted.len().to_string().red()
                );
            }

            Ok(0)
        }

        Commands::Log { limit, json } => {
            let engine = CowEngine::open(&workspace_root)?;
            let current_step = engine.session.current_step_id;

            let mut manifests = Vec::new();
            let start = if current_step >= limit as u64 {
                current_step - limit as u64 + 1
            } else {
                0
            };

            for step_id in (start..=current_step).rev() {
                if let Ok(m) = engine.load_manifest(step_id) {
                    manifests.push(m);
                }
            }

            if json {
                println!("{}", serde_json::to_string_pretty(&manifests)?);
            } else {
                for m in &manifests {
                    println!(
                        "{} {} [{}] - {}",
                        "step".bold().cyan(),
                        m.step_id.to_string().bold().cyan(),
                        m.branch.yellow(),
                        m.description.bold()
                    );
                    println!(
                        "  Timestamp: {} | Delta: +{} ~{} -{}",
                        m.timestamp,
                        m.delta.added.len().to_string().green(),
                        m.delta.modified.len().to_string().yellow(),
                        m.delta.deleted.len().to_string().red()
                    );
                    if let Some(ref cmd) = m.command {
                        println!(
                            "  Command: {} (exit: {})",
                            cmd,
                            m.exit_code.unwrap_or(0)
                        );
                    }
                    println!();
                }
            }

            Ok(0)
        }

        Commands::Diff {
            step_a,
            step_b,
            stat,
            name_only,
        } => {
            let engine = CowEngine::open(&workspace_root)?;
            let options = cow::diff::DiffOptions {
                stat,
                name_only,
                color: true,
            };

            let (old_manifest, new_manifest) = match (step_a, step_b) {
                (Some(a), Some(b)) => {
                    let id_a: u64 = a.parse().map_err(|_| {
                        AgentForkError::InvalidArgument(format!("Invalid step ID: {}", a))
                    })?;
                    let id_b: u64 = b.parse().map_err(|_| {
                        AgentForkError::InvalidArgument(format!("Invalid step ID: {}", b))
                    })?;
                    let m_a = engine.load_manifest(id_a)?;
                    let m_b = engine.load_manifest(id_b)?;
                    (Some(m_a), Some(m_b))
                }
                (Some(a), None) => {
                    let id_a: u64 = a.parse().map_err(|_| {
                        AgentForkError::InvalidArgument(format!("Invalid step ID: {}", a))
                    })?;
                    let m_a = engine.load_manifest(id_a)?;
                    (Some(m_a), None)
                }
                (None, None) => {
                    let curr = engine.session.current_step_id;
                    let m_a = engine.load_manifest(curr)?;
                    (Some(m_a), None)
                }
                _ => {
                    return Err(AgentForkError::InvalidArgument(
                        "Invalid combination of diff arguments".to_string(),
                    ));
                }
            };

            let diff_text = cow::diff::render_manifest_diff(
                old_manifest.as_ref(),
                new_manifest.as_ref(),
                &workspace_root,
                &engine.cas,
                &options,
            )?;

            print!("{}", diff_text);
            Ok(0)
        }

        Commands::Rewind { step, force, clean: _ } => {
            let mut engine = CowEngine::open(&workspace_root)?;
            let target_step_id: u64 = step.parse().map_err(|_| {
                AgentForkError::InvalidArgument(format!("Invalid step ID: {}", step))
            })?;

            engine.rewind(target_step_id, force)?;

            if !cli.quiet {
                println!(
                    "{} workspace to step {}",
                    "Rewound".bold().green(),
                    target_step_id.to_string().cyan()
                );
            }

            Ok(0)
        }

        Commands::Fork { step, variants } => {
            let mut engine = CowEngine::open(&workspace_root)?;
            let base_step_id: u64 = step.parse().map_err(|_| {
                AgentForkError::InvalidArgument(format!("Invalid step ID: {}", step))
            })?;

            let variants_refs: Vec<&str> = variants.iter().map(|s| s.as_str()).collect();
            engine.fork(base_step_id, &variants_refs)?;

            if !cli.quiet {
                println!(
                    "{} {} branch variants from step {}: {}",
                    "Forked".bold().green(),
                    variants.len().to_string().cyan(),
                    base_step_id.to_string().cyan(),
                    variants.join(", ").yellow()
                );
            }

            Ok(0)
        }

        Commands::Branches { json } => {
            let engine = CowEngine::open(&workspace_root)?;
            let branches = engine.list_branches()?;

            if json {
                println!("{}", serde_json::to_string_pretty(&branches)?);
            } else {
                for b in &branches {
                    let indicator = if b.name == engine.session.active_branch {
                        "* ".bold().green().to_string()
                    } else {
                        "  ".to_string()
                    };
                    println!(
                        "{}{} (head: step {}, base: step {})",
                        indicator,
                        b.name.bold(),
                        b.head_step_id.to_string().cyan(),
                        b.base_step_id.to_string().cyan()
                    );
                    if !b.worktree_path.is_empty() {
                        println!("    Worktree: {}", b.worktree_path);
                    }
                }
            }

            Ok(0)
        }

        Commands::Promote { branch, force } => {
            let mut engine = CowEngine::open(&workspace_root)?;
            let manifest = engine.promote(&branch, force)?;

            if !cli.quiet {
                println!(
                    "{} branch '{}' into main at step {}",
                    "Promoted".bold().green(),
                    branch.yellow(),
                    manifest.step_id.to_string().cyan()
                );
            }

            Ok(0)
        }

        Commands::Cassette { action } => {
            let engine = CowEngine::open(&workspace_root)?;
            let cassettes_dir = engine.agent_fork_dir.join("cassettes");
            let store = cassette::store::CassetteStore::new(&cassettes_dir);

            match action {
                CassetteAction::List { step } => {
                    let step_filter = step.and_then(|s| s.parse::<u64>().ok());
                    let list = store.list_cassettes(&engine.session.session_id, step_filter)?;
                    for c in list {
                        println!(
                            "{} step {} (recorded: {}, turns: {})",
                            "Cassette".bold().cyan(),
                            c.step_id,
                            c.recorded_at,
                            c.entries.len()
                        );
                        for e in c.entries {
                            println!(
                                "  Turn {}: {} {} (status: {}, duration: {}ms)",
                                e.id.cyan(),
                                e.request.method.yellow(),
                                e.endpoint,
                                e.response.status,
                                e.duration_ms
                            );
                        }
                    }
                }
                CassetteAction::Show { id } => {
                    let record = store.load_turn(&id)?;
                    println!("{}", serde_json::to_string_pretty(&record)?);
                }
                CassetteAction::Edit {
                    id,
                    prompt,
                    response,
                } => {
                    store.edit_turn(&id, prompt.as_deref(), response.as_deref())?;
                    println!("{} cassette turn {}", "Edited".bold().green(), id.cyan());
                }
                CassetteAction::Clear => {
                    store.clear_all(&engine.session.session_id)?;
                    println!("{}", "Cleared all cassettes for session".green());
                }
            }

            Ok(0)
        }
    }
}
