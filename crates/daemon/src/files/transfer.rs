//! File transfer with chunked download/upload and atomic writes.
//!
//! This module provides secure file transfer operations including:
//! - Chunked file downloads with offset support for resuming
//! - Chunked file uploads to temporary files
//! - Atomic file finalization using rename

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use sha2::{Digest, Sha256};
use thiserror::Error;
use tracing::warn;

use super::browser::DirectoryBrowser;

/// Default chunk size for transfers (64KB).
pub const DEFAULT_CHUNK_SIZE: u32 = 64 * 1024;

/// Maximum chunk size (1MB).
pub const MAX_CHUNK_SIZE: u32 = 1024 * 1024;

/// Errors that can occur during file transfer.
#[derive(Debug, Error)]
pub enum TransferError {
    /// The requested path is outside allowed boundaries.
    #[error("path is outside allowed boundaries: {0}")]
    PathOutsideBoundary(PathBuf),

    /// The requested file does not exist.
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),

    /// The requested path is a directory, not a file.
    #[error("path is a directory: {0}")]
    IsADirectory(PathBuf),

    /// The file already exists and overwrite is not allowed.
    #[error("file already exists: {0}")]
    FileExists(PathBuf),

    /// The requested offset is beyond the file size.
    #[error("invalid offset {offset} for file of size {file_size}")]
    InvalidOffset { offset: u64, file_size: u64 },

    /// Upload not found or expired.
    #[error("upload not found: {0}")]
    UploadNotFound(String),

    /// Chunk received out of order.
    #[error("expected chunk at offset {expected}, got {received}")]
    ChunkOutOfOrder { expected: u64, received: u64 },

    /// Checksum mismatch.
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    /// File size mismatch.
    #[error("file size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: u64, actual: u64 },

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Path validation error.
    #[error("path validation error: {0}")]
    PathValidation(String),

    /// File too large.
    #[error("file too large: {size} bytes exceeds limit of {limit} bytes")]
    FileTooLarge { size: u64, limit: u64 },

    /// Lock poisoned during operation.
    #[error("lock poisoned: {context}")]
    LockPoisoned { context: String },
}

/// State for an in-progress upload.
#[derive(Debug)]
pub struct UploadState {
    /// Destination path.
    pub destination: PathBuf,
    /// Temporary file path.
    pub temp_path: PathBuf,
    /// Expected total size.
    pub total_size: u64,
    /// Current offset (bytes written).
    pub current_offset: u64,
    /// Unix permissions mode.
    pub mode: u32,
    /// Whether to overwrite existing file.
    pub overwrite: bool,
    /// File handle.
    file: Option<File>,
    /// SHA-256 hasher for checksum verification.
    hasher: Sha256,
}

impl UploadState {
    /// Create a new upload state.
    fn new(
        destination: PathBuf,
        temp_path: PathBuf,
        total_size: u64,
        mode: u32,
        overwrite: bool,
    ) -> Self {
        Self {
            destination,
            temp_path,
            total_size,
            current_offset: 0,
            mode,
            overwrite,
            file: None,
            hasher: Sha256::new(),
        }
    }
}

/// File transfer handler.
///
/// Manages file downloads and uploads with path validation and atomic writes.
pub struct FileTransfer {
    /// Browser for path validation.
    browser: DirectoryBrowser,
    /// In-progress uploads, keyed by destination path.
    uploads: RwLock<HashMap<String, UploadState>>,
    /// Maximum file size allowed.
    max_file_size: u64,
    /// Temporary directory for uploads.
    temp_dir: PathBuf,
}

impl FileTransfer {
    /// Create a new file transfer handler.
    pub fn new(browser: DirectoryBrowser, max_file_size: u64) -> Self {
        let temp_dir = std::env::temp_dir().join("remoshell_uploads");
        Self {
            browser,
            uploads: RwLock::new(HashMap::new()),
            max_file_size,
            temp_dir,
        }
    }

    /// Set the temporary directory for uploads.
    pub fn with_temp_dir(mut self, temp_dir: PathBuf) -> Self {
        self.temp_dir = temp_dir;
        self
    }

