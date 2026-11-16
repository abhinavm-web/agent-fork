use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_cli_help_displays_all_subcommands() {
    let mut cmd = Command::cargo_bin("agent-fork").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("checkpoint"))
        .stdout(predicate::str::contains("log"))
        .stdout(predicate::str::contains("diff"))
        .stdout(predicate::str::contains("rewind"))
        .stdout(predicate::str::contains("fork"))
        .stdout(predicate::str::contains("branches"))
        .stdout(predicate::str::contains("promote"))
        .stdout(predicate::str::contains("cassette"));
}

#[test]
fn test_cli_checkpoint_and_log_workflow() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("hello.txt"), "hello world\n").unwrap();

    // 1. Initial checkpoint
    let mut cmd = Command::cargo_bin("agent-fork").unwrap();
    cmd.current_dir(root)
        .arg("checkpoint")
        .arg("-m")
        .arg("Initial commit")
        .assert()
        .success()
        .stdout(predicate::str::contains("step 1"));

    // 2. Log verification
    let mut log_cmd = Command::cargo_bin("agent-fork").unwrap();
    log_cmd.current_dir(root)
        .arg("log")
        .assert()
        .success()
        .stdout(predicate::str::contains("Initial commit"))
        .stdout(predicate::str::contains("step 1"));

    // 3. Log JSON verification
    let mut json_cmd = Command::cargo_bin("agent-fork").unwrap();
    json_cmd.current_dir(root)
        .arg("log")
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"step_id\": 1"));
}

#[test]
fn test_cli_diff_subcommand() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("file.txt"), "line 1\n").unwrap();

    // Checkpoint 1
    Command::cargo_bin("agent-fork").unwrap()
        .current_dir(root)
        .args(["checkpoint", "-m", "Step 1"])
        .assert()
        .success();

    // Modify file
    fs::write(root.join("file.txt"), "line 1\nline 2\n").unwrap();

    // Run diff
    Command::cargo_bin("agent-fork").unwrap()
        .current_dir(root)
        .arg("diff")
        .assert()
        .success()
        .stdout(predicate::str::contains("+line 2"));
}

#[test]
fn test_cli_rewind_workflow() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("state.txt"), "v1").unwrap();

    // Step 1
    Command::cargo_bin("agent-fork").unwrap()
        .current_dir(root)
        .args(["checkpoint", "-m", "v1"])
        .assert()
        .success();

    // Modify and create junk
    fs::write(root.join("state.txt"), "v2 broken").unwrap();
    fs::write(root.join("garbage.log"), "junk").unwrap();

    // Rewind to 1 with force
    Command::cargo_bin("agent-fork").unwrap()
        .current_dir(root)
        .args(["rewind", "1", "--force"])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(root.join("state.txt")).unwrap(), "v1");
    assert!(!root.join("garbage.log").exists());
}

#[test]
fn test_cli_fork_and_branches() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("code.py"), "print(1)\n").unwrap();

    Command::cargo_bin("agent-fork").unwrap()
        .current_dir(root)
        .args(["checkpoint", "-m", "initial"])
        .assert()
        .success();

    // Fork variants
    Command::cargo_bin("agent-fork").unwrap()
        .current_dir(root)
        .args(["fork", "1", "-v", "branch-a", "-v", "branch-b"])
        .assert()
        .success();

    // List branches
    Command::cargo_bin("agent-fork").unwrap()
        .current_dir(root)
        .arg("branches")
        .assert()
        .success()
        .stdout(predicate::str::contains("main"))
        .stdout(predicate::str::contains("branch-a"))
        .stdout(predicate::str::contains("branch-b"));
}

#[test]
fn test_cli_run_forwards_child_exit_code() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    #[cfg(windows)]
    let child_args = ["cmd.exe", "/c", "exit 42"];
    #[cfg(not(windows))]
    let child_args = ["sh", "-c", "exit 42"];

    let mut cmd = Command::cargo_bin("agent-fork").unwrap();
    cmd.current_dir(root)
        .arg("run")
        .arg("--no-proxy")
        .arg("--")
        .args(child_args)
        .assert()
        .code(42);
}
