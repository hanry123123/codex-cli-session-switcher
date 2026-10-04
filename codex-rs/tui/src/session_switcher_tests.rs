use super::*;
use pretty_assertions::assert_eq;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::fmt::Write;

fn thread(id: &str, name: &str, preview: &str) -> Thread {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "sessionId": id,
        "preview": preview,
        "ephemeral": false,
        "modelProvider": "openai",
        "model": "gpt-6-luna",
        "createdAt": 1,
        "updatedAt": 2,
        "recencyAt": 2,
        "status": { "type": "idle" },
        "path": null,
        "cwd": "/tmp/project",
        "cliVersion": "0.160.0",
        "source": "cli",
        "name": name,
        "turns": []
    }))
    .expect("valid test thread")
}

fn render_to_string(area: Rect, buffer: &Buffer) -> String {
    let mut output = String::new();
    for y in area.y..area.bottom() {
        if y > area.y {
            output.push('\n');
        }
        for x in area.x..area.right() {
            let _ = write!(output, "{}", buffer[(x, y)].symbol());
        }
    }
    output
}

#[test]
fn session_switcher_renders_summary_above_secondary_request() {
    let first_id = "0199c0a0-0000-7000-8000-000000000001";
    let second_id = "0199c0a0-0000-7000-8000-000000000002";
    let state = SessionSwitcherState {
        items: vec![
            SessionSwitcherItem {
                thread: thread(first_id, "Sidebar summary", "First prompt preview"),
                archived: false,
                summary: Some("Summarizes work on the sidebar".to_string()),
                last_user_request: Some("Please check the latest layout".to_string()),
            },
            SessionSwitcherItem {
                thread: thread(second_id, "Archived conversation", "Older prompt"),
                archived: true,
                summary: Some("A completed archived task".to_string()),
                last_user_request: Some("Review the final patch".to_string()),
            },
        ],
        selected: 1,
        focused: true,
        ..Default::default()
    };
    let area = Rect::new(0, 0, 36, 13);
    let mut buffer = Buffer::empty(area);
    state.render(area, &mut buffer, ThreadId::try_from(first_id).ok());
    insta::assert_snapshot!(render_to_string(area, &buffer));
}

#[test]
fn session_switcher_selection_stays_in_bounds() {
    let mut state = SessionSwitcherState::default();
    state.select_next(1);
    assert_eq!(state.selected, 0);

    state.items = vec![
        SessionSwitcherItem {
            thread: thread("0199c0a0-0000-7000-8000-000000000001", "One", "One"),
            archived: false,
            summary: None,
            last_user_request: None,
        },
        SessionSwitcherItem {
            thread: thread("0199c0a0-0000-7000-8000-000000000002", "Two", "Two"),
            archived: false,
            summary: None,
            last_user_request: None,
        },
    ];
    state.select_next(1);
    state.select_next(1);
    assert_eq!(state.selected, 1);
    state.select_next(-1);
    assert_eq!(state.selected, 0);
}

#[test]
fn session_switcher_places_new_session_before_thread_rows() {
    let mut state = SessionSwitcherState {
        items: vec![
            SessionSwitcherItem {
                thread: thread("0199c0a0-0000-7000-8000-000000000001", "One", "One"),
                archived: false,
                summary: None,
                last_user_request: None,
            },
            SessionSwitcherItem {
                thread: thread("0199c0a0-0000-7000-8000-000000000002", "Two", "Two"),
                archived: false,
                summary: None,
                last_user_request: None,
            },
        ],
        focused: true,
        ..Default::default()
    };

    state.select_next(-1);
    assert!(state.new_session_selected);
    assert_eq!(state.selected_thread_id(), None);
    state.select_next(1);
    assert!(!state.new_session_selected);
    assert_eq!(
        state.selected_thread_id().map(|id| id.to_string()),
        Some("0199c0a0-0000-7000-8000-000000000001".to_string())
    );
}

#[test]
fn session_switcher_scroll_hit_test_uses_sidebar_columns_without_click_state() {
    let sidebar_width = SessionSwitcherState::sidebar_width(120);
    assert!(SessionSwitcherState::contains_column(
        120,
        sidebar_width - 1
    ));
    assert!(!SessionSwitcherState::contains_column(120, sidebar_width));
}

#[test]
fn session_switcher_keeps_only_two_in_memory_previews() {
    let first = ThreadId::try_from("0199c0a0-0000-7000-8000-000000000001").unwrap();
    let second = ThreadId::try_from("0199c0a0-0000-7000-8000-000000000002").unwrap();
    let third = ThreadId::try_from("0199c0a0-0000-7000-8000-000000000003").unwrap();
    let mut state = SessionSwitcherState::default();
    state.cache_thread_preview(first, &[]);
    state.cache_thread_preview(second, &[]);
    state.cache_thread_preview(third, &[]);

    assert!(state.cached_thread_preview(first).is_none());
    assert!(matches!(
        state.cached_thread_preview(second),
        Some(cells) if cells.is_empty()
    ));
    assert!(matches!(
        state.cached_thread_preview(third),
        Some(cells) if cells.is_empty()
    ));
}

#[test]
fn session_switcher_mru_toggles_between_the_last_two_threads() {
    let first = ThreadId::try_from("0199c0a0-0000-7000-8000-000000000001").unwrap();
    let second = ThreadId::try_from("0199c0a0-0000-7000-8000-000000000002").unwrap();
    let mut state = SessionSwitcherState::default();
    state.note_thread_switch(None, first);
    state.note_thread_switch(Some(first), second);
    assert_eq!(state.previous_thread(Some(second)), Some(first));
    state.note_thread_switch(Some(second), first);
    assert_eq!(state.previous_thread(Some(first)), Some(second));
}

#[test]
fn session_switcher_summary_updates_are_deduplicated_until_taken() {
    let first = ThreadId::try_from("0199c0a0-0000-7000-8000-000000000001").unwrap();
    let second = ThreadId::try_from("0199c0a0-0000-7000-8000-000000000002").unwrap();
    let mut state = SessionSwitcherState::default();
    state.queue_summary_update(first);
    state.queue_summary_update(first);
    state.queue_summary_update(second);

    assert_eq!(state.take_pending_summary_updates(), [first, second]);
    assert!(state.take_pending_summary_updates().is_empty());
}
