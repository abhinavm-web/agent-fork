use ignore::{Walk, WalkBuilder};
use std::path::Path;

pub fn is_system_ignored(rel_path: &str) -> bool {
    let clean = rel_path.replace('\\', "/").trim_start_matches('/').to_string();
    clean == ".agent-fork"
        || clean.starts_with(".agent-fork/")
        || clean == ".git"
        || clean.starts_with(".git/")
        || clean == "target"
        || clean.starts_with("target/")
        || clean == "node_modules"
        || clean.starts_with("node_modules/")
        || clean == ".venv"
        || clean.starts_with(".venv/")
        || clean == "venv"
        || clean.starts_with("venv/")
}

pub fn create_workspace_walker(workspace_root: &Path) -> Walk {
    let mut builder = WalkBuilder::new(workspace_root);
    builder
        .hidden(false) // Allow dotfiles like .eslintrc, .env
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .parents(false);

    let root = workspace_root.to_path_buf();
    builder.filter_entry(move |entry| {
        if let Ok(rel) = entry.path().strip_prefix(&root) {
            let rel_str = rel.to_string_lossy();
            if is_system_ignored(&rel_str) {
                return false;
            }
        }
        true
    });

    builder.build()
}
