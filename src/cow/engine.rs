use crate::cow::manifest::{FileEntry, StepDelta, StepManifest};
use crate::cow::storage::CasStore;
use crate::error::AgentForkError;
use crate::session::branch::BranchMetadata;
use crate::session::timeline::SessionState;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub struct CowEngine {
    pub workspace_root: PathBuf,
    pub agent_fork_dir: PathBuf,
    pub manifests_dir: PathBuf,
    pub branches_dir: PathBuf,
    pub cas: CasStore,
    pub session: SessionState,
}

impl CowEngine {
    pub fn init(workspace_root: &Path) -> Result<Self, AgentForkError> {
        let agent_fork_dir = workspace_root.join(".agent-fork");
        let manifests_dir = agent_fork_dir.join("manifests");
        let branches_dir = agent_fork_dir.join("branches");
        let storage_dir = agent_fork_dir.join("storage");
        let cassettes_dir = agent_fork_dir.join("cassettes");

        fs::create_dir_all(&manifests_dir)?;
        fs::create_dir_all(&branches_dir)?;
        fs::create_dir_all(&cassettes_dir)?;

        let cas = CasStore::init(&storage_dir)?;

        let session_file = agent_fork_dir.join("session.json");
        let session = if session_file.is_file() {
            SessionState::load(&session_file)?
        } else {
            let s = SessionState::new("main");
            s.save(&session_file)?;
            s
        };

        // Create main branch metadata if missing
        let main_branch_dir = branches_dir.join("main");
        fs::create_dir_all(&main_branch_dir)?;
        let main_branch_file = main_branch_dir.join("branch.json");
        if !main_branch_file.is_file() {
            let meta = BranchMetadata::new("main", 0, "Main primary branch", "");
            meta.save(&main_branch_file)?;
        }

        let mut engine = Self {
            workspace_root: workspace_root.to_path_buf(),
            agent_fork_dir,
            manifests_dir,
            branches_dir,
            cas,
            session,
        };

        // If Step 0 manifest does not exist, create it as the base snapshot
        let step_0_path = engine.manifests_dir.join("0.json");
        if !step_0_path.is_file() {
            engine.create_step_0()?;
        }

        Ok(engine)
    }

    pub fn open(workspace_root: &Path) -> Result<Self, AgentForkError> {
        let agent_fork_dir = workspace_root.join(".agent-fork");
        if !agent_fork_dir.is_dir() {
            return Err(AgentForkError::SessionNotFound(
                workspace_root.display().to_string(),
            ));
        }

        let manifests_dir = agent_fork_dir.join("manifests");
        let branches_dir = agent_fork_dir.join("branches");
        let storage_dir = agent_fork_dir.join("storage");

        let cas = CasStore::init(&storage_dir)?;
        let session_file = agent_fork_dir.join("session.json");
        let session = SessionState::load(&session_file)?;

        Ok(Self {
            workspace_root: workspace_root.to_path_buf(),
            agent_fork_dir,
            manifests_dir,
            branches_dir,
            cas,
            session,
        })
    }

    fn create_step_0(&mut self) -> Result<StepManifest, AgentForkError> {
        let mut files = BTreeMap::new();
        let walker = crate::cow::ignore::create_workspace_walker(&self.workspace_root);

        for entry_res in walker {
            let entry = entry_res?;
            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                let full_path = entry.path();
                if let Ok(rel) = full_path.strip_prefix(&self.workspace_root) {
                    let norm_path = rel.to_string_lossy().replace('\\', "/");
                    if crate::cow::ignore::is_system_ignored(&norm_path) {
                        continue;
                    }
                    let meta = entry.metadata()?;
                    let size = meta.len();
                    let mtime_ms = get_mtime_ms(&meta);
                    let is_exec = is_executable(&meta);

                    let bytes = fs::read(full_path)?;
                    let hash = blake3::hash(&bytes).to_hex().to_string();
                    self.cas.store_bytes(&bytes, &hash)?;

                    files.insert(
                        norm_path.clone(),
                        FileEntry {
                            path: norm_path,
                            hash,
                            size,
                            mtime_ms,
                            is_executable: is_exec,
                        },
                    );
                }
            }
        }

        let mut manifest = StepManifest::new(
            0,
            self.session.session_id.clone(),
            self.session.active_branch.clone(),
            None,
            "Initial base workspace state".to_string(),
            None,
            None,
        );
        manifest.delta.added = files.keys().cloned().collect();
        manifest.files = files;

        let manifest_path = self.manifests_dir.join("0.json");
        let json_str = serde_json::to_string_pretty(&manifest)?;
        fs::write(manifest_path, json_str)?;

        self.session.current_step_id = 0;
        self.session.save(&self.agent_fork_dir.join("session.json"))?;

