//! Canonical tool catalog — single source of truth for tool metadata.
//!
//! Stores all tool entries (internal built-ins, schema-only definitions, and
//! HTTP-installed tools) with profile and owner annotations.  Replaces the
//! ad-hoc composition previously spread across `DefaultToolRegistry`,
//! `ToolInstaller`, and the static lists in `definitions.rs`.

use crate::config::{load_tools_from_file, ToolConfigError};
use crate::definitions;
use crate::tool::builtin_tools;
use crate::ToolConfig;
use aura_core::InstalledToolDefinition;
use aura_reasoner::ToolDefinition;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::RwLock;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Who provides execution for this tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolOwner {
    /// Executed by an internal handler (built-in `Tool` impl).
    Internal,
    /// Executed via HTTP POST to an installed endpoint.
    Http,
    /// Has both internal and HTTP handlers; internal takes precedence.
    Both,
}

/// Runtime visibility profile.
///
/// `Core` ⊂ `Agent` and `Core` ⊂ `Engine` — querying for `Agent` or
/// `Engine` automatically includes all `Core` tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolProfile {
    /// Core tools only (fs, shell, search).
    Core,
    /// Chat agent: core + domain management tools.
    Agent,
    /// Task engine: core + engine-specific tools.
    Engine,
}

/// A single entry in the catalog.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub definition: ToolDefinition,
    pub owner: ToolOwner,
    /// Profiles that include this tool.
    pub profiles: Vec<ToolProfile>,
}

// ---------------------------------------------------------------------------
// ToolCatalog
// ---------------------------------------------------------------------------

/// Canonical catalog of every tool the system knows about.
///
/// Static entries are populated at construction from `definitions.rs` and
/// `builtin_tools()`.  Dynamic HTTP-installed entries can be added / removed
/// at runtime (thread-safe via `RwLock`).
pub struct ToolCatalog {
    entries: Vec<CatalogEntry>,
    installed: RwLock<HashMap<String, InstalledToolDefinition>>,
}

impl ToolCatalog {
    /// Build the default catalog from all static tool definitions.
    #[must_use]
    pub fn new() -> Self {
        let mut entries = Vec::new();
        let mut seen = HashSet::new();

        let all_profiles = vec![ToolProfile::Core, ToolProfile::Agent, ToolProfile::Engine];

        // Core tools from curated definitions (model-facing schemas).
        for def in definitions::core_tool_definitions() {
            seen.insert(def.name.clone());
            entries.push(CatalogEntry {
                definition: def,
                owner: ToolOwner::Internal,
                profiles: all_profiles.clone(),
            });
        }

        // Built-in tool impls not covered by definitions.rs (e.g. stat_file).
        for tool in builtin_tools() {
            let name = tool.name().to_string();
            if seen.insert(name) {
                entries.push(CatalogEntry {
                    definition: tool.definition(),
                    owner: ToolOwner::Internal,
                    profiles: all_profiles.clone(),
                });
            }
        }

        // Agent-only management tools (spec, task, project, dev-loop).
        for def in definitions::chat_management_tools() {
            seen.insert(def.name.clone());
            entries.push(CatalogEntry {
                definition: def,
                owner: ToolOwner::Internal,
                profiles: vec![ToolProfile::Agent],
            });
        }

        // Engine-only tools (task_done, get_task_context, submit_plan).
        for def in definitions::engine_specific_tools() {
            seen.insert(def.name.clone());
            entries.push(CatalogEntry {
                definition: def,
                owner: ToolOwner::Internal,
                profiles: vec![ToolProfile::Engine],
            });
        }

        debug!(entry_count = entries.len(), "Built tool catalog");
        Self {
            entries,
            installed: RwLock::new(HashMap::new()),
        }
    }

    // -----------------------------------------------------------------------
    // Visibility
    // -----------------------------------------------------------------------

    /// Get tool definitions for a profile **without** `ToolConfig` filtering.
    ///
    /// Includes installed HTTP tools (visible to every profile).
    #[must_use]
    pub fn tools_for_profile(&self, profile: ToolProfile) -> Vec<ToolDefinition> {
        let mut tools: Vec<ToolDefinition> = self
            .entries
            .iter()
            .filter(|e| e.profiles.contains(&profile))
            .map(|e| e.definition.clone())
            .collect();

        tools.extend(self.installed_definitions());
        tools
    }

    /// Get visible tools for a profile, filtered by `ToolConfig` permissions.
    #[must_use]
    pub fn visible_tools(&self, profile: ToolProfile, config: &ToolConfig) -> Vec<ToolDefinition> {
        let mut tools = self.tools_for_profile(profile);
        apply_config_filter(&mut tools, config);
        tools
    }

