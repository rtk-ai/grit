use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct GitRepo {
    root: PathBuf,
}

impl GitRepo {
    pub fn open(path: &str) -> Result<Self> {
        let root = std::fs::canonicalize(path)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn grit_dir(&self) -> PathBuf {
        self.root.join(".grit")
    }

    /// Create an isolated git worktree for an agent
    pub fn create_worktree(&self, agent_id: &str) -> Result<PathBuf> {
        let wt_path = self.grit_dir().join("worktrees").join(agent_id);
        let branch_name = format!("agent/{}", agent_id);

        if wt_path.exists() {
            anyhow::bail!("Worktree already exists at {}", wt_path.display());
        }

        std::fs::create_dir_all(wt_path.parent().unwrap())?;

        // Create a new branch and worktree
        let output = Command::new("git")
            .args(["worktree", "add", "-b", &branch_name, &wt_path.to_string_lossy()])
            .current_dir(&self.root)
            .output()
            .context("Failed to run git worktree add")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // If branch already exists, try without -b
            if stderr.contains("already exists") {
                let output2 = Command::new("git")
                    .args(["worktree", "add", &wt_path.to_string_lossy(), &branch_name])
                    .current_dir(&self.root)
                    .output()?;
                if !output2.status.success() {
                    anyhow::bail!("git worktree add failed: {}", String::from_utf8_lossy(&output2.stderr));
                }
            } else {
                anyhow::bail!("git worktree add failed: {}", stderr);
            }
        }

        Ok(wt_path)
    }

    /// Remove a worktree for an agent
    pub fn remove_worktree(&self, agent_id: &str) -> Result<()> {
        let wt_path = self.grit_dir().join("worktrees").join(agent_id);

        if !wt_path.exists() {
            anyhow::bail!("Worktree does not exist at {}", wt_path.display());
        }

        let output = Command::new("git")
            .args(["worktree", "remove", "--force", &wt_path.to_string_lossy()])
            .current_dir(&self.root)
            .output()
            .context("Failed to run git worktree remove")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git worktree remove failed: {}", stderr);
        }

        // Clean up the agent branch
        let branch_name = format!("agent/{}", agent_id);
        let _ = Command::new("git")
            .args(["branch", "-D", &branch_name])
            .current_dir(&self.root)
            .output();

        Ok(())
    }

    /// Merge an agent's worktree branch back into the current branch.
    /// Uses a file lock to serialize merges (git can't handle concurrent merges).
    pub fn merge_worktree(&self, agent_id: &str) -> Result<()> {
        let branch_name = format!("agent/{}", agent_id);
        let wt_path = self.grit_dir().join("worktrees").join(agent_id);

        if !wt_path.exists() {
            anyhow::bail!("Worktree does not exist for agent {}", agent_id);
        }

        // Commit any changes in the worktree
        let status_output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&wt_path)
            .output()?;

        let status_str = String::from_utf8_lossy(&status_output.stdout);
        if !status_str.trim().is_empty() {
            let _ = Command::new("git")
                .args(["add", "-A"])
                .current_dir(&wt_path)
                .output()?;

            let _ = Command::new("git")
                .args(["commit", "-m", &format!("grit: agent {} changes", agent_id)])
                .current_dir(&wt_path)
                .output()?;
        }

        // Acquire merge lock (serialize all merges because git can't handle concurrent ones)
        let lock_path = self.grit_dir().join("merge.lock");
        let _lock = self.acquire_file_lock(&lock_path)?;

        // Merge the agent branch into main branch
        let output = Command::new("git")
            .args(["merge", "--no-ff", &branch_name, "-m", &format!("grit: merge agent/{}", agent_id)])
            .current_dir(&self.root)
            .output()
            .context("Failed to run git merge")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Abort any failed merge state
            let _ = Command::new("git")
                .args(["merge", "--abort"])
                .current_dir(&self.root)
                .output();
            anyhow::bail!("git merge failed: {}", stderr);
        }

        Ok(())
    }

    /// Simple file-based spinlock for serializing git operations
    fn acquire_file_lock(&self, path: &Path) -> Result<FileLock> {
        let max_retries = 200; // 200 × 50ms = 10s max wait
        for attempt in 0..max_retries {
            // Try to exclusively create the lock file
            match fs::OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(file) => {
                    use std::io::Write;
                    let mut file = file;
                    let _ = write!(file, "{}", std::process::id());
                    return Ok(FileLock { path: path.to_path_buf() });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Check if the lock is stale (older than 30s)
                    if let Ok(meta) = fs::metadata(path) {
                        if let Ok(modified) = meta.modified() {
                            if modified.elapsed().unwrap_or_default().as_secs() > 30 {
                                let _ = fs::remove_file(path);
                                continue;
                            }
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => anyhow::bail!("Failed to acquire merge lock: {}", e),
            }
            if attempt > 0 && attempt % 20 == 0 {
                eprintln!("  waiting for merge lock ({} attempts)...", attempt);
            }
        }
        anyhow::bail!("Timeout acquiring merge lock after 10s")
    }

    /// List all active agent worktrees
    pub fn list_worktrees(&self) -> Result<Vec<String>> {
        let wt_dir = self.grit_dir().join("worktrees");
        if !wt_dir.exists() {
            return Ok(Vec::new());
        }

        let mut agents = Vec::new();
        for entry in std::fs::read_dir(&wt_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    agents.push(name.to_string());
                }
            }
        }
        agents.sort();
        Ok(agents)
    }
}

/// RAII file lock — automatically removed when dropped
struct FileLock {
    path: PathBuf,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
