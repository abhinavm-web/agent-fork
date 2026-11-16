use agent_fork::CowEngine;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let ws = tmp.path();

    fs::write(ws.join("auth.rs"), "pub fn verify_token(tok: &str) -> bool { tok == \"legacy\" }\n")?;

    println!("=== agent-fork: Copy-on-Write Time-Travel & Trajectory Forking Showcase ===");
    let mut engine = CowEngine::init(ws)?;
    println!("[step 0] Initialized CoW sandbox at {}", ws.display());

    // Simulate an agent modifying files and creating a database artifact via shell
    fs::write(ws.join("auth.rs"), "pub fn verify_token(tok: &str) -> bool { tok.starts_with(\"jwt_\") }\n")?;
    fs::write(ws.join("local.sqlite"), b"SQLite format 3\0migrated_table_v1")?;
    let m1 = engine.checkpoint_with_cmd(
        "Agent migrated auth.rs and created local.sqlite",
        Some("cargo test && sqlite3 local.sqlite < migrate.sql".to_string()),
        Some(0),
    )?;
    println!(
        "[step {}] Checkpointed: '{}' (+{} ~{} -{})",
        m1.step_id,
        m1.description,
        m1.delta.added.len(),
        m1.delta.modified.len(),
        m1.delta.deleted.len()
    );

    // Simulate a broken step 2 where the agent corrupts the state
    fs::write(ws.join("auth.rs"), "compile_error!(\"broken agent edit\");\n")?;
    fs::remove_file(ws.join("local.sqlite"))?;
    let m2 = engine.checkpoint_with_cmd(
        "Agent broke auth.rs and deleted local.sqlite",
        Some("rm local.sqlite".to_string()),
        Some(1),
    )?;
    println!(
        "[step {}] Bad agent turn: '{}' (+{} ~{} -{})",
        m2.step_id,
        m2.description,
        m2.delta.added.len(),
        m2.delta.modified.len(),
        m2.delta.deleted.len()
    );

    // Rewind back to Step 1 (restoring both auth.rs AND local.sqlite!)
    engine.rewind(1, true)?;
    println!(
        "[rewind -> step 1] Restored local.sqlite exists: {}, auth.rs restored: {}",
        ws.join("local.sqlite").exists(),
        fs::read_to_string(ws.join("auth.rs"))?.trim()
    );

    // Fork two parallel trajectories from Step 1
    engine.fork(1, &["ed25519-tokens", "session-cookies"])?;
    for branch in engine.list_branches()? {
        println!(
            "[branch] '{}' (head: step {}, base: step {})",
            branch.name, branch.head_step_id, branch.base_step_id
        );
    }

    Ok(())
}