    /// Agent tools with a required `project_id` parameter (multi-project mode).
    #[must_use]
    pub fn visible_tools_multi_project(&self, config: &ToolConfig) -> Vec<ToolDefinition> {
        self.visible_tools(ToolProfile::Agent, config)
            .into_iter()
            .map(add_project_id_param)
            .collect()
    }

    // -----------------------------------------------------------------------
    // Installed (HTTP) tool management — replaces `ToolInstaller`
    // -----------------------------------------------------------------------

    /// Install (or replace) an HTTP tool definition.
    pub fn install(&self, def: InstalledToolDefinition) {
        info!(tool = %def.name, "Installing tool");
        self.installed
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(def.name.clone(), def);
    }

    /// Uninstall a tool by name. Returns `true` if it existed.
    pub fn uninstall(&self, name: &str) -> bool {
        info!(tool = %name, "Uninstalling tool");
        self.installed
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(name)
            .is_some()
    }

    /// Load installed tools from a TOML config file.
    ///
    /// # Errors
    /// Returns `ToolConfigError` if the file cannot be read or parsed.
    pub fn load_from_file(&self, path: &Path) -> Result<usize, ToolConfigError> {
        let defs = load_tools_from_file(path)?;
        let count = defs.len();
        let mut map = self
            .installed
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for def in defs {
            debug!(tool = %def.name, "Loading tool from config");
            map.insert(def.name.clone(), def);
        }
        info!(count, path = %path.display(), "Loaded tools from config file");
        Ok(count)
    }

    /// Snapshot of all installed HTTP tool definitions.
    #[must_use]
    pub fn installed_snapshot(&self) -> Vec<InstalledToolDefinition> {
        self.installed
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .cloned()
            .collect()
    }

    /// Model-facing `ToolDefinition`s for installed tools.
    #[must_use]
    pub fn installed_definitions(&self) -> Vec<ToolDefinition> {
        self.installed
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .map(|def| ToolDefinition {
                name: def.name.clone(),
                description: def.description.clone(),
                input_schema: def.input_schema.clone(),
                cache_control: None,
            })
            .collect()
    }

    /// Look up an installed tool by name.
    #[must_use]
    pub fn get_installed(&self, name: &str) -> Option<InstalledToolDefinition> {
        self.installed
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(name)
            .cloned()
    }

    /// Number of installed HTTP tools.
    #[must_use]
    pub fn installed_count(&self) -> usize {
        self.installed
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Names of all installed HTTP tools.
    #[must_use]
    pub fn installed_names(&self) -> Vec<String> {
        self.installed
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }

    /// Whether the catalog is empty (no installed tools).
    #[must_use]
    pub fn is_installed_empty(&self) -> bool {
        self.installed_count() == 0
    }

    /// Total static entry count.
    #[must_use]
    pub fn static_count(&self) -> usize {
        self.entries.len()
    }

    /// Determine the effective [`ToolOwner`] for a tool name.
    #[must_use]
    pub fn owner_of(&self, name: &str) -> Option<ToolOwner> {
        let is_static = self.entries.iter().any(|e| e.definition.name == name);
        let is_installed = self
            .installed
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(name);
        match (is_static, is_installed) {
            (true, true) => Some(ToolOwner::Both),
            (true, false) => Some(ToolOwner::Internal),
            (false, true) => Some(ToolOwner::Http),
            (false, false) => None,
        }
    }
}

impl Default for ToolCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ToolCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolCatalog")
            .field("static_entries", &self.entries.len())
            .field("installed_count", &self.installed_count())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Apply `ToolConfig` permission filters in-place.
fn apply_config_filter(tools: &mut Vec<ToolDefinition>, config: &ToolConfig) {
    const FS_TOOL_NAMES: &[&str] = &[
        "read_file",
        "write_file",
        "edit_file",
        "delete_file",
        "list_files",
        "find_files",
        "stat_file",
        "search_code",
    ];
    tools.retain(|t| {
        if !config.enable_commands && t.name == "run_command" {
            return false;
        }
        if !config.enable_fs && FS_TOOL_NAMES.contains(&t.name.as_str()) {
            return false;
        }
        true
    });
}

fn add_project_id_param(mut td: ToolDefinition) -> ToolDefinition {
    if let Some(props) = td
        .input_schema
        .get_mut("properties")
        .and_then(|p| p.as_object_mut())
    {
        props.insert(
            "project_id".to_string(),
            serde_json::json!({
                "type": "string",
                "description": "The project ID to operate on (required for multi-project context)"
            }),
        );
    }
    if let Some(req) = td.input_schema.get_mut("required") {
        if let Some(arr) = req.as_array_mut() {
            arr.insert(0, serde_json::json!("project_id"));
        }
    }
    td
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::ToolAuth;
    use std::collections::HashSet;

    #[test]
    fn catalog_has_entries() {
        let cat = ToolCatalog::new();
        assert!(cat.static_count() > 0);
    }

    #[test]
    fn core_profile_contains_fs_and_cmd() {
        let cat = ToolCatalog::new();
        let tools = cat.tools_for_profile(ToolProfile::Core);
        let names: HashSet<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains("read_file"));
        assert!(names.contains("write_file"));
        assert!(names.contains("run_command"));
        assert!(names.contains("search_code"));
    }

