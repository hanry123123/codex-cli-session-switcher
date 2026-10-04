use super::*;
use pretty_assertions::assert_eq;

fn user_entry(turn_id: &str, id: &str, text: &str) -> codex_app_server_protocol::ThreadItemEntry {
    codex_app_server_protocol::ThreadItemEntry {
        turn_id: turn_id.to_string(),
        item: ThreadItem::UserMessage {
            id: id.to_string(),
            client_id: None,
            content: vec![UserInput::Text {
                text: text.to_string(),
                text_elements: Vec::new(),
            }],
        },
        started_at_ms: None,
        completed_at_ms: None,
    }
}

fn assistant_entry(
    turn_id: &str,
    id: &str,
    text: &str,
) -> codex_app_server_protocol::ThreadItemEntry {
    codex_app_server_protocol::ThreadItemEntry {
        turn_id: turn_id.to_string(),
        item: ThreadItem::AgentMessage {
            id: id.to_string(),
            text: text.to_string(),
            phase: None,
            memory_citation: None,
            delivery: None,
            questions: None,
        },
        started_at_ms: None,
        completed_at_ms: None,
    }
}

#[test]
fn summary_rounds_pair_user_and_assistant_items_by_turn() {
    let rounds = complete_rounds(&[
        assistant_entry("new", "a2", "new answer"),
        user_entry("new", "u2", "new request"),
        assistant_entry("old", "a1", "old answer"),
        user_entry("old", "u1", "old request"),
        user_entry("incomplete", "u3", "pending request"),
    ]);

    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[0].turn_id, "new");
    assert_eq!(rounds[0].user_messages, ["new request"]);
    assert_eq!(rounds[0].assistant_messages, ["new answer"]);
    assert_eq!(rounds[1].turn_id, "old");
}

#[test]
fn request_preview_uses_the_newest_user_item() {
    let request = latest_user_request(vec![
        assistant_entry("new", "a2", "new answer"),
        user_entry("new", "u2", "new request"),
        user_entry("old", "u1", "old request"),
    ]);

    assert_eq!(request, Some(Some("new request".to_string())));
}

#[test]
fn incremental_summary_waits_for_three_rounds_after_its_anchor() {
    let rounds: Vec<_> = (0..4)
        .map(|index| ConversationRound {
            turn_id: format!("turn-{index}"),
            user_messages: vec![format!("request-{index}")],
            assistant_messages: vec![format!("answer-{index}")],
        })
        .collect();

    assert!(rounds_after_anchor(&rounds[..3], "turn-2").is_none());
    assert_eq!(
        rounds_after_anchor(&rounds, "turn-3")
            .expect("three new rounds")
            .iter()
            .map(|round| round.turn_id.as_str())
            .collect::<Vec<_>>(),
        ["turn-0", "turn-1", "turn-2"]
    );
}

#[test]
fn summary_prompt_keeps_existing_summary_and_bounds_transcript() {
    let rounds = vec![ConversationRound {
        turn_id: "large".to_string(),
        user_messages: vec!["用户内容".repeat(MAX_SUMMARY_CONTEXT_CHARS)],
        assistant_messages: vec!["助手内容".repeat(MAX_SUMMARY_CONTEXT_CHARS)],
    }];
    let prompt = summary_prompt(Some("existing summary"), &rounds);
    let transcript = prompt
        .split("<conversation>\n")
        .nth(1)
        .and_then(|tail| tail.split("\n</conversation>").next())
        .expect("conversation section");

    assert!(prompt.contains("existing summary"));
    assert!(transcript.chars().count() <= MAX_SUMMARY_CONTEXT_CHARS);
}

#[tokio::test]
async fn summary_cache_round_trips_and_ignores_unknown_versions() {
    let temp_dir = tempfile::tempdir().expect("temporary directory");
    let codex_home = temp_dir.path();
    let mut entries = HashMap::new();
    entries.insert(
        "thread-1".to_string(),
        SummaryCacheEntry {
            summary: "A saved summary".to_string(),
            last_summarized_turn_id: "turn-2".to_string(),
            generated_at: 42,
        },
    );
    write_cache(
        codex_home,
        &SummaryCache {
            version: 0,
            entries,
        },
    )
    .await
    .expect("write cache");

    let cache = read_cache(codex_home).await.expect("read cache");
    assert_eq!(cache.version, SUMMARY_CACHE_VERSION);
    assert_eq!(cache.entries["thread-1"].summary, "A saved summary");

    let path = summary_cache_path(codex_home);
    tokio::fs::write(
        path,
        br#"{"version":99,"entries":{"thread-1":{"summary":"stale","last_summarized_turn_id":"turn-1","generated_at":7}}}"#,
    )
    .await
    .expect("write unknown cache version");
    assert!(read_cache(codex_home).await.unwrap().entries.is_empty());
}
