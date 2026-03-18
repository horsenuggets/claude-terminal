//! Git worktree management for parallel Claude sessions
//!
//! When multiple Claude sessions need to work on the same repository simultaneously,
//! this module handles creating and managing git worktrees to avoid conflicts.

use anyhow::{anyhow, Context, Result};
use git2::Repository;
use std::path::{Path, PathBuf};

/// Default directory for worktrees
const DEFAULT_WORKTREES_DIR: &str = "git/worktrees";

/// Manages git worktrees for parallel sessions
pub struct WorktreeManager {
    worktrees_dir: PathBuf,
}

impl WorktreeManager {
    /// Create a new worktree manager
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;
        let worktrees_dir = home.join(DEFAULT_WORKTREES_DIR);
        std::fs::create_dir_all(&worktrees_dir)?;

        Ok(Self { worktrees_dir })
    }

    /// Create a worktree manager with a custom directory
    pub fn with_dir(worktrees_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&worktrees_dir)?;
        Ok(Self { worktrees_dir })
    }

    /// Check if the given path is inside a git repository
    pub fn find_repo(path: &Path) -> Result<Repository> {
        Repository::discover(path).context("Not a git repository")
    }

    /// Check if another session is working in this repository
    ///
    /// Returns the session IDs of any other sessions working in this repo
    pub fn check_for_conflicts(&self, repo_path: &Path) -> Result<Vec<String>> {
        let sessions_dir = dirs::home_dir()
            .ok_or_else(|| anyhow!("Could not find home directory"))?
            .join(".claude-sessions");

        if !sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut conflicts = Vec::new();
        let current_pid = std::process::id();

        for entry in std::fs::read_dir(&sessions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<SessionInfo>(&content) {
                        // Skip self
                        if session.pid == current_pid {
                            continue;
                        }

                        // Check if process is still alive
                        if !is_process_alive(session.pid) {
                            // Clean up stale session
                            let _ = std::fs::remove_file(&path);
                            continue;
                        }

                        // Check if working in same repo
                        let session_path = PathBuf::from(&session.cwd);
                        if paths_share_repo(repo_path, &session_path)? {
                            conflicts.push(session.id);
                        }
                    }
                }
            }
        }

        Ok(conflicts)
    }

    /// Create a worktree for isolated work
    ///
    /// Returns the path to the new worktree
    pub fn create_worktree(&self, repo_path: &Path, session_id: &str) -> Result<PathBuf> {
        let repo = Self::find_repo(repo_path)?;
        let repo_root = repo
            .workdir()
            .ok_or_else(|| anyhow!("Repository has no working directory"))?;

        // Get repo name from path
        let repo_name = repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo");

        // Create unique worktree name
        let short_id = &session_id[..session_id.len().min(8)];
        let worktree_name = format!("{}-{}", repo_name, short_id);
        let worktree_path = self.worktrees_dir.join(&worktree_name);

        // Check if worktree already exists
        if worktree_path.exists() {
            return Ok(worktree_path);
        }

        // Get current branch
        let head = repo.head()?;
        let _branch_name = head
            .shorthand()
            .ok_or_else(|| anyhow!("Could not get branch name"))?;

        // Create a new branch for this worktree
        let worktree_branch = format!("worktree/{}", worktree_name);
        let head_commit = head.peel_to_commit()?;

        // Create branch if it doesn't exist
        if repo.find_branch(&worktree_branch, git2::BranchType::Local).is_err() {
            repo.branch(&worktree_branch, &head_commit, false)?;
        }

        // Create the worktree
        repo.worktree(
            &worktree_name,
            &worktree_path,
            Some(
                git2::WorktreeAddOptions::new()
                    .reference(Some(&repo.find_branch(&worktree_branch, git2::BranchType::Local)?
                        .into_reference())),
            ),
        )?;

        tracing::info!(
            "Created worktree at {} for branch {}",
            worktree_path.display(),
            worktree_branch
        );

        Ok(worktree_path)
    }

    /// Ensure we have an isolated workspace, creating worktree if needed
    ///
    /// If there are conflicts (other sessions working in the same repo),
    /// creates a worktree and returns its path. Otherwise returns the original path.
    pub fn ensure_isolated_workspace(
        &self,
        repo_path: &Path,
        session_id: &str,
    ) -> Result<PathBuf> {
        // Check if this path is already a worktree
        if repo_path.starts_with(&self.worktrees_dir) {
            return Ok(repo_path.to_path_buf());
        }

        // Check for conflicts
        let conflicts = self.check_for_conflicts(repo_path)?;

        if conflicts.is_empty() {
            // No conflicts, use original path
            Ok(repo_path.to_path_buf())
        } else {
            tracing::info!(
                "Detected {} other sessions in this repo, creating worktree",
                conflicts.len()
            );
            self.create_worktree(repo_path, session_id)
        }
    }

    /// Clean up a worktree when done
    pub fn cleanup_worktree(&self, worktree_path: &Path) -> Result<()> {
        // Only clean up if it's in our worktrees directory
        if !worktree_path.starts_with(&self.worktrees_dir) {
            return Ok(());
        }

        let worktree_name = worktree_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("Invalid worktree path"))?;

        // Find the main repo by looking at .git file
        let git_file = worktree_path.join(".git");
        if git_file.exists() {
            let content = std::fs::read_to_string(&git_file)?;
            if let Some(gitdir) = content.strip_prefix("gitdir: ") {
                let gitdir = gitdir.trim();
                // gitdir points to .git/worktrees/<name>, go up to find main repo
                let main_git_dir = PathBuf::from(gitdir)
                    .parent()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_path_buf());

                if let Some(main_git_dir) = main_git_dir {
                    if let Ok(repo) = Repository::open(&main_git_dir) {
                        // Remove the worktree
                        if let Ok(worktree) = repo.find_worktree(worktree_name) {
                            // Try to prune the worktree
                            let _ = worktree.prune(Some(
                                git2::WorktreePruneOptions::new()
                                    .valid(true)
                                    .working_tree(true),
                            ));
                        }

                        // Also delete the branch we created
                        let branch_name = format!("worktree/{}", worktree_name);
                        if let Ok(mut branch) =
                            repo.find_branch(&branch_name, git2::BranchType::Local)
                        {
                            let _ = branch.delete();
                        }
                    }
                }
            }
        }

        // Remove the directory
        if worktree_path.exists() {
            std::fs::remove_dir_all(worktree_path)?;
        }

        tracing::info!("Cleaned up worktree at {}", worktree_path.display());

        Ok(())
    }

    /// List all worktrees managed by this instance
    pub fn list_worktrees(&self) -> Result<Vec<PathBuf>> {
        let mut worktrees = Vec::new();

        if self.worktrees_dir.exists() {
            for entry in std::fs::read_dir(&self.worktrees_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    worktrees.push(path);
                }
            }
        }

        Ok(worktrees)
    }
}