    #[test]
    fn agent_profile_includes_core_and_management() {
        let cat = ToolCatalog::new();
        let tools = cat.tools_for_profile(ToolProfile::Agent);
        let names: HashSet<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains("read_file"), "agent should include core");
        assert!(names.contains("list_specs"), "agent should include management");
        assert!(!names.contains("task_done"), "agent should not include engine");
    }

    #[test]
    fn engine_profile_includes_core_and_engine() {
        let cat = ToolCatalog::new();
        let tools = cat.tools_for_profile(ToolProfile::Engine);
        let names: HashSet<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains("read_file"), "engine should include core");
        assert!(names.contains("task_done"), "engine should include engine tools");
        assert!(
            !names.contains("list_specs"),
            "engine should not include management"
        );
    }

    #[test]
    fn visible_tools_filters_by_config() {
        let cat = ToolCatalog::new();
        let mut config = ToolConfig::default();
        config.enable_commands = false;
        config.enable_fs = false;

        let tools = cat.visible_tools(ToolProfile::Core, &config);
        let names: HashSet<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(!names.contains("run_command"));
        assert!(!names.contains("read_file"));
    }

    #[test]
    fn install_and_uninstall() {
        let cat = ToolCatalog::new();
        assert!(cat.is_installed_empty());

        cat.install(InstalledToolDefinition {
            name: "http_tool".into(),
            description: "test".into(),
            input_schema: serde_json::json!({"type": "object"}),
            endpoint: "http://localhost/tool".into(),
            auth: ToolAuth::None,
            timeout_ms: None,
            namespace: None,
            metadata: Default::default(),
        });
        assert_eq!(cat.installed_count(), 1);

        let tools = cat.tools_for_profile(ToolProfile::Core);
        assert!(tools.iter().any(|t| t.name == "http_tool"));

        assert!(cat.uninstall("http_tool"));
        assert!(cat.is_installed_empty());
    }

    #[test]
    fn owner_of_reports_correctly() {
        let cat = ToolCatalog::new();
        assert_eq!(cat.owner_of("read_file"), Some(ToolOwner::Internal));
        assert_eq!(cat.owner_of("nonexistent"), None);

        cat.install(InstalledToolDefinition {
            name: "ext_tool".into(),
            description: "test".into(),
            input_schema: serde_json::json!({"type": "object"}),
            endpoint: "http://localhost/ext".into(),
            auth: ToolAuth::None,
            timeout_ms: None,
            namespace: None,
            metadata: Default::default(),
        });
        assert_eq!(cat.owner_of("ext_tool"), Some(ToolOwner::Http));
    }

    #[test]
    fn multi_project_adds_project_id() {
        let cat = ToolCatalog::new();
        let config = ToolConfig::default();
        let tools = cat.visible_tools_multi_project(&config);
        for tool in &tools {
            let has_project_id = tool
                .input_schema
                .get("properties")
                .and_then(|p| p.get("project_id"))
                .is_some();
            assert!(
                has_project_id,
                "multi-project tool '{}' must have project_id",
                tool.name
            );
        }
    }

    #[test]
    fn no_duplicate_names_in_any_profile() {
        let cat = ToolCatalog::new();
        for profile in [ToolProfile::Core, ToolProfile::Agent, ToolProfile::Engine] {
            let tools = cat.tools_for_profile(profile);
            let mut seen = HashSet::new();
            for t in &tools {
                assert!(seen.insert(&t.name), "duplicate: {} in {profile:?}", t.name);
            }
        }
    }

    #[test]
    fn every_builtin_has_catalog_entry() {
        let cat = ToolCatalog::new();
        let core = cat.tools_for_profile(ToolProfile::Core);
        let names: HashSet<_> = core.iter().map(|t| t.name.as_str()).collect();
        for tool in builtin_tools() {
            assert!(
                names.contains(tool.name()),
                "builtin '{}' missing from core profile",
                tool.name()
            );
        }
    }

    #[test]
    fn load_from_file_works() {
        use std::io::Write;
        let toml = r#"
[[tool]]
name = "loaded_tool"
description = "A loaded tool"
endpoint = "http://localhost:8080/loaded"
[tool.input_schema]
type = "object"
"#;
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(toml.as_bytes()).unwrap();

        let cat = ToolCatalog::new();
        let count = cat.load_from_file(file.path()).unwrap();
        assert_eq!(count, 1);
        assert_eq!(cat.installed_count(), 1);
    }
}
