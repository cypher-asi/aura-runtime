//! Message sanitization — repairs message history for API validity.
//!
//! Runs 6 passes:
//! 1. Remove empty messages
//! 2. Merge consecutive same-role messages
//! 3. Fix orphan tool results (`tool_result` without matching `tool_use`)
//! 4. Fix unpaired tool uses (`tool_use` without matching `tool_result`)
//! 5. Ensure conversation starts with a user message
//! 6. Assert positional tool_use/tool_result constraint (debug guard)

use aura_reasoner::{ContentBlock, Message, Role, ToolResultContent};
use std::collections::HashSet;
use tracing::warn;

/// Run all sanitization passes on the message history.
pub fn validate_and_repair(messages: &mut Vec<Message>) {
    remove_empty_messages(messages);
    merge_consecutive_same_role(messages);
    fix_orphan_tool_results(messages);
    fix_unpaired_tool_uses(messages);
    ensure_starts_with_user(messages);
    debug_assert_tool_pairing(messages);
}

/// Pass 1: Remove messages with no content blocks or only empty text.
fn remove_empty_messages(messages: &mut Vec<Message>) {
    messages.retain(|msg| {
        !msg.content.is_empty()
            && msg.content.iter().any(|block| match block {
                ContentBlock::Text { text } => !text.trim().is_empty(),
                _ => true,
            })
    });
}

/// Pass 2: Merge consecutive messages with the same role.
fn merge_consecutive_same_role(messages: &mut Vec<Message>) {
    if messages.len() < 2 {
        return;
    }

    let mut i = 0;
    while i + 1 < messages.len() {
        if messages[i].role == messages[i + 1].role {
            let next_content = messages[i + 1].content.clone();
            messages[i].content.extend(next_content);
            messages.remove(i + 1);
        } else {
            i += 1;
        }
    }
}

/// Pass 3: Remove orphan tool results that don't have a matching `tool_use`.
fn fix_orphan_tool_results(messages: &mut Vec<Message>) {
    let tool_use_ids: HashSet<String> = messages
        .iter()
        .flat_map(|msg| &msg.content)
        .filter_map(|block| {
            if let ContentBlock::ToolUse { id, .. } = block {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect();

    for msg in messages.iter_mut() {
        msg.content.retain(|block| {
            if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                tool_use_ids.contains(tool_use_id)
            } else {
                true
            }
        });
    }

    messages.retain(|msg| !msg.content.is_empty());
}

/// Pass 4: Ensure every assistant `tool_use` has a matching `tool_result`
/// in the **immediately following** user message (Anthropic positional rule).
///
/// Injects synthetic error results for any missing pairings.
fn fix_unpaired_tool_uses(messages: &mut Vec<Message>) {
    let mut i = 0;
    while i < messages.len() {
        if messages[i].role != Role::Assistant {
            i += 1;
            continue;
        }

        let tool_use_ids: Vec<String> = messages[i]
            .content
            .iter()
            .filter_map(|b| {
                if let ContentBlock::ToolUse { id, .. } = b {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        if tool_use_ids.is_empty() {
            i += 1;
            continue;
        }

        let existing_result_ids: HashSet<String> = messages
            .get(i + 1)
            .filter(|m| m.role == Role::User)
            .map(|m| {
                m.content
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                            Some(tool_use_id.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let missing: Vec<String> = tool_use_ids
            .into_iter()
            .filter(|id| !existing_result_ids.contains(id))
            .collect();

        if !missing.is_empty() {
            let synthetic: Vec<ContentBlock> = missing
                .into_iter()
                .map(|id| {
                    ContentBlock::tool_result(
                        &id,
                        ToolResultContent::text(
                            "[Tool result was lost during context compaction]",
                        ),
                        true,
                    )
                })
                .collect();

            if i + 1 < messages.len() && messages[i + 1].role == Role::User {
                let insert_pos = messages[i + 1].content.len();
                for (offset, block) in synthetic.into_iter().enumerate() {
                    messages[i + 1].content.insert(offset, block);
                }
                let _ = insert_pos;
            } else {
                messages.insert(i + 1, Message::new(Role::User, synthetic));
            }
        }

        i += 1;
    }
}

/// Pass 5: Ensure the conversation starts with a user message.
fn ensure_starts_with_user(messages: &mut Vec<Message>) {
    if messages.is_empty() || messages[0].role != Role::User {
        messages.insert(0, Message::user("[System: conversation context]"));
    }
}

/// Pass 6 (guard): Verify that every assistant message containing `tool_use`
/// is immediately followed by a user message containing matching `tool_result`
/// blocks.  Logs a warning on any violation so it surfaces in traces rather
/// than silently hitting the Anthropic 400 error.
fn debug_assert_tool_pairing(messages: &[Message]) {
    for (i, msg) in messages.iter().enumerate() {
        if msg.role != Role::Assistant {
            continue;
        }

        let tool_use_ids: Vec<&str> = msg
            .content
            .iter()
            .filter_map(|b| {
                if let ContentBlock::ToolUse { id, .. } = b {
                    Some(id.as_str())
                } else {
                    None
                }
            })
            .collect();

        if tool_use_ids.is_empty() {
            continue;
        }

        let next_result_ids: HashSet<&str> = messages
            .get(i + 1)
            .filter(|m| m.role == Role::User)
            .map(|m| {
                m.content
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                            Some(tool_use_id.as_str())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        for id in &tool_use_ids {
            if !next_result_ids.contains(id) {
                warn!(
                    message_index = i,
                    tool_use_id = id,
                    "Sanitization guard: tool_use without matching tool_result in next message"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_empty_messages() {
        let mut messages = vec![
            Message::user("hello"),
            Message::new(Role::User, vec![ContentBlock::text("")]),
            Message::assistant("world"),
        ];
        remove_empty_messages(&mut messages);
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_merge_consecutive_same_role() {
        let mut messages = vec![
            Message::user("hello"),
            Message::user("world"),
            Message::assistant("hi"),
        ];
        merge_consecutive_same_role(&mut messages);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content.len(), 2);
    }

    #[test]
    fn test_ensure_starts_with_user() {
        let mut messages = vec![Message::assistant("hi")];
        ensure_starts_with_user(&mut messages);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_fix_orphan_tool_results() {
        let mut messages = vec![
            Message::user("go"),
            Message::new(
                Role::User,
                vec![ContentBlock::tool_result(
                    "orphan_id",
                    ToolResultContent::text("result"),
                    false,
                )],
            ),
        ];
        fix_orphan_tool_results(&mut messages);
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_validate_and_repair_full_pipeline() {
        let mut messages = vec![
            Message::assistant("oops first"),
            Message::user(""),
            Message::user("hello"),
            Message::user("world"),
        ];
        validate_and_repair(&mut messages);
        assert_eq!(messages[0].role, Role::User);
        assert!(messages.len() >= 2);
    }
}