        Ok(manifest)
    }

    pub fn load_manifest(&self, step_id: u64) -> Result<StepManifest, AgentForkError> {
        let path = self.manifests_dir.join(format!("{}.json", step_id));
        if !path.is_file() {
            return Err(AgentForkError::StepNotFound(step_id.to_string()));
        }
        let data = fs::read_to_string(&path)?;
        let manifest: StepManifest = serde_json::from_str(&data)?;
        Ok(manifest)
    }

    pub fn get_latest_manifest(&self) -> Result<StepManifest, AgentForkError> {
        self.load_manifest(self.session.current_step_id)
    }

    pub fn is_dirty(&self) -> Result<bool, AgentForkError> {
        let parent = self.get_latest_manifest()?;
        let mut current_paths = BTreeSet::new();

        let walker = crate::cow::ignore::create_workspace_walker(&self.workspace_root);
        for entry_res in walker {
            let entry = entry_res?;
            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                let full = entry.path();
                if let Ok(rel) = full.strip_prefix(&self.workspace_root) {
                    let norm = rel.to_string_lossy().replace('\\', "/");
                    if crate::cow::ignore::is_system_ignored(&norm) {
                        continue;
                    }
                    current_paths.insert(norm.clone());

                    let meta = entry.metadata()?;
                    let size = meta.len();
                    let mtime_ms = get_mtime_ms(&meta);

                    match parent.files.get(&norm) {
                        Some(parent_entry) => {
                            if parent_entry.size != size || parent_entry.mtime_ms != mtime_ms {
                                let bytes = fs::read(full)?;
                                let hash = blake3::hash(&bytes).to_hex().to_string();
                                if hash != parent_entry.hash {
                                    return Ok(true);
                                }
                            }
                        }
                        None => return Ok(true), // new file
                    }
                }
            }
        }

        // Check for deleted files
        for p in parent.files.keys() {
            if !current_paths.contains(p) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    pub fn checkpoint(
        &mut self,
        description: &str,
    ) -> Result<StepManifest, AgentForkError> {
        self.checkpoint_with_cmd(description, None, None)
    }

    pub fn checkpoint_with_cmd(
        &mut self,
        description: &str,
        command: Option<String>,
        exit_code: Option<i32>,
    ) -> Result<StepManifest, AgentForkError> {
        let parent_id = self.session.current_step_id;
        let parent = self.load_manifest(parent_id)?;
        let next_step_id = parent_id + 1;

        let mut current_files = BTreeMap::new();
        let mut delta = StepDelta::default();

        let walker = crate::cow::ignore::create_workspace_walker(&self.workspace_root);
        for entry_res in walker {
            let entry = entry_res?;
            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                let full = entry.path();
                if let Ok(rel) = full.strip_prefix(&self.workspace_root) {
                    let norm = rel.to_string_lossy().replace('\\', "/");
                    if crate::cow::ignore::is_system_ignored(&norm) {
                        continue;
                    }

                    let meta = entry.metadata()?;
                    let size = meta.len();
                    let mtime_ms = get_mtime_ms(&meta);
                    let is_exec = is_executable(&meta);

                    // Fast dirty check
                    let (hash, was_modified, is_new) = match parent.files.get(&norm) {
                        Some(parent_entry) => {
                            if parent_entry.size == size && parent_entry.mtime_ms == mtime_ms {
                                (parent_entry.hash.clone(), false, false)
                            } else {
                                let bytes = fs::read(full)?;
                                let h = blake3::hash(&bytes).to_hex().to_string();
                                self.cas.store_bytes(&bytes, &h)?;
                                let modified = h != parent_entry.hash;
                                (h, modified, false)
                            }
                        }
                        None => {
                            let bytes = fs::read(full)?;
                            let h = blake3::hash(&bytes).to_hex().to_string();
                            self.cas.store_bytes(&bytes, &h)?;
                            (h, false, true)
                        }
                    };

                    if is_new {
                        delta.added.push(norm.clone());
                    } else if was_modified {
                        delta.modified.push(norm.clone());
                    }

                    current_files.insert(
                        norm.clone(),
                        FileEntry {
                            path: norm,
                            hash,
                            size,
                            mtime_ms,
                            is_executable: is_exec,
                        },
                    );
                }
            }
        }

        // Deleted files
        for parent_path in parent.files.keys() {
            if !current_files.contains_key(parent_path) {
                delta.deleted.push(parent_path.clone());
            }
        }

        delta.added.sort();
        delta.modified.sort();
        delta.deleted.sort();

        let mut manifest = StepManifest::new(
            next_step_id,
            self.session.session_id.clone(),
            self.session.active_branch.clone(),
            Some(parent_id),
            description.to_string(),
            command,
            exit_code,
        );
        manifest.delta = delta;
        manifest.files = current_files;

        // Write manifest
        let manifest_path = self.manifests_dir.join(format!("{}.json", next_step_id));
        let json_str = serde_json::to_string_pretty(&manifest)?;
        fs::write(manifest_path, json_str)?;

        // Update session
        self.session.current_step_id = next_step_id;
        self.session.save(&self.agent_fork_dir.join("session.json"))?;

        // Update branch head
        let branch_file = self
            .branches_dir
            .join(&self.session.active_branch)
            .join("branch.json");
        if branch_file.is_file() {
            if let Ok(mut meta) = BranchMetadata::load(&branch_file) {
                meta.head_step_id = next_step_id;
                let _ = meta.save(&branch_file);
            }
        }

        Ok(manifest)
    }

    pub fn rewind(&mut self, target_step_id: u64, force: bool) -> Result<(), AgentForkError> {
        if !force && self.is_dirty()? {
            return Err(AgentForkError::DirtyWorkingTree);
        }

        let target_manifest = self.load_manifest(target_step_id)?;

        // Phase 1: Remove files not in target manifest
        let walker = crate::cow::ignore::create_workspace_walker(&self.workspace_root);
        for entry_res in walker {
            let entry = entry_res?;
            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                let full = entry.path();
                if let Ok(rel) = full.strip_prefix(&self.workspace_root) {
                    let norm = rel.to_string_lossy().replace('\\', "/");
                    if crate::cow::ignore::is_system_ignored(&norm) {
                        continue;
                    }
                    if !target_manifest.files.contains_key(&norm) {
                        let _ = fs::remove_file(full);
                    }
                }
            }
        }

        // Bottom-up directory cleanup
        prune_empty_dirs(&self.workspace_root);

        // Phase 2: Restore missing or modified files from CAS
        for (rel_path, entry) in &target_manifest.files {
            let full_target = self.workspace_root.join(rel_path);
            let needs_restore = if full_target.is_file() {
                if let Ok(meta) = full_target.metadata() {
                    if meta.len() == entry.size && get_mtime_ms(&meta) == entry.mtime_ms {
                        false
                    } else if let Ok(bytes) = fs::read(&full_target) {
                        blake3::hash(&bytes).to_hex().as_str() != entry.hash
                    } else {
                        true
                    }
                } else {
                    true
                }
            } else {
                true
            };

            if needs_restore {
                let bytes = self.cas.read_bytes(&entry.hash)?;
                safe_replace_file(&full_target, &bytes, entry.is_executable)?;
            }
        }

        // Phase 3: Update session
        self.session.current_step_id = target_step_id;
        self.session.save(&self.agent_fork_dir.join("session.json"))?;

        let branch_file = self
            .branches_dir
            .join(&self.session.active_branch)
            .join("branch.json");
        if branch_file.is_file() {
            if let Ok(mut meta) = BranchMetadata::load(&branch_file) {
                meta.head_step_id = target_step_id;
                let _ = meta.save(&branch_file);
            }
        }

        Ok(())
    }

    pub fn fork(&mut self, base_step_id: u64, variants: &[&str]) -> Result<(), AgentForkError> {
        let base_manifest = self.load_manifest(base_step_id)?;

        for variant in variants {
            let branch_dir = self.branches_dir.join(variant);
            if branch_dir.exists() {
                return Err(AgentForkError::BranchAlreadyExists(variant.to_string()));
            }

            let worktree_dir = branch_dir.join("worktree");
            fs::create_dir_all(&worktree_dir)?;

            // Restore files from base_manifest into worktree_dir
            for (rel_path, entry) in &base_manifest.files {
                let target_path = worktree_dir.join(rel_path);
                let bytes = self.cas.read_bytes(&entry.hash)?;
                safe_replace_file(&target_path, &bytes, entry.is_executable)?;
            }

            // Write branch.json
            let meta = BranchMetadata::new(
                variant,
                base_step_id,
                &format!("Forked from step {}", base_step_id),
                &worktree_dir.display().to_string(),
            );
            meta.save(&branch_dir.join("branch.json"))?;

            if !self.session.branches.contains(&variant.to_string()) {
                self.session.branches.push(variant.to_string());
            }
        }

        self.session.save(&self.agent_fork_dir.join("session.json"))?;
        Ok(())
    }

    pub fn list_branches(&self) -> Result<Vec<BranchMetadata>, AgentForkError> {
        let mut list = Vec::new();
        if self.branches_dir.is_dir() {
            for entry in fs::read_dir(&self.branches_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    let branch_file = path.join("branch.json");
                    if branch_file.is_file() {
                        if let Ok(meta) = BranchMetadata::load(&branch_file) {
                            list.push(meta);
                        }
                    }
                }
            }
        }
        list.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(list)
    }

    pub fn promote(&mut self, branch_name: &str, force: bool) -> Result<StepManifest, AgentForkError> {
        if !force && self.is_dirty()? {
            return Err(AgentForkError::DirtyWorkingTree);
        }

        let branch_dir = self.branches_dir.join(branch_name);
        let branch_file = branch_dir.join("branch.json");
        if !branch_file.is_file() {
            return Err(AgentForkError::BranchNotFound(branch_name.to_string()));
        }

        let branch_meta = BranchMetadata::load(&branch_file)?;
        let worktree_dir = branch_dir.join("worktree");

        // The branch might have modifications directly in its worktree directory!
        // We scan the worktree directory, save any new files to CAS, and build the target state.
        let mut target_files = BTreeMap::new();
        if worktree_dir.is_dir() {
            let walker = crate::cow::ignore::create_workspace_walker(&worktree_dir);
            for entry_res in walker {
                let entry = entry_res?;
                if entry.file_type().is_some_and(|ft| ft.is_file()) {
                    let full = entry.path();
                    if let Ok(rel) = full.strip_prefix(&worktree_dir) {
                        let norm = rel.to_string_lossy().replace('\\', "/");
                        if crate::cow::ignore::is_system_ignored(&norm) {
                            continue;
                        }

                        let meta = entry.metadata()?;
                        let size = meta.len();
                        let mtime_ms = get_mtime_ms(&meta);
                        let is_exec = is_executable(&meta);

                        let bytes = fs::read(full)?;
                        let hash = blake3::hash(&bytes).to_hex().to_string();
                        self.cas.store_bytes(&bytes, &hash)?;

                        target_files.insert(
                            norm.clone(),
                            FileEntry {
                                path: norm,
                                hash,
                                size,
                                mtime_ms,
                                is_executable: is_exec,
                            },
                        );
                    }
                }
            }
        } else {
            // Fall back to head manifest of the branch
            let head_manifest = self.load_manifest(branch_meta.head_step_id)?;
            target_files = head_manifest.files;
        }

        // Apply target_files to the main base workspace:
        // 1. Delete files in base not in target_files
        let walker = crate::cow::ignore::create_workspace_walker(&self.workspace_root);
        for entry_res in walker {
            let entry = entry_res?;
            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                let full = entry.path();
                if let Ok(rel) = full.strip_prefix(&self.workspace_root) {
                    let norm = rel.to_string_lossy().replace('\\', "/");
                    if crate::cow::ignore::is_system_ignored(&norm) {
                        continue;
                    }
                    if !target_files.contains_key(&norm) {
                        let _ = fs::remove_file(full);
                    }
                }
            }
        }
        prune_empty_dirs(&self.workspace_root);

        // 2. Restore all files from target_files into base workspace
        for (rel_path, entry) in &target_files {
            let target_path = self.workspace_root.join(rel_path);
            let bytes = self.cas.read_bytes(&entry.hash)?;
            safe_replace_file(&target_path, &bytes, entry.is_executable)?;
        }

        // 3. Create a checkpoint recording the promotion on main timeline
        let description = format!(
            "Promoted branch '{}' (base step {})",
            branch_name, branch_meta.base_step_id
        );
        self.checkpoint(&description)
    }
}

