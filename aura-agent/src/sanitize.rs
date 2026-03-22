//! Message sanitization — repairs message history for API validity.
//!
//! Runs 5 passes:
//! 1. Remove empty messages
//! 2. Merge consecutive same-role messages
//! 3. Fix orphan tool results (`tool_result` without matching `tool_use`)
//! 4. Fix unpaired tool uses (`tool_use` without matching `tool_result`)
//! 5. Ensure conversation starts with a user message

use aura_reasoner::{ContentBlock, Message, Role, ToolResultContent};
use std::collections::HashSet;

/// Run all sanitization passes on the message history.
pub fn validate_and_repair(messages: &mut Vec<Message>) {
    remove_empty_messages(messages);
    merge_consecutive_same_role(messages);
    fix_orphan_tool_results(messages);
    fix_unpaired_tool_uses(messages);
    ensure_starts_with_user(messages);
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

/// Pass 4: Inject synthetic error results for `tool_use` blocks that lack a `tool_result`.
fn fix_unpaired_tool_uses(messages: &mut Vec<Message>) {
    let tool_result_ids: HashSet<String> = messages
        .iter()
        .flat_map(|msg| &msg.content)
        .filter_map(|block| {
            if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                Some(tool_use_id.clone())
            } else {
                None
            }
        })
        .collect();

    let unpaired: Vec<(String, usize)> = messages
        .iter()
        .enumerate()
        .flat_map(|(idx, msg)| {
            let ids_ref = &tool_result_ids;
            msg.content.iter().filter_map(move |block| {
                if let ContentBlock::ToolUse { id, .. } = block {
                    if ids_ref.contains(id) {
                        None
                    } else {
                        Some((id.clone(), idx))
                    }
                } else {
                    None
                }
            })
        })
        .collect();

    for (id, msg_idx) in unpaired {
        let synthetic = ContentBlock::tool_result(
            id.as_str(),
            ToolResultContent::text("[Tool result was lost during context compaction]"),
            true,
        );

        let insert_idx = msg_idx + 1;
        if insert_idx < messages.len() && messages[insert_idx].role == Role::User {
            messages[insert_idx].content.insert(0, synthetic);
        } else {
            let new_msg = Message::new(Role::User, vec![synthetic]);
            let pos = (msg_idx + 1).min(messages.len());
            messages.insert(pos, new_msg);
        }
    }
}

/// Pass 5: Ensure the conversation starts with a user message.
fn ensure_starts_with_user(messages: &mut Vec<Message>) {
    if messages.is_empty() || messages[0].role != Role::User {
        messages.insert(0, Message::user("[System: conversation context]"));
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