    /// Download a chunk of a file.
    ///
    /// Returns the chunk data and whether this is the last chunk.
    pub fn download_chunk(
        &self,
        path: &Path,
        offset: u64,
        chunk_size: u32,
    ) -> Result<(Vec<u8>, u64, bool), TransferError> {
        // Validate path
        let canonical = self
            .browser
            .validate_path(path)
            .map_err(|e| TransferError::PathValidation(e.to_string()))?;

        // Get file metadata
        let metadata = fs::metadata(&canonical).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                TransferError::FileNotFound(path.to_path_buf())
            } else {
                TransferError::Io(e)
            }
        })?;

        if metadata.is_dir() {
            return Err(TransferError::IsADirectory(path.to_path_buf()));
        }

        let file_size = metadata.len();

        // Validate offset
        if offset > file_size {
            return Err(TransferError::InvalidOffset { offset, file_size });
        }

        // Open file and seek to offset
        let mut file = File::open(&canonical)?;
        file.seek(SeekFrom::Start(offset))?;

        // Read chunk
        let chunk_size = (chunk_size.min(MAX_CHUNK_SIZE) as u64).min(file_size - offset) as usize;
        let mut buffer = vec![0u8; chunk_size];
        let bytes_read = file.read(&mut buffer)?;
        buffer.truncate(bytes_read);

        let new_offset = offset + bytes_read as u64;
        let is_last = new_offset >= file_size;

        Ok((buffer, file_size, is_last))
    }

    /// Start a new file upload.
    ///
    /// Creates a temporary file and prepares for receiving chunks.
    pub fn start_upload(
        &self,
        path: &Path,
        size: u64,
        mode: u32,
        overwrite: bool,
    ) -> Result<(), TransferError> {
        // Check file size limit
        if size > self.max_file_size {
            return Err(TransferError::FileTooLarge {
                size,
                limit: self.max_file_size,
            });
        }

        // Validate destination path
        let destination = self
            .browser
            .validate_path_for_creation(path)
            .map_err(|e| TransferError::PathValidation(e.to_string()))?;

        // Check if file exists and overwrite is not allowed
        if destination.exists() && !overwrite {
            return Err(TransferError::FileExists(path.to_path_buf()));
        }

        // Create temp directory if it doesn't exist
        fs::create_dir_all(&self.temp_dir)?;

        // Create temporary file with unique name
        let temp_filename = format!(
            "upload_{:x}_{}.tmp",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            rand::random::<u32>()
        );
        let temp_path = self.temp_dir.join(temp_filename);

        // Create the upload state
        let mut state = UploadState::new(
            destination.clone(),
            temp_path.clone(),
            size,
            mode,
            overwrite,
        );

        // Create and open the temp file
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)?;

        state.file = Some(file);

        // Store the upload state
        let key = path.to_string_lossy().to_string();
        let mut uploads = self
            .uploads
            .write()
            .map_err(|_| TransferError::LockPoisoned { context: "uploads lock during start_upload".to_string() })?;

        uploads.insert(key, state);

        Ok(())
    }

    /// Write a chunk of data to an in-progress upload.
    pub fn write_chunk(&self, path: &Path, offset: u64, data: &[u8]) -> Result<(), TransferError> {
        let key = path.to_string_lossy().to_string();

        let mut uploads = self
            .uploads
            .write()
            .map_err(|_| TransferError::LockPoisoned { context: "uploads lock during write_chunk".to_string() })?;

        let state = uploads
            .get_mut(&key)
            .ok_or_else(|| TransferError::UploadNotFound(key.clone()))?;

        // Verify offset matches expected position
        if offset != state.current_offset {
            return Err(TransferError::ChunkOutOfOrder {
                expected: state.current_offset,
                received: offset,
            });
        }

        // Write data to temp file
        if let Some(ref mut file) = state.file {
            file.write_all(data)?;
        } else {
            return Err(TransferError::Io(std::io::Error::other(
                "file handle not available",
            )));
        }

        // Update hasher with chunk data
        state.hasher.update(data);

        // Update offset
        state.current_offset += data.len() as u64;

        Ok(())
    }

    /// Complete an upload by verifying checksum and atomically moving to destination.
    pub fn complete_upload(&self, path: &Path, checksum: &[u8]) -> Result<(), TransferError> {
        let key = path.to_string_lossy().to_string();

        // Remove the upload state
        let state = {
            let mut uploads = self
                .uploads
                .write()
                .map_err(|_| TransferError::LockPoisoned { context: "uploads lock during complete_upload".to_string() })?;

            uploads
                .remove(&key)
                .ok_or_else(|| TransferError::UploadNotFound(key.clone()))?
        };

        // Close the file handle
        drop(state.file);

        // Verify file size
        let metadata = fs::metadata(&state.temp_path)?;
        if metadata.len() != state.total_size {
            // Clean up temp file
            if let Err(e) = fs::remove_file(&state.temp_path) {
                warn!(path = ?state.temp_path, error = %e, "Failed to cleanup temp file after size mismatch");
            }
            return Err(TransferError::SizeMismatch {
                expected: state.total_size,
                actual: metadata.len(),
            });
        }

        // Verify checksum
        let actual_hash = state.hasher.finalize();
        if actual_hash.as_slice() != checksum {
            // Clean up temp file
            if let Err(e) = fs::remove_file(&state.temp_path) {
                warn!(path = ?state.temp_path, error = %e, "Failed to cleanup temp file after checksum mismatch");
            }
            return Err(TransferError::ChecksumMismatch {
                expected: hex::encode(checksum),
                actual: hex::encode(actual_hash),
            });
        }

        // Set file permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(state.mode);
            fs::set_permissions(&state.temp_path, perms)?;
        }

        // Atomic rename to destination
        fs::rename(&state.temp_path, &state.destination)?;

        Ok(())
    }

    /// Cancel an in-progress upload.
    pub fn cancel_upload(&self, path: &Path) -> Result<(), TransferError> {
        let key = path.to_string_lossy().to_string();

        let state = {
            let mut uploads = self
                .uploads
                .write()
                .map_err(|_| TransferError::LockPoisoned { context: "uploads lock during cancel_upload".to_string() })?;

            uploads.remove(&key)
        };

        if let Some(state) = state {
            // Clean up temp file
            if let Err(e) = fs::remove_file(&state.temp_path) {
                warn!(path = ?state.temp_path, error = %e, "Failed to cleanup temp file during upload cancellation");
            }
        }

        Ok(())
    }

    /// Get the status of an in-progress upload.
    pub fn get_upload_status(&self, path: &Path) -> Option<(u64, u64)> {
        let key = path.to_string_lossy().to_string();

        let uploads = self.uploads.read().ok()?;
        uploads.get(&key).map(|s| (s.current_offset, s.total_size))
    }

    /// Clean up stale uploads older than the given duration.
    pub fn cleanup_stale_uploads(
        &self,
        _max_age: std::time::Duration,
    ) -> Result<(), TransferError> {
        // For now, just clean up the temp directory
        if self.temp_dir.exists() {
            for entry in fs::read_dir(&self.temp_dir)? {
                let entry = entry?;
                if entry
                    .path()
                    .extension()
                    .map(|e| e == "tmp")
                    .unwrap_or(false)
                {
                    // Check if file is old enough
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(age) = modified.elapsed() {
                                if age > _max_age {
                                    if let Err(e) = fs::remove_file(entry.path()) {
                                        warn!(path = ?entry.path(), error = %e, "Failed to cleanup stale temp file");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Helper function to compute SHA-256 hash of a file.
pub fn hash_file(path: &Path) -> Result<Vec<u8>, std::io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hasher.finalize().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_download_chunk_entire_file() {
        let temp_dir = TempDir::new().unwrap();
        let content = b"Hello, World!";
        let file_path = create_test_file(temp_dir.path(), "test.txt", content);

        let browser = DirectoryBrowser::allow_all();
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024);

        let (data, total_size, is_last) = transfer.download_chunk(&file_path, 0, 1024).unwrap();

        assert_eq!(data, content);
        assert_eq!(total_size, content.len() as u64);
        assert!(is_last);
    }

    #[test]
    fn test_download_chunk_partial() {
        let temp_dir = TempDir::new().unwrap();
        let content = b"Hello, World! This is a longer file.";
        let file_path = create_test_file(temp_dir.path(), "test.txt", content);

        let browser = DirectoryBrowser::allow_all();
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024);

        // Download first chunk
        let (data1, total_size, is_last) = transfer.download_chunk(&file_path, 0, 10).unwrap();
        assert_eq!(data1, b"Hello, Wor");
        assert_eq!(total_size, content.len() as u64);
        assert!(!is_last);

        // Download second chunk
        let (data2, _, is_last) = transfer.download_chunk(&file_path, 10, 10).unwrap();
        assert_eq!(data2, b"ld! This i");
        assert!(!is_last);

        // Download last chunk
        let (data3, _, is_last) = transfer.download_chunk(&file_path, 20, 100).unwrap();
        assert_eq!(data3, b"s a longer file.");
        assert!(is_last);
    }

    #[test]
    fn test_download_chunk_with_offset() {
        let temp_dir = TempDir::new().unwrap();
        let content = b"0123456789ABCDEF";
        let file_path = create_test_file(temp_dir.path(), "test.txt", content);

        let browser = DirectoryBrowser::allow_all();
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024);

        let (data, _, _) = transfer.download_chunk(&file_path, 5, 5).unwrap();
        assert_eq!(data, b"56789");
    }

    #[test]
    fn test_download_chunk_invalid_offset() {
        let temp_dir = TempDir::new().unwrap();
        let content = b"Small file";
        let file_path = create_test_file(temp_dir.path(), "test.txt", content);

        let browser = DirectoryBrowser::allow_all();
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024);

        let result = transfer.download_chunk(&file_path, 1000, 100);
        assert!(matches!(result, Err(TransferError::InvalidOffset { .. })));
    }

    #[test]
    fn test_download_chunk_not_found() {
        let temp_dir = TempDir::new().unwrap();

        let browser = DirectoryBrowser::allow_all();
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024);

        let result = transfer.download_chunk(&temp_dir.path().join("nonexistent.txt"), 0, 100);
        assert!(matches!(result, Err(TransferError::PathValidation(_))));
    }

    #[test]
    fn test_download_chunk_is_directory() {
        let temp_dir = TempDir::new().unwrap();
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();

        let browser = DirectoryBrowser::allow_all();
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024);

        let result = transfer.download_chunk(&subdir, 0, 100);
        assert!(matches!(result, Err(TransferError::IsADirectory(_))));
    }

    #[test]
    fn test_upload_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let upload_dir = temp_dir.path().join("uploads");
        fs::create_dir_all(&upload_dir).unwrap();

        let content = b"Hello, this is uploaded content!";
        let checksum = {
            let mut hasher = Sha256::new();
            hasher.update(content);
            hasher.finalize().to_vec()
        };

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024)
            .with_temp_dir(temp_dir.path().join("tmp"));

        let dest_path = upload_dir.join("uploaded.txt");

        // Start upload
        transfer
            .start_upload(&dest_path, content.len() as u64, 0o644, false)
            .unwrap();

        // Write chunks
        transfer.write_chunk(&dest_path, 0, &content[..16]).unwrap();
        transfer
            .write_chunk(&dest_path, 16, &content[16..])
            .unwrap();

        // Verify upload status
        let (offset, total) = transfer.get_upload_status(&dest_path).unwrap();
        assert_eq!(offset, content.len() as u64);
        assert_eq!(total, content.len() as u64);

        // Complete upload
        transfer.complete_upload(&dest_path, &checksum).unwrap();

        // Verify file exists with correct content
        let uploaded_content = fs::read(&dest_path).unwrap();
        assert_eq!(uploaded_content, content);
    }

    #[test]
    fn test_upload_checksum_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let upload_dir = temp_dir.path().join("uploads");
        fs::create_dir_all(&upload_dir).unwrap();

        let content = b"Test content";
        let wrong_checksum = vec![0u8; 32]; // Wrong checksum

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024)
            .with_temp_dir(temp_dir.path().join("tmp"));

        let dest_path = upload_dir.join("bad_checksum.txt");

        transfer
            .start_upload(&dest_path, content.len() as u64, 0o644, false)
            .unwrap();
        transfer.write_chunk(&dest_path, 0, content).unwrap();

        let result = transfer.complete_upload(&dest_path, &wrong_checksum);
        assert!(matches!(
            result,
            Err(TransferError::ChecksumMismatch { .. })
        ));

        // File should not exist
        assert!(!dest_path.exists());
    }

    #[test]
    fn test_upload_chunk_out_of_order() {
        let temp_dir = TempDir::new().unwrap();
        let upload_dir = temp_dir.path().join("uploads");
        fs::create_dir_all(&upload_dir).unwrap();

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024)
            .with_temp_dir(temp_dir.path().join("tmp"));

        let dest_path = upload_dir.join("out_of_order.txt");

        transfer
            .start_upload(&dest_path, 100, 0o644, false)
            .unwrap();

        // Try to write chunk at wrong offset
        let result = transfer.write_chunk(&dest_path, 50, b"wrong offset");
        assert!(matches!(result, Err(TransferError::ChunkOutOfOrder { .. })));
    }

    #[test]
    fn test_upload_file_exists_no_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let existing_file = create_test_file(temp_dir.path(), "existing.txt", b"existing");

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024);

        let result = transfer.start_upload(&existing_file, 10, 0o644, false);
        assert!(matches!(result, Err(TransferError::FileExists(_))));
    }

    #[test]
    fn test_upload_file_exists_with_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let existing_file = create_test_file(temp_dir.path(), "existing.txt", b"old content");

        let content = b"new content";
        let checksum = {
            let mut hasher = Sha256::new();
            hasher.update(content);
            hasher.finalize().to_vec()
        };

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024)
            .with_temp_dir(temp_dir.path().join("tmp"));

        transfer
            .start_upload(&existing_file, content.len() as u64, 0o644, true)
            .unwrap();
        transfer.write_chunk(&existing_file, 0, content).unwrap();
        transfer.complete_upload(&existing_file, &checksum).unwrap();

        let final_content = fs::read(&existing_file).unwrap();
        assert_eq!(final_content, content);
    }

    #[test]
    fn test_upload_file_too_large() {
        let temp_dir = TempDir::new().unwrap();

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let transfer = FileTransfer::new(browser, 1024); // 1KB limit

        let dest_path = temp_dir.path().join("large.txt");
        let result = transfer.start_upload(&dest_path, 10 * 1024, 0o644, false);
        assert!(matches!(result, Err(TransferError::FileTooLarge { .. })));
    }

    #[test]
    fn test_upload_not_found() {
        let temp_dir = TempDir::new().unwrap();

        let browser = DirectoryBrowser::allow_all();
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024);

        let result = transfer.write_chunk(&temp_dir.path().join("nonexistent.txt"), 0, b"data");
        assert!(matches!(result, Err(TransferError::UploadNotFound(_))));
    }

    #[test]
    fn test_cancel_upload() {
        let temp_dir = TempDir::new().unwrap();
        let upload_dir = temp_dir.path().join("uploads");
        fs::create_dir_all(&upload_dir).unwrap();

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024)
            .with_temp_dir(temp_dir.path().join("tmp"));

        let dest_path = upload_dir.join("cancelled.txt");

        transfer
            .start_upload(&dest_path, 100, 0o644, false)
            .unwrap();
        transfer
            .write_chunk(&dest_path, 0, b"partial data")
            .unwrap();

        // Cancel the upload
        transfer.cancel_upload(&dest_path).unwrap();

        // Upload status should be gone
        assert!(transfer.get_upload_status(&dest_path).is_none());

        // File should not exist
        assert!(!dest_path.exists());
    }

    #[test]
    fn test_hash_file() {
        let temp_dir = TempDir::new().unwrap();
        let content = b"Hello, World!";
        let file_path = create_test_file(temp_dir.path(), "test.txt", content);

        let hash = hash_file(&file_path).unwrap();

        // Verify against known SHA-256 hash
        let expected: [u8; 32] = [
            0xdf, 0xfd, 0x60, 0x21, 0xbb, 0x2b, 0xd5, 0xb0, 0xaf, 0x67, 0x62, 0x90, 0x80, 0x9e,
            0xc3, 0xa5, 0x31, 0x91, 0xdd, 0x81, 0xc7, 0xf7, 0x0a, 0x4b, 0x28, 0x68, 0x8a, 0x36,
            0x21, 0x82, 0x98, 0x6f,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_download_path_outside_boundary() {
        let temp_dir = TempDir::new().unwrap();
        let other_dir = TempDir::new().unwrap();
        create_test_file(other_dir.path(), "secret.txt", b"secret");

        let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let transfer = FileTransfer::new(browser, 100 * 1024 * 1024);

        let result = transfer.download_chunk(&other_dir.path().join("secret.txt"), 0, 100);
        assert!(matches!(result, Err(TransferError::PathValidation(_))));
    }
}
