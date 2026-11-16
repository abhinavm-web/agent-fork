use crate::cow::manifest::StepManifest;
use crate::cow::storage::CasStore;
use crate::error::AgentForkError;
use colored::Colorize;
use similar::{ChangeTag, TextDiff};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct DiffOptions {
    pub stat: bool,
    pub name_only: bool,
    pub color: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FileDiffStat {
    pub path: String,
    pub insertions: usize,
    pub deletions: usize,
}

pub fn format_text_diff(old_text: &str, new_text: &str, file_path: &str, color: bool) -> String {
    let diff = TextDiff::from_lines(old_text, new_text);
    let mut output = String::new();

    if color {
        output.push_str(&format!("--- a/{}\n", file_path).bold().red().to_string());
        output.push_str(&format!("+++ b/{}\n", file_path).bold().green().to_string());
    } else {
        output.push_str(&format!("--- a/{}\n+++ b/{}\n", file_path, file_path));
    }

    for group in diff.grouped_ops(3) {
        for op in group {
            for change in diff.iter_changes(&op) {
                let (sign, styled) = match change.tag() {
                    ChangeTag::Delete => {
                        let text = format!("-{}", change.value());
                        let colored_text = if color { text.red().to_string() } else { text };
                        ("-", colored_text)
                    }
                    ChangeTag::Insert => {
                        let text = format!("+{}", change.value());
                        let colored_text = if color { text.green().to_string() } else { text };
                        ("+", colored_text)
                    }
                    ChangeTag::Equal => {
                        let text = format!(" {}", change.value());
                        (" ", text)
                    }
                };
                let _ = sign;
                output.push_str(&styled);
            }
        }
    }

    output
}

pub fn compute_file_stat(old_text: &str, new_text: &str, file_path: &str) -> FileDiffStat {
    let diff = TextDiff::from_lines(old_text, new_text);
    let mut insertions = 0;
    let mut deletions = 0;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => insertions += 1,
            ChangeTag::Delete => deletions += 1,
            ChangeTag::Equal => {}
        }
    }

    FileDiffStat {
        path: file_path.to_string(),
        insertions,
        deletions,
    }
}

pub fn render_manifest_diff(
    old_manifest: Option<&StepManifest>,
    new_manifest: Option<&StepManifest>,
    workspace_root: &Path,
    cas: &CasStore,
    options: &DiffOptions,
) -> Result<String, AgentForkError> {
    let mut all_paths = BTreeSet::new();

    if let Some(m) = old_manifest {
        for p in m.files.keys() {
            all_paths.insert(p.clone());
        }
    }

    if let Some(m) = new_manifest {
        for p in m.files.keys() {
            all_paths.insert(p.clone());
        }
    } else {
        // Comparing against current working tree
        let walker = crate::cow::ignore::create_workspace_walker(workspace_root);
        for entry_res in walker {
            let entry = entry_res?;
            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                if let Ok(rel) = entry.path().strip_prefix(workspace_root) {
                    let norm = rel.to_string_lossy().replace('\\', "/");
                    all_paths.insert(norm);
                }
            }
        }
    }

    let mut out = String::new();
    let mut stats = Vec::new();
    let mut changed_files = Vec::new();

    for rel_path in all_paths {
        let old_entry = old_manifest.and_then(|m| m.files.get(&rel_path));
        let old_bytes = match old_entry {
            Some(e) => cas.read_bytes(&e.hash).ok(),
            None => None,
        };

        let new_bytes = match new_manifest {
            Some(m) => match m.files.get(&rel_path) {
                Some(e) => cas.read_bytes(&e.hash).ok(),
                None => None,
            },
            None => {
                let full = workspace_root.join(&rel_path);
                if full.is_file() {
                    fs::read(&full).ok()
                } else {
                    None
                }
            }
        };

        if old_bytes == new_bytes {
            continue;
        }

        changed_files.push(rel_path.clone());

        let old_text = old_bytes.as_ref().and_then(|b| std::str::from_utf8(b).ok());
        let new_text = new_bytes.as_ref().and_then(|b| std::str::from_utf8(b).ok());

        match (old_text, new_text) {
            (Some(o), Some(n)) => {
                if options.stat {
                    stats.push(compute_file_stat(o, n, &rel_path));
                } else if !options.name_only {
                    out.push_str(&format_text_diff(o, n, &rel_path, options.color));
                }
            }
            (None, Some(n)) => {
                if options.stat {
                    stats.push(compute_file_stat("", n, &rel_path));
                } else if !options.name_only {
                    out.push_str(&format_text_diff("", n, &rel_path, options.color));
                }
            }
            (Some(o), None) => {
                if options.stat {
                    stats.push(compute_file_stat(o, "", &rel_path));
                } else if !options.name_only {
                    out.push_str(&format_text_diff(o, "", &rel_path, options.color));
                }
            }
            _ => {
                // Binary or non-utf8
                if options.stat {
                    stats.push(FileDiffStat {
                        path: rel_path.clone(),
                        insertions: 0,
                        deletions: 0,
                    });
                } else if !options.name_only {
                    out.push_str(&format!("Binary files differ: {}\n", rel_path));
                }
            }
        }
    }

    if options.name_only {
        return Ok(changed_files.join("\n"));
    }

    if options.stat {
        let mut stat_output = String::new();
        let mut total_ins = 0;
        let mut total_del = 0;
        for s in &stats {
            total_ins += s.insertions;
            total_del += s.deletions;
            stat_output.push_str(&format!(
                " {} | +{} -{}\n",
                s.path, s.insertions, s.deletions
            ));
        }
        stat_output.push_str(&format!(
            " {} file(s) changed, {} insertions(+), {} deletions(-)\n",
            stats.len(),
            total_ins,
            total_del
        ));
        return Ok(stat_output);
    }

    Ok(out)
}
