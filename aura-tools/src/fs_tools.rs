//! Filesystem tool implementations.

use crate::error::ToolError;
use crate::sandbox::Sandbox;
use aura_core::ToolResult;
use std::collections::HashMap;
use std::fs;
use std::os::windows::fs::MetadataExt;
use tracing::{debug, instrument};

/// List directory contents.
#[instrument(skip(sandbox), fields(path = %path))]
pub fn fs_ls(sandbox: &Sandbox, path: &str) -> Result<ToolResult, ToolError> {
    let resolved = sandbox.resolve_existing(path)?;
    debug!(?resolved, "Listing directory");

    if !resolved.is_dir() {
        return Err(ToolError::InvalidArguments(format!(
            "{path} is not a directory"
        )));
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&resolved)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = entry.metadata()?;

        let entry_type = if metadata.is_dir() {
            "dir"
        } else if metadata.is_file() {
            "file"
        } else {
            "other"
        };

        entries.push(format!("{}\t{}\t{}", entry_type, metadata.len(), name));
    }

    let output = entries.join("\n");
    Ok(ToolResult::success("fs.ls", output))
}

/// Read file contents.
#[instrument(skip(sandbox), fields(path = %path, max_bytes))]
pub fn fs_read(sandbox: &Sandbox, path: &str, max_bytes: usize) -> Result<ToolResult, ToolError> {
    let resolved = sandbox.resolve_existing(path)?;
    debug!(?resolved, "Reading file");

    if !resolved.is_file() {
        return Err(ToolError::InvalidArguments(format!("{path} is not a file")));
    }

    // Check file size before reading
    let metadata = fs::metadata(&resolved)?;
    let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);

    if size > max_bytes {
        return Err(ToolError::SizeLimitExceeded {
            actual: size,
            limit: max_bytes,
        });
    }

    let contents = fs::read(&resolved)?;
    Ok(ToolResult::success("fs.read", contents).with_metadata("size", size.to_string()))
}

/// Get file metadata.
#[instrument(skip(sandbox), fields(path = %path))]
pub fn fs_stat(sandbox: &Sandbox, path: &str) -> Result<ToolResult, ToolError> {
    let resolved = sandbox.resolve_existing(path)?;
    debug!(?resolved, "Getting file stats");

    let metadata = fs::metadata(&resolved)?;

    let mut result_metadata = HashMap::new();
    result_metadata.insert("size".to_string(), metadata.len().to_string());
    result_metadata.insert("is_file".to_string(), metadata.is_file().to_string());
    result_metadata.insert("is_dir".to_string(), metadata.is_dir().to_string());
    result_metadata.insert(
        "readonly".to_string(),
        metadata.permissions().readonly().to_string(),
    );

    // Windows-specific attributes
    result_metadata.insert(
        "file_attributes".to_string(),
        metadata.file_attributes().to_string(),
    );

    // Format output as key=value pairs
    let output: Vec<String> = result_metadata
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();

    let mut tool_result = ToolResult::success("fs.stat", output.join("\n"));
    tool_result.metadata = result_metadata;
    Ok(tool_result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_sandbox() -> (Sandbox, TempDir) {
        let dir = TempDir::new().unwrap();
        let sandbox = Sandbox::new(dir.path()).unwrap();
        (sandbox, dir)
    }

    #[test]
    fn test_fs_ls() {
        let (sandbox, dir) = create_test_sandbox();

        // Create some files and dirs
        fs::write(dir.path().join("file1.txt"), "hello").unwrap();
        fs::write(dir.path().join("file2.txt"), "world").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();

        let result = fs_ls(&sandbox, ".").unwrap();
        assert!(result.ok);
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(output.contains("file1.txt"));
        assert!(output.contains("file2.txt"));
        assert!(output.contains("subdir"));
    }

    #[test]
    fn test_fs_read() {
        let (sandbox, dir) = create_test_sandbox();

        let content = "Hello, Aura!";
        fs::write(dir.path().join("test.txt"), content).unwrap();

        let result = fs_read(&sandbox, "test.txt", 1024).unwrap();
        assert!(result.ok);
        assert_eq!(&result.stdout[..], content.as_bytes());
    }

    #[test]
    fn test_fs_read_size_limit() {
        let (sandbox, dir) = create_test_sandbox();

        let content = "Hello, Aura!";
        fs::write(dir.path().join("test.txt"), content).unwrap();

        let result = fs_read(&sandbox, "test.txt", 5);
        assert!(matches!(result, Err(ToolError::SizeLimitExceeded { .. })));
    }

    #[test]
    fn test_fs_stat() {
        let (sandbox, dir) = create_test_sandbox();

        let content = "Hello!";
        fs::write(dir.path().join("test.txt"), content).unwrap();

        let result = fs_stat(&sandbox, "test.txt").unwrap();
        assert!(result.ok);
        assert_eq!(result.metadata.get("size").unwrap(), "6");
        assert_eq!(result.metadata.get("is_file").unwrap(), "true");
        assert_eq!(result.metadata.get("is_dir").unwrap(), "false");
    }
}
