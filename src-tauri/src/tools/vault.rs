use crate::db::{Db, PreferenceOperations};
use crate::tools::ToolError;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const PREF_VAULT_ROOT: &str = "plugins.files.vault_root";

pub struct VaultPath {
    pub full_path: PathBuf,
    pub display_path: String,
}

pub fn get_vault_root(db: &Db) -> Result<PathBuf, ToolError> {
    let root = PreferenceOperations::get_preference(db, PREF_VAULT_ROOT)
        .map_err(|err| ToolError::new(format!("Failed to load vault root: {err}")))?;
    let root = root.ok_or_else(|| ToolError::new("Vault root is not configured"))?;
    let root_path = PathBuf::from(root);
    let root_path = root_path
        .canonicalize()
        .map_err(|err| ToolError::new(format!("Invalid vault root: {err}")))?;
    if !root_path.is_dir() {
        return Err(ToolError::new("Vault root is not a directory"));
    }
    Ok(root_path)
}

pub fn resolve_vault_path(db: &Db, input: &str) -> Result<VaultPath, ToolError> {
    let root = get_vault_root(db)?;
    let relative = normalize_vault_input(&root, input)?;
    reject_symlink_components(&root, &relative)?;
    let full_path = root.join(&relative);
    ensure_inside_root(&root, &full_path)?;
    let display_path = to_display_path(&root, &full_path);
    Ok(VaultPath {
        full_path,
        display_path,
    })
}

pub fn normalize_relative_path(input: &str) -> Result<PathBuf, ToolError> {
    if input.trim().is_empty() {
        return Err(ToolError::new("Path is required"));
    }
    let path = Path::new(input);
    if path.is_absolute() {
        return Err(ToolError::new("Absolute paths are not allowed"));
    }
    normalize_relative_path_buf(path)
}

fn normalize_vault_input(root: &Path, input: &str) -> Result<PathBuf, ToolError> {
    if input.trim().is_empty() {
        return Err(ToolError::new("Path is required"));
    }
    let path = Path::new(input);
    if path.is_absolute() {
        if !path.starts_with(root) {
            return Err(ToolError::new("Path escapes vault root"));
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| ToolError::new("Path escapes vault root"))?;
        return normalize_relative_path_buf(relative);
    }
    normalize_relative_path_buf(path)
}

fn normalize_relative_path_buf(path: &Path) -> Result<PathBuf, ToolError> {
    if path.as_os_str().is_empty() {
        return Err(ToolError::new("Path is required"));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ToolError::new("Path traversal is not allowed"));
            }
        }
    }
    Ok(path.to_path_buf())
}

pub fn reject_symlink_components(root: &Path, relative: &Path) -> Result<(), ToolError> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        if let Component::Normal(part) = component {
            current.push(part);
            if current.exists() {
                let metadata = fs::symlink_metadata(&current)
                    .map_err(|err| ToolError::new(format!("Failed to inspect path: {err}")))?;
                if metadata.file_type().is_symlink() {
                    return Err(ToolError::new("Symlink paths are not allowed"));
                }
            }
        }
    }
    Ok(())
}

pub fn ensure_inside_root(root: &Path, candidate: &Path) -> Result<(), ToolError> {
    if candidate.exists() {
        let canonical = candidate
            .canonicalize()
            .map_err(|err| ToolError::new(format!("Failed to resolve path: {err}")))?;
        if !canonical.starts_with(root) {
            return Err(ToolError::new("Path escapes vault root"));
        }
        return Ok(());
    }

    if let Some(parent) = candidate.parent() {
        if parent.exists() {
            let canonical = parent
                .canonicalize()
                .map_err(|err| ToolError::new(format!("Failed to resolve path: {err}")))?;
            if !canonical.starts_with(root) {
                return Err(ToolError::new("Path escapes vault root"));
            }
        }
    }
    Ok(())
}

pub fn to_display_path(root: &Path, full: &Path) -> String {
    let relative = full.strip_prefix(root).unwrap_or(full);
    relative
        .to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

pub fn ensure_parent_dirs(path: &Path) -> Result<(), ToolError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| ToolError::new(format!("Failed to create directories: {err}")))?;
    }
    Ok(())
}
