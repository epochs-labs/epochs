//! Git-style refs and HEAD management.

use std::fs;
use std::path::{Path, PathBuf};

use crate::branch::Branch;
use crate::error::{EpochsError, Result};
use crate::hash::Hash;

/// Local epochs repository layout on disk.
#[derive(Debug)]
pub struct Repo {
    path: PathBuf,
    head: Option<String>,
}

impl Repo {
    /// Initialize a new repository at `path`.
    pub fn init(path: &Path) -> Result<Self> {
        fs::create_dir_all(path.join("data"))?;
        fs::create_dir_all(path.join("refs/heads"))?;

        let head_path = path.join("HEAD");
        if !head_path.exists() {
            fs::write(&head_path, "ref: refs/heads/main\n")?;
        }

        Ok(Self {
            path: path.to_path_buf(),
            head: Some("main".into()),
        })
    }

    /// Open an existing repository.
    pub fn open(path: &Path) -> Result<Self> {
        if !path.join("data").exists() {
            return Err(EpochsError::Io(format!(
                "not an epochs repository: {}",
                path.display()
            )));
        }

        let head = Self::read_head(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            head,
        })
    }

    /// Repository root path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Current HEAD branch name, if set.
    pub fn head_branch(&self) -> Option<&str> {
        self.head.as_deref()
    }

    /// Set HEAD to a branch name.
    pub fn set_head(&mut self, branch: &str) -> Result<()> {
        let ref_path = self.branch_ref_path(branch);
        if !ref_path.exists() {
            return Err(EpochsError::BranchNotFound(branch.into()));
        }
        fs::write(
            self.path.join("HEAD"),
            format!("ref: refs/heads/{branch}\n"),
        )?;
        self.head = Some(branch.into());
        Ok(())
    }

    /// Read a branch ref.
    pub fn read_branch(&self, name: &str) -> Result<Branch> {
        let ref_path = self.branch_ref_path(name);
        if !ref_path.exists() {
            return Err(EpochsError::BranchNotFound(name.into()));
        }
        let contents = fs::read_to_string(&ref_path)?.trim().to_string();
        let target = Hash::from_hex(&contents)?;
        Ok(Branch {
            name: name.into(),
            target,
        })
    }

    /// Write a branch ref.
    pub fn write_branch(&self, name: &str, target: Hash) -> Result<()> {
        let ref_path = self.branch_ref_path(name);
        if let Some(parent) = ref_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&ref_path, format!("{target}\n"))?;
        Ok(())
    }

    /// Create a branch if it does not exist.
    pub fn create_branch(&self, name: &str, target: Hash) -> Result<()> {
        let ref_path = self.branch_ref_path(name);
        if ref_path.exists() {
            return Err(EpochsError::BranchExists(name.into()));
        }
        self.write_branch(name, target)
    }

    /// Update an existing branch ref.
    pub fn update_branch(&self, name: &str, target: Hash) -> Result<()> {
        if !self.branch_ref_path(name).exists() {
            return Err(EpochsError::BranchNotFound(name.into()));
        }
        self.write_branch(name, target)
    }

    /// Delete a branch ref. Refuses to delete the current HEAD branch.
    pub fn delete_branch(&mut self, name: &str) -> Result<()> {
        if self.head.as_deref() == Some(name) {
            return Err(EpochsError::InvalidTarget(format!(
                "cannot delete HEAD branch '{name}'"
            )));
        }
        let ref_path = self.branch_ref_path(name);
        if !ref_path.exists() {
            return Err(EpochsError::BranchNotFound(name.into()));
        }
        fs::remove_file(ref_path)?;
        Ok(())
    }

    /// List branch names.
    pub fn list_branches(&self) -> Result<Vec<String>> {
        let heads = self.path.join("refs/heads");
        let mut names = Vec::new();
        if heads.exists() {
            for entry in fs::read_dir(heads)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    names.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    fn branch_ref_path(&self, name: &str) -> PathBuf {
        self.path.join("refs/heads").join(name)
    }

    fn read_head(path: &Path) -> Result<Option<String>> {
        let head_path = path.join("HEAD");
        if !head_path.exists() {
            return Ok(None);
        }
        let contents = fs::read_to_string(head_path)?;
        if let Some(branch) = contents.trim().strip_prefix("ref: refs/heads/") {
            Ok(Some(branch.into()))
        } else {
            Ok(None)
        }
    }
}