/// Session info structure (matches sessions/manager.rs)
#[derive(Debug, serde::Deserialize)]
struct SessionInfo {
    id: String,
    pid: u32,
    cwd: String,
}

/// Check if two paths share the same git repository
fn paths_share_repo(path1: &Path, path2: &Path) -> Result<bool> {
    let repo1 = match Repository::discover(path1) {
        Ok(r) => r,
        Err(_) => return Ok(false),
    };
    let repo2 = match Repository::discover(path2) {
        Ok(r) => r,
        Err(_) => return Ok(false),
    };

    // Compare repo roots
    let root1 = repo1.workdir().map(|p| p.canonicalize().ok()).flatten();
    let root2 = repo2.workdir().map(|p| p.canonicalize().ok()).flatten();

    match (root1, root2) {
        (Some(r1), Some(r2)) => Ok(r1 == r2),
        _ => Ok(false),
    }
}

/// Check if a process is alive
fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: kill with signal 0 just checks if process exists
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(windows)]
    {
        use std::process::Command;
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_worktree_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = WorktreeManager::with_dir(temp_dir.path().to_path_buf()).unwrap();
        assert!(temp_dir.path().exists());
    }

    #[test]
    fn test_find_repo() {
        // This test assumes we're running from within a git repo
        let cwd = std::env::current_dir().unwrap();
        let result = WorktreeManager::find_repo(&cwd);
        // May or may not be in a repo depending on where test runs
        let _ = result;
    }
}
