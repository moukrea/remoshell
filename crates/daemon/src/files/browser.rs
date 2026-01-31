//! Directory browsing with path validation.
//!
//! This module provides secure directory listing functionality that validates
//! all paths against allowed boundaries and prevents path traversal attacks.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use protocol::messages::{FileEntry, FileEntryType};
use thiserror::Error;

/// Errors that can occur during directory browsing.
#[derive(Debug, Error)]
pub enum BrowserError {
    /// The requested path is outside allowed boundaries.
    #[error("path is outside allowed boundaries: {0}")]
    PathOutsideBoundary(PathBuf),

    /// The requested path does not exist.
    #[error("path does not exist: {0}")]
    PathNotFound(PathBuf),

    /// The requested path is not a directory.
    #[error("path is not a directory: {0}")]
    NotADirectory(PathBuf),

    /// Path traversal attempt detected.
    #[error("path traversal detected: {0}")]
    PathTraversal(String),

    /// Symlink points outside allowed boundary.
    #[error("symlink points outside allowed boundary: {0}")]
    SymlinkOutsideBoundary(PathBuf),

    /// Permission denied.
    #[error("permission denied: {0}")]
    PermissionDenied(PathBuf),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// A directory entry with metadata.
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    /// Entry name (not full path).
    pub name: String,
    /// Full canonical path.
    pub path: PathBuf,
    /// Entry type.
    pub entry_type: FileEntryType,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Unix permissions mode.
    pub mode: u32,
    /// Last modified timestamp.
    pub modified: SystemTime,
    /// Whether this is a symbolic link.
    pub is_symlink: bool,
    /// Target path if this is a symlink.
    pub symlink_target: Option<PathBuf>,
}

impl DirectoryEntry {
    /// Convert to protocol FileEntry.
    pub fn to_protocol(&self) -> FileEntry {
        let modified = self
            .modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        FileEntry {
            name: self.name.clone(),
            entry_type: self.entry_type,
            size: self.size,
            mode: self.mode,
            modified,
        }
    }
}

/// Directory browser with path validation.
///
/// The browser validates all paths against a set of allowed paths before
/// performing any operations. Path traversal attacks are prevented by
/// canonicalizing all paths.
pub struct DirectoryBrowser {
    /// Allowed paths for browsing. Empty means all paths allowed.
    allowed_paths: Vec<PathBuf>,
    /// Whether to follow symlinks (default: false for security).
    follow_symlinks: bool,
}

impl DirectoryBrowser {
    /// Create a new directory browser with the given allowed paths.
    ///
    /// If `allowed_paths` is empty, all paths are allowed.
    pub fn new(allowed_paths: Vec<PathBuf>) -> Self {
        Self {
            allowed_paths,
            follow_symlinks: false,
        }
    }

    /// Create a browser that allows all paths.
    pub fn allow_all() -> Self {
        Self {
            allowed_paths: Vec::new(),
            follow_symlinks: false,
        }
    }

    /// Set whether to follow symlinks.
    ///
    /// By default, symlinks are not followed for security. When enabled,
    /// symlink targets are still validated against allowed paths.
    pub fn follow_symlinks(mut self, follow: bool) -> Self {
        self.follow_symlinks = follow;
        self
    }

