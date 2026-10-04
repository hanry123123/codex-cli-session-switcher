//! Session-list loading and keyboard routing for the persistent sidebar.

use super::session_switcher_summaries::latest_user_request;
use super::session_switcher_summaries::load_cached_summaries;
use super::session_switcher_summaries::start_summary_generation;
use super::*;
use crate::session_switcher::SessionSwitcherItem;
use codex_app_server_client::AppServerRequestHandle;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::SortDirection;
use codex_app_server_protocol::ThreadItemsListCursor;
use codex_app_server_protocol::ThreadItemsListParams;
use codex_app_server_protocol::ThreadItemsListResponse;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadSortKey;
use codex_app_server_protocol::ThreadSourceKind;
use codex_protocol::ThreadId;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use crossterm::event::MouseEventKind;
use futures::StreamExt;

const THREAD_LIST_PAGE_SIZE: u32 = 100;

impl App {
    pub(super) fn refresh_session_switcher(&mut self, app_server: &mut AppServerSession) {
        if self.session_switcher.loading {
            return;
        }
        self.session_switcher.loading = true;
        self.session_switcher.error = None;
        let codex_home = self.config.codex_home.to_path_buf();
        let request_handle = app_server.request_handle();
        let active_request_id = app_server.next_request_id();
        let archived_request_id = app_server.next_request_id();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let (cached_summaries, active, archived) = tokio::join!(
                load_cached_summaries(codex_home.as_path()),
                load_threads(
                    request_handle.clone(),
                    SessionThreadList::Active,
                    active_request_id,
                ),
                load_threads(
                    request_handle,
                    SessionThreadList::Archived,
                    archived_request_id,
                ),
            );
            let result = active
                .and_then(|active| archived.map(|archived| (active, archived)))
                .map(|(active, archived)| {
                    let mut items = active
                        .into_iter()
                        .map(|thread| SessionSwitcherItem {
                            summary: cached_summaries.get(&thread.id).cloned(),
                            last_user_request: None,
                            thread,
                            archived: false,
                        })
                        .chain(archived.into_iter().map(|thread| SessionSwitcherItem {
                            summary: cached_summaries.get(&thread.id).cloned(),
                            last_user_request: None,
                            thread,
                            archived: true,
                        }))
                        .collect::<Vec<_>>();
                    items.sort_by_key(|item| {
                        std::cmp::Reverse(item.thread.recency_at.unwrap_or(item.thread.updated_at))
                    });
                    items
                })
                .map_err(|err| err.to_string());
            app_event_tx.send(AppEvent::SessionSwitcherThreadsLoaded { result });
        });
    }

    pub(super) fn start_pending_session_summaries(&mut self, app_server: &AppServerSession) {
        if !crate::session_switcher::is_session_switcher_enabled()
            || !self.chat_widget.has_chatgpt_account()
            || self.session_switcher.summaries_loading
        {
            return;
        }
        let items: Vec<_> = self
            .session_switcher
            .take_pending_summary_updates()
            .into_iter()
            .filter_map(|thread_id| {
                self.session_switcher
                    .items
                    .iter()
                    .find(|item| item.thread.id == thread_id.to_string())
                    .cloned()
            })
            .collect();
        if items.is_empty() {
            return;
        }
        self.session_switcher.summaries_loading = true;
        start_summary_generation(
            self.config.codex_home.to_path_buf(),
            app_server.request_handle(),
            self.app_event_tx.clone(),
            items,
        );
    }

    pub(super) fn start_session_summaries(&mut self, app_server: &AppServerSession) {
        if !crate::session_switcher::is_session_switcher_enabled()
            || !self.chat_widget.has_chatgpt_account()
            || self.session_switcher.summaries_loading
        {
            return;
        }
        self.session_switcher.summaries_loading = true;
        start_summary_generation(
            self.config.codex_home.to_path_buf(),
            app_server.request_handle(),
            self.app_event_tx.clone(),
            self.session_switcher.items.clone(),
        );
    }

    pub(super) fn load_selected_last_request(&mut self, app_server: &mut AppServerSession) {
        let Some(item) = self.session_switcher.selected_item() else {
            return;
        };
        if item.last_user_request.is_some() {
            return;
        }
        let Ok(thread_id) = ThreadId::try_from(item.thread.id.as_str()) else {
            return;
        };
        if self.session_switcher.last_request_was_loaded(thread_id) {
            return;
        }
        self.start_last_request_load(app_server, thread_id);
    }

    pub(super) fn start_last_request_load(
        &mut self,
        app_server: &mut AppServerSession,
        thread_id: ThreadId,
    ) {
        if !self.session_switcher.begin_last_request_load(thread_id) {
            return;
        }
        let request_id = app_server.next_request_id();
        start_last_request_fetch(
            app_server.request_handle(),
            self.app_event_tx.clone(),
            vec![(thread_id, request_id)],
        );
    }

    pub(super) fn handle_session_switcher_mouse(
        &mut self,
        tui: &mut tui::Tui,
        app_server: &mut AppServerSession,
        event: &TuiEvent,
    ) -> bool {
        if !crate::session_switcher::is_session_switcher_enabled()
            || !tui.is_owned_screen()
            || self.overlay.is_some()
            || !self.chat_widget.no_modal_or_popup_active()
        {
            return false;
        }
        let TuiEvent::Mouse(mouse) = event else {
            return false;
        };
        if !matches!(
            mouse.kind,
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        ) {
            return false;
        }
        if !crate::session_switcher::SessionSwitcherState::contains_column(
            tui.terminal.last_known_screen_size.width,
            mouse.column,
        ) {
            return false;
        }
        self.session_switcher.focused = true;
        match mouse.kind {
            MouseEventKind::ScrollUp => self.session_switcher.select_next(-1),
            MouseEventKind::ScrollDown => self.session_switcher.select_next(1),
            _ => return false,
        }
        if self.session_switcher.items.is_empty() {
            self.refresh_session_switcher(app_server);
        } else {
            self.load_selected_last_request(app_server);
        }
        tui.frame_requester().schedule_frame();
        true
    }

    pub(super) async fn handle_session_switcher_key(
        &mut self,
        tui: &mut tui::Tui,
        app_server: &mut AppServerSession,
        key: KeyEvent,
    ) -> bool {
        if !crate::session_switcher::is_session_switcher_enabled()
            || key.kind != KeyEventKind::Press
            || self.overlay.is_some()
            || !self.chat_widget.no_modal_or_popup_active()
            || self.chat_widget.has_active_view()
            || (tui.is_owned_screen() && self.transcript_view.has_active_interaction())
        {
            return false;
        }

        if key.code == KeyCode::F(6) && key.modifiers == KeyModifiers::NONE {
            let entering_sidebar = !self.session_switcher.focused;
            self.session_switcher.focused = entering_sidebar;
            if entering_sidebar {
                self.refresh_session_switcher(app_server);
                self.load_selected_last_request(app_server);
            }
            self.session_switcher.activate_on_main_enter = !entering_sidebar
                && self
                    .session_switcher
                    .selected_thread_id()
                    .is_some_and(|thread_id| Some(thread_id) != self.current_displayed_thread_id());
            tui.frame_requester().schedule_frame();
            return true;
        }

        if key.code == KeyCode::F(7) && key.modifiers == KeyModifiers::NONE {
            self.session_switcher.activate_on_main_enter = false;
            if let Some(thread_id) = self
                .session_switcher
                .previous_thread(self.current_displayed_thread_id())
                && self
                    .open_session_switcher_thread(tui, app_server, thread_id)
                    .await
            {
                return true;
            }
            tui.frame_requester().schedule_frame();
            return true;
        }

        if self.session_switcher.focused {
            match (key.code, key.modifiers) {
                (KeyCode::Up, KeyModifiers::NONE) => {
                    self.session_switcher.select_next(-1);
                    self.load_selected_last_request(app_server);
                }
                (KeyCode::Down, KeyModifiers::NONE) => {
                    self.session_switcher.select_next(1);
                    self.load_selected_last_request(app_server);
                }
                (KeyCode::Enter, KeyModifiers::NONE) => {
                    if self.session_switcher.new_session_selected {
                        self.session_switcher.focused = false;
                        self.session_switcher.new_session_selected = false;
                        self.app_event_tx.send(AppEvent::NewSession { name: None });
                    } else if let Some(thread_id) = self.session_switcher.selected_thread_id() {
                        self.open_session_switcher_thread(tui, app_server, thread_id)
                            .await;
                    }
                }
                (KeyCode::Esc, KeyModifiers::NONE) => {
                    self.session_switcher.focused = false;
                    self.session_switcher.activate_on_main_enter = false;
                }
                _ => {}
            }
            tui.frame_requester().schedule_frame();
            return true;
        }

        if self.session_switcher.activate_on_main_enter {
            self.session_switcher.activate_on_main_enter = false;
            if key.code == KeyCode::Enter
                && key.modifiers == KeyModifiers::NONE
                && self.chat_widget.composer_is_empty()
                && let Some(thread_id) = self.session_switcher.selected_thread_id()
            {
                self.open_session_switcher_thread(tui, app_server, thread_id)
                    .await;
                return true;
            }
        }
        false
    }

    async fn open_session_switcher_thread(
        &mut self,
        tui: &mut tui::Tui,
        app_server: &mut AppServerSession,
        thread_id: ThreadId,
    ) -> bool {
        let started_at = Instant::now();
        if let Some(index) = self
            .session_switcher
            .items
            .iter()
            .position(|item| item.thread.id == thread_id.to_string())
        {
            self.session_switcher.selected = index;
            self.session_switcher.new_session_selected = false;
        }
        self.session_switcher.opening_thread_id = Some(thread_id);
        if tui.is_owned_screen() {
            match tui.prepare_draw_size() {
                Ok(size) => {
                    if let Err(err) = self.render_owned_transcript(tui, size) {
                        tracing::warn!(error = %err, "failed to draw session switch preview");
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "failed to prepare session switch preview")
                }
            }
        }
        if let Some(item) = self
            .session_switcher
            .items
            .iter_mut()
            .find(|item| item.thread.id == thread_id.to_string())
            && item.archived
        {
            match app_server.thread_unarchive(thread_id).await {
                Ok(thread) => {
                    item.thread = thread;
                    item.archived = false;
                }
                Err(err) => {
                    self.session_switcher.opening_thread_id = None;
                    self.session_switcher.error =
                        Some(format!("Could not unarchive session: {err}"));
                    tui.frame_requester().schedule_frame();
                    return true;
                }
            }
        }

        let target = self
            .session_switcher
            .items
            .iter()
            .find(|item| item.thread.id == thread_id.to_string())
            .map(|item| SessionTarget {
                path: item.thread.path.clone(),
                thread_id,
                cwd: Some(item.thread.cwd.to_path_buf()),
                history_mode: Some(item.thread.history_mode),
            })
            .unwrap_or(SessionTarget {
                path: None,
                thread_id,
                cwd: None,
                history_mode: None,
            });
        match self
            .apply_resume_picker_selection_from_switcher(
                tui,
                app_server,
                SessionSelection::Resume(target),
            )
            .await
        {
            Ok(AppRunControl::Continue) => {
                self.session_switcher.opening_thread_id = None;
                self.session_switcher.focused = false;
                self.session_switcher.activate_on_main_enter = false;
                tracing::debug!(
                    elapsed_ms = started_at.elapsed().as_millis(),
                    "session switch completed"
                );
                true
            }
            Ok(AppRunControl::Exit(_)) => {
                self.session_switcher.opening_thread_id = None;
                self.app_event_tx.send(AppEvent::Exit(ExitMode::Immediate));
                true
            }
            Err(err) => {
                self.session_switcher.opening_thread_id = None;
                self.session_switcher.error = Some(err.to_string());
                tracing::debug!(
                    elapsed_ms = started_at.elapsed().as_millis(),
                    "session switch failed"
                );
                true
            }
        }
    }
}