pub fn get_mtime_ms(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn is_executable(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        false
    }
}

pub fn safe_replace_file(
    target: &Path,
    bytes: &[u8],
    is_exec: bool,
) -> Result<(), AgentForkError> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| AgentForkError::IoWithPath {
            path: parent.display().to_string(),
            source: e,
        })?;
    }

    let tmp = target.with_extension(format!("af_tmp_{}", uuid::Uuid::new_v4()));
    fs::write(&tmp, bytes).map_err(|e| AgentForkError::IoWithPath {
        path: tmp.display().to_string(),
        source: e,
    })?;

    #[cfg(unix)]
    if is_exec {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755));
    }
    let _ = is_exec;

    let mut retries = 0;
    loop {
        match fs::rename(&tmp, target) {
            Ok(_) => return Ok(()),
            Err(_e) if retries < 3 => {
                retries += 1;
                std::thread::sleep(std::time::Duration::from_millis(15 * retries));
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(AgentForkError::IoWithPath {
                    path: target.display().to_string(),
                    source: e,
                });
            }
        }
    }
}

pub fn prune_empty_dirs(root: &Path) {
    let mut dirs = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(false)
        .build();

    for entry in walker.flatten() {
        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            let p = entry.path().to_path_buf();
            if p != root {
                if let Ok(rel) = p.strip_prefix(root) {
                    let norm = rel.to_string_lossy().replace('\\', "/");
                    if !crate::cow::ignore::is_system_ignored(&norm) {
                        dirs.push(p);
                    }
                }
            }
        }
    }

    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for dir in dirs {
        let _ = fs::remove_dir(dir);
    }
}