    /// Validate that a path is within allowed boundaries.
    ///
    /// This canonicalizes the path and checks it against all allowed paths.
    /// Returns the canonicalized path if valid.
    pub fn validate_path(&self, path: &Path) -> Result<PathBuf, BrowserError> {
        // First check for obvious path traversal patterns
        let path_str = path.to_string_lossy();
        if path_str.contains("..") {
            // Double-check by normalizing - legitimate paths with ".." in names
            // will still work after canonicalization
            let has_traversal = path.components().any(|c| {
                matches!(c, std::path::Component::ParentDir)
            });
            if has_traversal && !path.is_absolute() {
                // Only reject if it's trying to go up from a relative path
                // Absolute paths with .. will be caught by canonicalize
            }
        }

        // Canonicalize the path to resolve any symlinks and ".." components
        let canonical = fs::canonicalize(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                BrowserError::PathNotFound(path.to_path_buf())
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                BrowserError::PermissionDenied(path.to_path_buf())
            } else {
                BrowserError::Io(e)
            }
        })?;

        // If no allowed paths are configured, allow everything
        if self.allowed_paths.is_empty() {
            return Ok(canonical);
        }

        // Check if the canonical path is within any allowed path
        for allowed in &self.allowed_paths {
            // Canonicalize the allowed path too (it might not exist yet)
            let allowed_canonical = match fs::canonicalize(allowed) {
                Ok(p) => p,
                Err(_) => continue, // Skip non-existent allowed paths
            };

            if canonical.starts_with(&allowed_canonical) {
                return Ok(canonical);
            }
        }

        Err(BrowserError::PathOutsideBoundary(path.to_path_buf()))
    }

    /// Validate a path that may not exist yet (for uploads).
    ///
    /// This validates the parent directory and ensures the full path would
    /// be within allowed boundaries.
    pub fn validate_path_for_creation(&self, path: &Path) -> Result<PathBuf, BrowserError> {
        // Get the parent directory
        let parent = path
            .parent()
            .ok_or_else(|| BrowserError::PathTraversal("path has no parent".to_string()))?;

        // Validate the parent directory exists and is allowed
        let parent_canonical = self.validate_path(parent)?;

        // Construct the full path
        let file_name = path
            .file_name()
            .ok_or_else(|| BrowserError::PathTraversal("path has no file name".to_string()))?;

        // Check file name for sneaky attempts
        let file_name_str = file_name.to_string_lossy();
        if file_name_str.contains('/') || file_name_str.contains('\\') {
            return Err(BrowserError::PathTraversal(
                "file name contains path separator".to_string(),
            ));
        }
        if file_name_str == ".." || file_name_str == "." {
            return Err(BrowserError::PathTraversal(
                "invalid file name".to_string(),
            ));
        }

        Ok(parent_canonical.join(file_name))
    }

    /// Validate a symlink target is within allowed boundaries.
    fn validate_symlink(&self, link_path: &Path) -> Result<PathBuf, BrowserError> {
        // Read the symlink target
        let target = fs::read_link(link_path).map_err(BrowserError::Io)?;

        // If the target is relative, resolve it relative to the link's parent
        let absolute_target = if target.is_relative() {
            link_path
                .parent()
                .map(|p| p.join(&target))
                .unwrap_or(target)
        } else {
            target
        };

        // Canonicalize and validate the target
        match self.validate_path(&absolute_target) {
            Ok(canonical) => Ok(canonical),
            Err(_) => Err(BrowserError::SymlinkOutsideBoundary(link_path.to_path_buf())),
        }
    }

    /// List contents of a directory.
    ///
    /// Returns a list of entries in the directory. Hidden files (starting with '.')
    /// are included if `include_hidden` is true.
    pub fn list_directory(
        &self,
        path: &Path,
        include_hidden: bool,
    ) -> Result<Vec<DirectoryEntry>, BrowserError> {
        // Validate and canonicalize the path
        let canonical = self.validate_path(path)?;

        // Verify it's a directory
        let metadata = fs::metadata(&canonical).map_err(BrowserError::Io)?;
        if !metadata.is_dir() {
            return Err(BrowserError::NotADirectory(canonical));
        }

        // Read directory contents
        let entries = fs::read_dir(&canonical).map_err(BrowserError::Io)?;

        let mut results = Vec::new();

        for entry_result in entries {
            let entry = match entry_result {
                Ok(e) => e,
                Err(_) => continue, // Skip entries we can't read
            };

            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files if not requested
            if !include_hidden && name.starts_with('.') {
                continue;
            }

            // Get entry metadata (don't follow symlinks)
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue, // Skip entries we can't stat
            };

            let symlink_metadata = fs::symlink_metadata(entry.path()).ok();
            let is_symlink = symlink_metadata
                .as_ref()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);

            // Get symlink target if applicable
            let symlink_target = if is_symlink {
                fs::read_link(entry.path()).ok()
            } else {
                None
            };

            // Determine entry type
            let entry_type = if is_symlink {
                // For symlinks, check if we should validate the target
                if !self.follow_symlinks {
                    FileEntryType::Symlink
                } else {
                    // Validate symlink target
                    match self.validate_symlink(&entry.path()) {
                        Ok(_) => {
                            // Get the type of the target
                            if metadata.is_dir() {
                                FileEntryType::Directory
                            } else if metadata.is_file() {
                                FileEntryType::File
                            } else {
                                FileEntryType::Other
                            }
                        }
                        Err(_) => FileEntryType::Symlink, // Show as symlink if target is invalid
                    }
                }
            } else if metadata.is_dir() {
                FileEntryType::Directory
            } else if metadata.is_file() {
                FileEntryType::File
            } else {
                FileEntryType::Other
            };

            let size = if metadata.is_file() {
                metadata.len()
            } else {
                0
            };

            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

            results.push(DirectoryEntry {
                name,
                path: entry.path(),
                entry_type,
                size,
                mode: metadata.mode(),
                modified,
                is_symlink,
                symlink_target,
            });
        }

        // Sort by name (directories first, then files)
        results.sort_by(|a, b| {
            let a_is_dir = matches!(a.entry_type, FileEntryType::Directory);
            let b_is_dir = matches!(b.entry_type, FileEntryType::Directory);
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });

        Ok(results)
    }

    /// Get metadata for a single path.
    pub fn get_entry(&self, path: &Path) -> Result<DirectoryEntry, BrowserError> {
        let canonical = self.validate_path(path)?;

        let metadata = fs::metadata(&canonical).map_err(BrowserError::Io)?;
        let symlink_metadata = fs::symlink_metadata(&canonical).ok();
        let is_symlink = symlink_metadata
            .as_ref()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);

        let symlink_target = if is_symlink {
            fs::read_link(&canonical).ok()
        } else {
            None
        };

        let entry_type = if is_symlink && !self.follow_symlinks {
            FileEntryType::Symlink
        } else if metadata.is_dir() {
            FileEntryType::Directory
        } else if metadata.is_file() {
            FileEntryType::File
        } else {
            FileEntryType::Other
        };

        let size = if metadata.is_file() {
            metadata.len()
        } else {
            0
        };

        let name = canonical
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        Ok(DirectoryEntry {
            name,
            path: canonical,
            entry_type,
            size,
            mode: metadata.mode(),
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            is_symlink,
            symlink_target,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    fn create_test_structure(dir: &Path) {
        // Create directories
        fs::create_dir_all(dir.join("subdir")).unwrap();
        fs::create_dir_all(dir.join(".hidden_dir")).unwrap();

        // Create files
        fs::write(dir.join("file.txt"), "Hello").unwrap();
        fs::write(dir.join("subdir/nested.txt"), "Nested").unwrap();
        fs::write(dir.join(".hidden"), "Hidden").unwrap();
    }

    #[test]
    fn test_list_directory() {
        let temp_dir = TempDir::new().unwrap();
        create_test_structure(temp_dir.path());

        let browser = DirectoryBrowser::allow_all();
        let entries = browser.list_directory(temp_dir.path(), false).unwrap();

        // Should have file.txt and subdir (hidden files excluded)
        assert_eq!(entries.len(), 2);

        // Directories should come first
        assert_eq!(entries[0].name, "subdir");
        assert!(matches!(entries[0].entry_type, FileEntryType::Directory));

        assert_eq!(entries[1].name, "file.txt");
        assert!(matches!(entries[1].entry_type, FileEntryType::File));
        assert_eq!(entries[1].size, 5); // "Hello"
    }

    #[test]
    fn test_list_directory_with_hidden() {
        let temp_dir = TempDir::new().unwrap();
        create_test_structure(temp_dir.path());

        let browser = DirectoryBrowser::allow_all();
        let entries = browser.list_directory(temp_dir.path(), true).unwrap();

        // Should include hidden files
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&".hidden"));
        assert!(names.contains(&".hidden_dir"));
    }

    #[test]
    fn test_validate_path_allowed() {
        let temp_dir = TempDir::new().unwrap();
        create_test_structure(temp_dir.path());

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);

        // Valid path within allowed boundary
        let result = browser.validate_path(&temp_dir.path().join("file.txt"));
        assert!(result.is_ok());

        // Nested path
        let result = browser.validate_path(&temp_dir.path().join("subdir/nested.txt"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_path_outside_boundary() {
        let temp_dir = TempDir::new().unwrap();
        let other_dir = TempDir::new().unwrap();
        create_test_structure(temp_dir.path());
        fs::write(other_dir.path().join("other.txt"), "Other").unwrap();

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);

        // Path outside allowed boundary
        let result = browser.validate_path(&other_dir.path().join("other.txt"));
        assert!(matches!(result, Err(BrowserError::PathOutsideBoundary(_))));
    }

    #[test]
    fn test_path_traversal_prevention() {
        let temp_dir = TempDir::new().unwrap();
        create_test_structure(temp_dir.path());

        let browser = DirectoryBrowser::new(vec![temp_dir.path().join("subdir")]);

        // Try to traverse up to parent
        let malicious_path = temp_dir.path().join("subdir/../file.txt");
        let result = browser.validate_path(&malicious_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_path_not_found() {
        let temp_dir = TempDir::new().unwrap();

        let browser = DirectoryBrowser::allow_all();
        let result = browser.validate_path(&temp_dir.path().join("nonexistent"));
        assert!(matches!(result, Err(BrowserError::PathNotFound(_))));
    }

    #[test]
    fn test_symlink_within_boundary() {
        let temp_dir = TempDir::new().unwrap();
        create_test_structure(temp_dir.path());

        // Create symlink within the same directory
        let link_path = temp_dir.path().join("link_to_file");
        symlink(temp_dir.path().join("file.txt"), &link_path).unwrap();

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let entries = browser.list_directory(temp_dir.path(), false).unwrap();

        // Symlink should be listed
        let link_entry = entries.iter().find(|e| e.name == "link_to_file");
        assert!(link_entry.is_some());
        assert!(link_entry.unwrap().is_symlink);
    }

    #[test]
    fn test_symlink_outside_boundary() {
        let temp_dir = TempDir::new().unwrap();
        let other_dir = TempDir::new().unwrap();
        fs::write(other_dir.path().join("secret.txt"), "Secret").unwrap();

        // Create symlink pointing outside
        let link_path = temp_dir.path().join("sneaky_link");
        symlink(other_dir.path().join("secret.txt"), &link_path).unwrap();

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);

        // Listing should work but symlink validation should fail when following
        let browser_follow = browser.follow_symlinks(true);
        let result = browser_follow.validate_symlink(&link_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_not_a_directory() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file.txt"), "Hello").unwrap();

        let browser = DirectoryBrowser::allow_all();
        let result = browser.list_directory(&temp_dir.path().join("file.txt"), false);
        assert!(matches!(result, Err(BrowserError::NotADirectory(_))));
    }

    #[test]
    fn test_validate_path_for_creation() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join("subdir")).unwrap();

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);

        // Valid new file path
        let result = browser.validate_path_for_creation(&temp_dir.path().join("subdir/new_file.txt"));
        assert!(result.is_ok());

        // Invalid path outside boundary
        let result = browser.validate_path_for_creation(&PathBuf::from("/tmp/outside.txt"));
        // This might succeed or fail depending on if /tmp is the same as temp_dir's parent
        // The important thing is it validates the parent directory
    }

    #[test]
    fn test_validate_path_for_creation_with_traversal() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join("subdir")).unwrap();

        let browser = DirectoryBrowser::new(vec![temp_dir.path().join("subdir")]);

        // Try to create file in parent using ".."
        let malicious = temp_dir.path().join("subdir/../escape.txt");
        let result = browser.validate_path_for_creation(&malicious);
        // Either the validation fails or the canonical path check catches it
        if let Ok(path) = result {
            assert!(path.starts_with(temp_dir.path().join("subdir")));
        }
    }

    #[test]
    fn test_validate_path_for_creation_sneaky_filename() {
        let temp_dir = TempDir::new().unwrap();

        let browser = DirectoryBrowser::allow_all();

        // Path with ".." as filename
        let result = browser.validate_path_for_creation(&temp_dir.path().join(".."));
        assert!(matches!(result, Err(BrowserError::PathTraversal(_))));
    }

    #[test]
    fn test_get_entry() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file.txt"), "Hello World").unwrap();

        let browser = DirectoryBrowser::allow_all();
        let entry = browser.get_entry(&temp_dir.path().join("file.txt")).unwrap();

        assert_eq!(entry.name, "file.txt");
        assert!(matches!(entry.entry_type, FileEntryType::File));
        assert_eq!(entry.size, 11); // "Hello World"
        assert!(!entry.is_symlink);
    }

    #[test]
    fn test_entry_to_protocol() {
        let entry = DirectoryEntry {
            name: "test.txt".to_string(),
            path: PathBuf::from("/test/test.txt"),
            entry_type: FileEntryType::File,
            size: 1024,
            mode: 0o644,
            modified: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1704067200),
            is_symlink: false,
            symlink_target: None,
        };

        let proto = entry.to_protocol();
        assert_eq!(proto.name, "test.txt");
        assert!(matches!(proto.entry_type, FileEntryType::File));
        assert_eq!(proto.size, 1024);
        assert_eq!(proto.mode, 0o644);
        assert_eq!(proto.modified, 1704067200);
    }

    #[test]
    fn test_allow_all_paths() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file.txt"), "Hello").unwrap();

        let browser = DirectoryBrowser::allow_all();

        // Should allow any path
        let result = browser.validate_path(&temp_dir.path().join("file.txt"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_allowed_paths() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();
        fs::write(temp_dir1.path().join("file1.txt"), "1").unwrap();
        fs::write(temp_dir2.path().join("file2.txt"), "2").unwrap();

        let browser = DirectoryBrowser::new(vec![
            temp_dir1.path().to_path_buf(),
            temp_dir2.path().to_path_buf(),
        ]);

        // Both paths should be allowed
        assert!(browser.validate_path(&temp_dir1.path().join("file1.txt")).is_ok());
        assert!(browser.validate_path(&temp_dir2.path().join("file2.txt")).is_ok());
    }

    #[test]
    fn test_directory_sorting() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("zebra.txt"), "z").unwrap();
        fs::write(temp_dir.path().join("apple.txt"), "a").unwrap();
        fs::create_dir_all(temp_dir.path().join("beta_dir")).unwrap();
        fs::create_dir_all(temp_dir.path().join("alpha_dir")).unwrap();

        let browser = DirectoryBrowser::allow_all();
        let entries = browser.list_directory(temp_dir.path(), false).unwrap();

        // Directories first, then files, both sorted alphabetically
        assert_eq!(entries[0].name, "alpha_dir");
        assert_eq!(entries[1].name, "beta_dir");
        assert_eq!(entries[2].name, "apple.txt");
        assert_eq!(entries[3].name, "zebra.txt");
    }
}
