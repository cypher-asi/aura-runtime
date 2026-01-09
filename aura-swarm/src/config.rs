//! Swarm configuration.

use std::path::PathBuf;

/// Swarm configuration.
#[derive(Debug, Clone)]
pub struct SwarmConfig {
    /// Data directory for `RocksDB` and workspaces
    pub data_dir: PathBuf,
    /// HTTP server bind address
    pub bind_addr: String,
    /// Enable sync writes to `RocksDB`
    pub sync_writes: bool,
    /// Record window size for kernel context
    pub record_window_size: usize,
    /// Reasoner gateway URL
    pub reasoner_url: String,
    /// Reasoner timeout in milliseconds
    pub reasoner_timeout_ms: u64,
    /// Enable filesystem tools
    pub enable_fs_tools: bool,
    /// Enable command tools
    pub enable_cmd_tools: bool,
    /// Allowed commands (if cmd tools enabled)
    pub allowed_commands: Vec<String>,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./aura_data"),
            bind_addr: "127.0.0.1:8080".to_string(),
            sync_writes: false,
            record_window_size: 50,
            reasoner_url: "http://localhost:3000".to_string(),
            reasoner_timeout_ms: 30_000,
            enable_fs_tools: true,
            enable_cmd_tools: false,
            allowed_commands: vec![],
        }
    }
}

impl SwarmConfig {
    /// Load configuration from environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(val) = std::env::var("DATA_DIR") {
            config.data_dir = PathBuf::from(val);
        }
        if let Ok(val) = std::env::var("BIND_ADDR") {
            config.bind_addr = val;
        }
        if let Ok(val) = std::env::var("SYNC_WRITES") {
            config.sync_writes = val == "true" || val == "1";
        }
        if let Ok(val) = std::env::var("RECORD_WINDOW_SIZE") {
            if let Ok(n) = val.parse() {
                config.record_window_size = n;
            }
        }
        if let Ok(val) = std::env::var("REASONER_URL") {
            config.reasoner_url = val;
        }
        if let Ok(val) = std::env::var("REASONER_TIMEOUT_MS") {
            if let Ok(n) = val.parse() {
                config.reasoner_timeout_ms = n;
            }
        }
        if let Ok(val) = std::env::var("ENABLE_FS_TOOLS") {
            config.enable_fs_tools = val != "false" && val != "0";
        }
        if let Ok(val) = std::env::var("ENABLE_CMD_TOOLS") {
            config.enable_cmd_tools = val == "true" || val == "1";
        }
        if let Ok(val) = std::env::var("ALLOWED_COMMANDS") {
            config.allowed_commands = val.split(',').map(String::from).collect();
        }

        config
    }

    /// Get the `RocksDB` path.
    #[must_use]
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("db")
    }

    /// Get the workspaces base path.
    #[must_use]
    pub fn workspaces_path(&self) -> PathBuf {
        self.data_dir.join("workspaces")
    }
}
