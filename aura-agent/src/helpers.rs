//! Helper functions for the agent loop.

use aura_reasoner::{ContentBlock, Message, Role};

/// Push a warning message or replace the last warning if present.
///
/// This prevents accumulating multiple warning messages in the context.
pub fn push_or_replace_warning(messages: &mut Vec<Message>, warning: &str) {
    let is_warning = |text: &str| -> bool {
        text.starts_with("WARNING:")
            || text.starts_with("NOTE:")
            || text.starts_with("CRITICAL")
            || text.starts_with("STRONG WARNING:")
    };

    if let Some(last) = messages.last() {
        if last.role == Role::User {
            if let Some(ContentBlock::Text { text }) = last.content.first() {
                if is_warning(text) {
                    messages.pop();
                }
            }
        }
    }

    messages.push(Message::user(warning));
}

/// Normalize aura-app tool names to aura-harness names.
#[must_use]
pub fn normalize_tool_name(name: &str) -> &str {
    match name {
        "read_file" => "fs_read",
        "write_file" => "fs_write",
        "edit_file" => "fs_edit",
        "delete_file" => "fs_delete",
        "list_files" => "fs_ls",
        "find_files" => "fs_find",
        _ => name,
    }
}

/// Strip property descriptions from tool definitions to reduce token usage.
pub fn compact_tools(tools: &mut [aura_reasoner::ToolDefinition]) {
    for tool in tools {
        if let Some(props) = tool.input_schema.get_mut("properties") {
            if let Some(obj) = props.as_object_mut() {
                for (_, prop_schema) in obj.iter_mut() {
                    if let Some(inner) = prop_schema.as_object_mut() {
                        inner.remove("description");
                    }
                }
            }
        }
    }
}

/// Check if a tool name is a write tool (mutation).
#[must_use]
pub fn is_write_tool(name: &str) -> bool {
    crate::constants::WRITE_TOOLS.contains(&name)
}

/// Check if a tool name is an exploration tool (read-only).
#[must_use]
pub fn is_exploration_tool(name: &str) -> bool {
    crate::constants::EXPLORATION_TOOLS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_tool_name_mapping() {
        assert_eq!(normalize_tool_name("read_file"), "fs_read");
        assert_eq!(normalize_tool_name("write_file"), "fs_write");
        assert_eq!(normalize_tool_name("edit_file"), "fs_edit");
        assert_eq!(normalize_tool_name("delete_file"), "fs_delete");
        assert_eq!(normalize_tool_name("list_files"), "fs_ls");
        assert_eq!(normalize_tool_name("find_files"), "fs_find");
    }

    #[test]
    fn test_normalize_tool_name_passthrough() {
        assert_eq!(normalize_tool_name("search_code"), "search_code");
        assert_eq!(normalize_tool_name("cmd_run"), "cmd_run");
        assert_eq!(normalize_tool_name("unknown_tool"), "unknown_tool");
    }

    #[test]
    fn test_compact_tools_strips_descriptions() {
        let mut tools = vec![aura_reasoner::ToolDefinition::new(
            "test",
            "A tool",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path"
                    }
                }
            }),
        )];
        compact_tools(&mut tools);
        let props = tools[0].input_schema["properties"]["path"]
            .as_object()
            .unwrap();
        assert!(!props.contains_key("description"));
        assert!(props.contains_key("type"));
    }

    #[test]
    fn test_push_or_replace_warning() {
        let mut messages = vec![Message::user("WARNING: old warning")];
        push_or_replace_warning(&mut messages, "WARNING: new warning");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text_content(), "WARNING: new warning");
    }
}
