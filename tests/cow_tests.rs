use agent_fork::cow::engine::CowEngine;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_cow_initialization_creates_base_step_0() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Setup base files
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();

    let engine = CowEngine::init(root).expect("Initialization should succeed");

    // Check .agent-fork structure
    assert!(root.join(".agent-fork/storage/objects").exists());
    assert!(root.join(".agent-fork/manifests/0.json").exists());
    assert!(root.join(".agent-fork/session.json").exists());

    let manifest = engine.load_manifest(0).expect("Step 0 manifest must exist");
    assert_eq!(manifest.files.len(), 2);
    assert!(manifest.files.contains_key("Cargo.toml"));
    assert!(manifest.files.contains_key("src/lib.rs"));
}

#[tokio::test]
async fn test_cow_file_modifications_captured_in_isolated_layers() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("README.md"), "# Original\n").unwrap();

    let mut engine = CowEngine::init(root).unwrap();

    // Step 1: Add new file, modify README.md
    fs::write(root.join("README.md"), "# Modified in Step 1\n").unwrap();
    fs::write(root.join("config.json"), "{\"env\":\"dev\"}\n").unwrap();
    let step_1 = engine.checkpoint("Step 1 changes").unwrap();

    assert_eq!(step_1.step_id, 1);
    assert_eq!(step_1.delta.modified, vec!["README.md"]);
    assert_eq!(step_1.delta.added, vec!["config.json"]);
    assert!(step_1.delta.deleted.is_empty());

    // Step 2: Delete README.md, modify config.json
    fs::remove_file(root.join("README.md")).unwrap();
    fs::write(root.join("config.json"), "{\"env\":\"prod\"}\n").unwrap();
    let step_2 = engine.checkpoint("Step 2 changes").unwrap();

    assert_eq!(step_2.step_id, 2);
    assert_eq!(step_2.delta.deleted, vec!["README.md"]);
    assert_eq!(step_2.delta.modified, vec!["config.json"]);
}

#[tokio::test]
async fn test_cow_rewind_restores_base_state_exact_fidelity() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Base state
    let base_code = "fn main() { println!(\"base\"); }\n";
    let base_readme = "# Base Project\n";
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), base_code).unwrap();
    fs::write(root.join("README.md"), base_readme).unwrap();

    let mut engine = CowEngine::init(root).unwrap();

    // Destructive changes across steps
    fs::write(root.join("src/main.rs"), "fn main() { panic!(\"broken\"); }\n").unwrap();
    fs::write(root.join("untracked_1.tmp"), "temp data 1").unwrap();
    fs::create_dir_all(root.join("nested/dir")).unwrap();
    fs::write(root.join("nested/dir/file.txt"), "nested").unwrap();
    fs::remove_file(root.join("README.md")).unwrap();
    let _ = engine.checkpoint("Destructive Step 1").unwrap();

    // Rewind back to Step 0
    engine.rewind(0, true).expect("Rewind to step 0 should succeed");

    // Assert base state restored exactly
    assert_eq!(fs::read_to_string(root.join("src/main.rs")).unwrap(), base_code);
    assert_eq!(fs::read_to_string(root.join("README.md")).unwrap(), base_readme);

    // Assert newly created files and directories are completely wiped
    assert!(!root.join("untracked_1.tmp").exists());
    assert!(!root.join("nested/dir/file.txt").exists());
    assert!(!root.join("nested").exists());
}

#[tokio::test]
async fn test_cow_branch_fork_and_isolation() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("app.js"), "const version = 1;\n").unwrap();

    let mut engine = CowEngine::init(root).unwrap();

    // Fork two competing branches from step 0
    engine.fork(0, &["variant-cookie", "variant-jwt"]).unwrap();

    let cookie_worktree = root.join(".agent-fork/branches/variant-cookie/worktree");
    let jwt_worktree = root.join(".agent-fork/branches/variant-jwt/worktree");

    assert!(cookie_worktree.join("app.js").exists());
    assert!(jwt_worktree.join("app.js").exists());

    // Mutate variant-cookie
    fs::write(cookie_worktree.join("app.js"), "const version = 1; const auth = 'cookie';\n").unwrap();
    fs::write(cookie_worktree.join("cookie_helper.js"), "// helper\n").unwrap();

    // Mutate variant-jwt
    fs::write(jwt_worktree.join("app.js"), "const version = 1; const auth = 'jwt';\n").unwrap();
    fs::write(jwt_worktree.join("jwt_helper.js"), "// helper\n").unwrap();

    // Verify complete isolation
    // Base workspace is untouched
    assert_eq!(fs::read_to_string(root.join("app.js")).unwrap(), "const version = 1;\n");
    assert!(!root.join("cookie_helper.js").exists());
    assert!(!root.join("jwt_helper.js").exists());

    // Branch A does not have Branch B files
    assert!(!cookie_worktree.join("jwt_helper.js").exists());
    assert!(!jwt_worktree.join("cookie_helper.js").exists());
}

#[tokio::test]
async fn test_cow_branch_promotion_to_main() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("config.txt"), "base\n").unwrap();

    let mut engine = CowEngine::init(root).unwrap();
    engine.fork(0, &["winner-branch"]).unwrap();

    let winner_worktree = root.join(".agent-fork/branches/winner-branch/worktree");
    fs::write(winner_worktree.join("config.txt"), "winning configuration\n").unwrap();
    fs::write(winner_worktree.join("new_feature.txt"), "feature data\n").unwrap();

    // Promote winner-branch
    engine.promote("winner-branch", true).unwrap();

    // Base workspace should now have the promoted changes
    assert_eq!(fs::read_to_string(root.join("config.txt")).unwrap(), "winning configuration\n");
    assert_eq!(fs::read_to_string(root.join("new_feature.txt")).unwrap(), "feature data\n");
}