fn start_last_request_fetch(
    request_handle: AppServerRequestHandle,
    app_event_tx: AppEventSender,
    requests: Vec<(ThreadId, RequestId)>,
) {
    tokio::spawn(async move {
        let mut requests = futures::stream::iter(requests)
            .map(|(thread_id, request_id)| {
                let request_handle = request_handle.clone();
                async move {
                    let mut cursor = None;
                    let mut request = None;
                    let mut loaded = false;
                    for page_index in 0..8 {
                        let page_request_id = if page_index == 0 {
                            request_id.clone()
                        } else {
                            RequestId::String(format!(
                                "session-switcher-last-request-{thread_id}-{request_id}-{page_index}"
                            ))
                        };
                        let result = request_handle
                            .request_typed::<ThreadItemsListResponse>(
                                ClientRequest::ThreadItemsList {
                                    request_id: page_request_id,
                                    params: ThreadItemsListParams {
                                        thread_id: thread_id.to_string(),
                                        turn_id: None,
                                        cursor: cursor.clone(),
                                        limit: Some(64),
                                        sort_direction: Some(SortDirection::Desc),
                                    },
                                },
                            )
                            .await;
                        match result {
                            Ok(page) => {
                                loaded = true;
                                if let Some(latest_request) = latest_user_request(page.data) {
                                    request = latest_request;
                                    break;
                                }
                                let Some(next_cursor) = page.next_cursor else {
                                    break;
                                };
                                let next_cursor = ThreadItemsListCursor::Opaque(next_cursor);
                                if cursor.as_ref() == Some(&next_cursor) {
                                    break;
                                }
                                cursor = Some(next_cursor);
                            }
                            Err(err) => {
                                tracing::debug!(%thread_id, error = %err, "failed to load latest session request");
                                loaded = false;
                                break;
                            }
                        }
                    }
                    (thread_id, request, loaded)
                }
            })
            .buffer_unordered(8);
        while let Some((thread_id, request, loaded)) = requests.next().await {
            app_event_tx.send(AppEvent::SessionSwitcherLastRequestUpdated {
                thread_id,
                request,
                loaded,
            });
        }
    });
}

#[derive(Clone, Copy)]
enum SessionThreadList {
    Active,
    Archived,
}

async fn load_threads(
    request_handle: AppServerRequestHandle,
    scope: SessionThreadList,
    first_request_id: RequestId,
) -> color_eyre::eyre::Result<Vec<codex_app_server_protocol::Thread>> {
    let mut cursor = None;
    let mut threads = Vec::new();
    let mut page_index = 0;
    loop {
        let request_id = if page_index == 0 {
            first_request_id.clone()
        } else {
            RequestId::String(format!(
                "session-switcher-thread-list-{}-{page_index}-{first_request_id}",
                matches!(scope, SessionThreadList::Archived)
            ))
        };
        let page: ThreadListResponse = request_handle
            .request_typed(ClientRequest::ThreadList {
                request_id,
                params: ThreadListParams {
                    cursor: cursor.clone(),
                    limit: Some(THREAD_LIST_PAGE_SIZE),
                    sort_key: Some(ThreadSortKey::RecencyAt),
                    sort_direction: Some(SortDirection::Desc),
                    model_providers: None,
                    source_kinds: Some(all_thread_sources()),
                    originators: None,
                    archived: Some(matches!(scope, SessionThreadList::Archived)),
                    section_id: None,
                    project_id: None,
                    cwd: None,
                    use_state_db_only: true,
                    search_term: None,
                    parent_thread_id: None,
                    ancestor_thread_id: None,
                },
            })
            .await?;
        threads.extend(page.data);
        let Some(next_cursor) = page.next_cursor else {
            break;
        };
        if cursor.as_deref() == Some(next_cursor.as_str()) {
            break;
        }
        cursor = Some(next_cursor);
        page_index += 1;
    }
    Ok(threads)
}

fn all_thread_sources() -> Vec<ThreadSourceKind> {
    vec![
        ThreadSourceKind::Cli,
        ThreadSourceKind::VsCode,
        ThreadSourceKind::Exec,
        ThreadSourceKind::AppServer,
        ThreadSourceKind::SubAgent,
        ThreadSourceKind::SubAgentReview,
        ThreadSourceKind::SubAgentCompact,
        ThreadSourceKind::SubAgentThreadSpawn,
        ThreadSourceKind::SubAgentOther,
        ThreadSourceKind::Unknown,
    ]
}
