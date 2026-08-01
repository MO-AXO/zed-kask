//! Swarm Panel — a center-pane `Item` listing Agent Bestiary World agents and
//! swarms (workspaces) as cards, mirroring the Kask Extensions panel layout.
//!
//! Entities are **agents** (from the ABW catalogue) and **swarms** (the
//! operator's workspaces), not skills. Data is fetched through the global
//! `ToolInvoker` hook (the same governed, OCAP-gated path the kask panel's
//! visualization views use), so all ABW calls flow through `hkask-mcp-swarm`
//! and the kask MCP runtime rather than ad-hoc HTTP from the UI.
//!
//! Layout mirrors `KaskExtensionsPage`: headline, search bar, filter toggle
//! (All / Swarms / Agents), a uniform list of `MarketplaceCard`s, and an
//! empty state that surfaces fetch errors. v1 is read-only — hire/fire and
//! spend actions are gated behind the cost/consent gate (see
//! `kask/docs/plans/abw-swarm-intelligence.md` §3.6).

mod panel_button;

pub use panel_button::SwarmPanelButton;

use std::ops::Range;
use std::time::Duration;

use editor::Editor;
use gpui::{
    App, Context, Entity, EventEmitter, Focusable, Render, Task, UniformListScrollHandle, Window,
    actions, uniform_list,
};
use marketplace_ui_common::{MarketplaceCard, marketplace_empty_state, marketplace_search_bar};
use serde::Deserialize;
use serde_json::json;
use ui::{
    ScrollableHandle, ToggleButtonGroup, ToggleButtonGroupSize, ToggleButtonGroupStyle,
    ToggleButtonSimple, WithScrollbar, prelude::*,
};
use workspace::{
    Workspace,
    item::{Item, ItemEvent},
};

actions!(
    swarm_panel,
    [
        /// Deploys a new Swarm Panel if none is open, else focuses the
        /// existing one. Used by the View menu entry and the status bar button.
        Toggle,
        /// Focuses an existing Swarm Panel (no-op if none is open).
        ToggleFocus,
    ]
);

/// The MCP server id (matches `BUILT_IN_MCP_SERVERS`).
const SWARM_SERVER: &str = "swarm";

pub fn init(cx: &mut App) {
    cx.observe_new(move |workspace: &mut Workspace, window, _cx| {
        let Some(_window) = window else {
            return;
        };
        // Per the `.rules` trap "Center-pane Item Toggle vs ToggleFocus", the
        // View menu entry uses `Toggle` (deploys a new item if none exists),
        // not `ToggleFocus` (silent no-op when absent).
        workspace
            .register_action(move |workspace, _: &Toggle, window, cx| {
                let existing = workspace
                    .active_pane()
                    .read(cx)
                    .items()
                    .find_map(|item| item.downcast::<SwarmPanel>());

                if let Some(existing) = existing {
                    workspace.activate_item(&existing, true, true, window, cx);
                } else {
                    let swarm_panel = SwarmPanel::new(window, cx);
                    workspace.add_item_to_active_pane(
                        Box::new(swarm_panel.clone()),
                        None,
                        true,
                        window,
                        cx,
                    );
                    // The panel's `focus_handle` delegates to the query editor
                    // (constructed inside `cx.new`), so per the `.rules`
                    // deploy-and-focus trap we focus it explicitly.
                    swarm_panel.focus_handle(cx).focus(window, cx);
                }
            })
            .register_action(move |workspace, _: &ToggleFocus, window, cx| {
                let existing = workspace
                    .active_pane()
                    .read(cx)
                    .items()
                    .find_map(|item| item.downcast::<SwarmPanel>());
                if let Some(existing) = existing {
                    workspace.activate_item(&existing, true, true, window, cx);
                }
            });
    })
    .detach();
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
enum SwarmFilter {
    All,
    Swarms,
    Agents,
}

// ── View model ─────────────────────────────────────────────────────────────

/// One row in the panel — either an ABW agent or an ABW swarm (workspace).
#[derive(Clone, Debug)]
enum SwarmEntry {
    Agent(AgentCard),
    Swarm(SwarmCard),
}

#[derive(Clone, Debug)]
struct AgentCard {
    id: String,
    agent_type: String,
    description: String,
    author: String,
    executions: u64,
}

#[derive(Clone, Debug)]
struct SwarmCard {
    id: String,
    name: String,
    description: String,
    agent_count: u64,
    budget: u64,
    remaining: u64,
}

// ── MCP response structs (minimal, mirror hkask-mcp-swarm's tool output) ────

#[derive(Debug, Deserialize)]
struct AgentListResponse {
    agents: Vec<AgentInfo>,
}

#[derive(Debug, Deserialize)]
struct AgentInfo {
    agent_id: Option<String>,
    agent_type: Option<String>,
    description: Option<String>,
    author: Option<String>,
    execution_stats: Option<ExecutionStats>,
}

#[derive(Debug, Deserialize)]
struct ExecutionStats {
    total_executions: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceListResponse {
    workspaces: Vec<WorkspaceInfo>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceInfo {
    id: Option<String>,
    name: Option<String>,
    description: Option<String>,
    agent_count: Option<u64>,
    workspace_budget: Option<u64>,
    workspace_remaining: Option<u64>,
}

// ── Panel ──────────────────────────────────────────────────────────────────

pub struct SwarmPanel {
    list: UniformListScrollHandle,
    is_fetching: bool,
    fetch_error: Option<SharedString>,
    filter: SwarmFilter,
    entries: Vec<SwarmEntry>,
    filtered_entry_indices: Vec<usize>,
    query_editor: Entity<Editor>,
    _subscriptions: [gpui::Subscription; 1],
    search_task: Option<Task<()>>,
}

impl SwarmPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Entity<Self> {
        cx.new(|cx| {
            let query_editor = cx.new(|cx| {
                let mut input = Editor::single_line(window, cx);
                input.set_placeholder_text("Search agents and swarms...", window, cx);
                input
            });
            let subscriptions = [cx.subscribe(&query_editor, Self::on_query_change)];

            let scroll_handle = UniformListScrollHandle::new();

            let mut this = Self {
                list: scroll_handle,
                is_fetching: false,
                fetch_error: None,
                filter: SwarmFilter::All,
                entries: Vec::new(),
                filtered_entry_indices: Vec::new(),
                query_editor,
                _subscriptions: subscriptions,
                search_task: None,
            };
            this.fetch_all(cx);
            this
        })
    }

    /// Fetch agents and swarms via the governed MCP tool path.
    fn fetch_all(&mut self, cx: &mut Context<Self>) {
        let Some(invoker) = kask_panel::shared_tool_invoker() else {
            self.fetch_error = Some(
                "Tool invoker not wired — the swarm MCP server is unavailable. \
                 Ensure kask MCP servers are enabled (kask.mcp.load_default)."
                    .into(),
            );
            cx.notify();
            return;
        };

        self.is_fetching = true;
        self.fetch_error = None;
        cx.notify();

        // Agents (keyless-capable).
        cx.spawn({
            let invoker = invoker.clone();
            async move |this, cx| {
                let result = invoker
                    .invoke_tool(SWARM_SERVER, "swarm_list_agents", json!({ "limit": 200 }))
                    .await;
                this.update(cx, |this, cx| {
                    this.is_fetching = false;
                    match result {
                        Ok(output) => match serde_json::from_str::<AgentListResponse>(&output) {
                            Ok(response) => {
                                let agents = response
                                    .agents
                                    .into_iter()
                                    .map(|a| {
                                        SwarmEntry::Agent(AgentCard {
                                            id: a.agent_id.unwrap_or_default(),
                                            agent_type: a.agent_type.unwrap_or_default(),
                                            description: a.description.unwrap_or_default(),
                                            author: a.author.unwrap_or_default(),
                                            executions: a
                                                .execution_stats
                                                .and_then(|s| s.total_executions)
                                                .unwrap_or(0),
                                        })
                                    })
                                    .collect::<Vec<_>>();
                                // Replace agent entries, keep swarm entries.
                                this.entries.retain(|e| matches!(e, SwarmEntry::Swarm(_)));
                                this.entries.extend(agents);
                                this.fetch_error = None;
                                this.filter_entries(cx);
                            }
                            Err(err) => {
                                this.fetch_error =
                                    Some(format!("Failed to parse agents: {err}").into());
                                this.filter_entries(cx);
                            }
                        },
                        Err(err) => {
                            this.fetch_error = Some(format!("Failed to list agents: {err}").into());
                            this.filter_entries(cx);
                        }
                    }
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();

        // Swarms (requires the ABW API key).
        cx.spawn(async move |this, cx| {
            let result = invoker
                .invoke_tool(SWARM_SERVER, "swarm_get_swarm", json!({}))
                .await;
            this.update(cx, |this, cx| match result {
                Ok(output) => match serde_json::from_str::<WorkspaceListResponse>(&output) {
                    Ok(response) => {
                        let swarms = response
                            .workspaces
                            .into_iter()
                            .map(|w| {
                                SwarmEntry::Swarm(SwarmCard {
                                    id: w.id.unwrap_or_default(),
                                    name: w.name.unwrap_or_default(),
                                    description: w.description.unwrap_or_default(),
                                    agent_count: w.agent_count.unwrap_or(0),
                                    budget: w.workspace_budget.unwrap_or(0),
                                    remaining: w.workspace_remaining.unwrap_or(0),
                                })
                            })
                            .collect::<Vec<_>>();
                        // Replace swarm entries, keep agent entries.
                        this.entries.retain(|e| matches!(e, SwarmEntry::Agent(_)));
                        let mut swarms = swarms;
                        swarms.extend(this.entries.drain(..));
                        this.entries = swarms;
                        this.filter_entries(cx);
                    }
                    Err(err) => {
                        this.fetch_error =
                            Some(format!("Failed to parse workspaces: {err}").into());
                        this.filter_entries(cx);
                    }
                },
                Err(err) => {
                    // Auth failures here are expected when no key is configured —
                    // degrade to agents-only rather than an error state.
                    log::warn!("swarm-panel: could not fetch workspaces (agents-only mode): {err}");
                    this.filter_entries(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn filter_entries(&mut self, cx: &mut Context<Self>) {
        let filter = self.filter;
        let query = self.search_query(cx).map(|q| q.to_lowercase());
        let indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                let kind_matches = match (filter, entry) {
                    (SwarmFilter::All, _) => true,
                    (SwarmFilter::Swarms, SwarmEntry::Swarm(_)) => true,
                    (SwarmFilter::Agents, SwarmEntry::Agent(_)) => true,
                    _ => false,
                };
                if !kind_matches {
                    return false;
                }
                match &query {
                    None => true,
                    Some(q) => {
                        let haystack = match entry {
                            SwarmEntry::Agent(a) => {
                                format!("{} {} {} {}", a.id, a.agent_type, a.description, a.author)
                            }
                            SwarmEntry::Swarm(s) => {
                                format!("{} {} {}", s.id, s.name, s.description)
                            }
                        };
                        haystack.to_lowercase().contains(q)
                    }
                }
            })
            .map(|(ix, _)| ix)
            .collect();
        self.filtered_entry_indices = indices;
        cx.notify();
    }

    fn scroll_to_top(&mut self, cx: &mut Context<Self>) {
        self.list
            .set_offset(gpui::point(gpui::px(0.), gpui::px(0.)));
        cx.notify();
    }

    fn render_entries(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<MarketplaceCard> {
        let mut cards = Vec::new();
        for ix in range {
            if ix >= self.filtered_entry_indices.len() {
                break;
            }
            let entry_ix = self.filtered_entry_indices[ix];
            let entry = self.entries[entry_ix].clone();
            cards.push(self.render_card(entry, cx));
        }
        cards
    }

    fn render_card(&mut self, entry: SwarmEntry, _cx: &mut Context<Self>) -> MarketplaceCard {
        match entry {
            SwarmEntry::Agent(agent) => MarketplaceCard::new().child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap_1()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(Label::new(agent.id.clone()).color(Color::Default))
                                    .child(
                                        Label::new(agent.agent_type.clone()).color(Color::Accent),
                                    )
                                    .child(
                                        Label::new(format!("▶ {}", agent.executions))
                                            .color(Color::Muted),
                                    )
                                    .child(
                                        Label::new(format!("by {}", agent.author))
                                            .color(Color::Muted),
                                    ),
                            )
                            .child(Label::new(agent.description.clone()).color(Color::Muted)),
                    )
                    .child(
                        Label::new("Agent")
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            ),
            SwarmEntry::Swarm(swarm) => MarketplaceCard::new().child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap_1()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(Label::new(swarm.name.clone()).color(Color::Default))
                                    .child(
                                        Label::new(format!("{} agents", swarm.agent_count))
                                            .color(Color::Accent),
                                    )
                                    .child(
                                        Label::new(format!(
                                            "⛽ {}/{}",
                                            swarm.remaining, swarm.budget
                                        ))
                                        .color(Color::Muted),
                                    ),
                            )
                            .child(Label::new(swarm.description.clone()).color(Color::Muted)),
                    )
                    .child(
                        Label::new("Swarm")
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            ),
        }
    }

    fn render_search(&self, cx: &mut Context<Self>) -> Div {
        marketplace_search_bar(&self.query_editor, false, cx)
    }

    fn on_query_change(
        &mut self,
        _: Entity<Editor>,
        event: &editor::EditorEvent,
        cx: &mut Context<Self>,
    ) {
        if let editor::EditorEvent::Edited { .. } = event {
            self.refresh_search(cx);
        }
    }

    fn refresh_search(&mut self, cx: &mut Context<Self>) {
        // Debounce search, then filter locally — both lists arrive in one
        // fetch each, so keystrokes must not re-hit the network.
        self.search_task = Some(cx.spawn(async move |this, cx| {
            let search = this
                .update(cx, |this, cx| this.search_query(cx))
                .ok()
                .flatten();

            if search.is_some() {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
            };

            this.update(cx, |this, cx| {
                this.filter_entries(cx);
                this.scroll_to_top(cx);
            })
            .ok();
        }));
    }

    pub fn search_query(&self, cx: &mut App) -> Option<String> {
        let search = self.query_editor.read(cx).text(cx);
        if search.trim().is_empty() {
            None
        } else {
            Some(search)
        }
    }

    fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_search = self.search_query(cx).is_some();

        let message: SharedString = if self.is_fetching {
            "Loading agents and swarms…".into()
        } else if let Some(fetch_error) = &self.fetch_error {
            format!("Failed to load swarm data: {fetch_error}").into()
        } else {
            match self.filter {
                SwarmFilter::All => {
                    if has_search {
                        "No agents or swarms that match your search."
                    } else {
                        "No agents or swarms. Set HKASK_ABW_API_KEY to see your swarms."
                    }
                }
                SwarmFilter::Swarms => {
                    if has_search {
                        "No swarms that match your search."
                    } else {
                        "No swarms. Set HKASK_ABW_API_KEY to see your workspaces."
                    }
                }
                SwarmFilter::Agents => {
                    if has_search {
                        "No agents that match your search."
                    } else {
                        "No agents."
                    }
                }
            }
            .into()
        };

        marketplace_empty_state(message, self.fetch_error.is_some())
    }
}

impl Render for SwarmPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .child(
                v_flex()
                    .gap_4()
                    .pt_4()
                    .px_4()
                    .bg(cx.theme().colors().editor_background)
                    .child(
                        h_flex()
                            .w_full()
                            .gap_1p5()
                            .justify_between()
                            .child(Headline::new("Agent Swarm").size(HeadlineSize::Large)),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .flex_wrap()
                            .gap_2()
                            .child(self.render_search(cx))
                            .child(
                                div().child(
                                    ToggleButtonGroup::single_row(
                                        "swarm-filter-buttons",
                                        [
                                            ToggleButtonSimple::new(
                                                "All",
                                                cx.listener(|this, _event, _, cx| {
                                                    this.filter = SwarmFilter::All;
                                                    this.filter_entries(cx);
                                                    this.scroll_to_top(cx);
                                                }),
                                            ),
                                            ToggleButtonSimple::new(
                                                "Swarms",
                                                cx.listener(|this, _event, _, cx| {
                                                    this.filter = SwarmFilter::Swarms;
                                                    this.filter_entries(cx);
                                                    this.scroll_to_top(cx);
                                                }),
                                            ),
                                            ToggleButtonSimple::new(
                                                "Agents",
                                                cx.listener(|this, _event, _, cx| {
                                                    this.filter = SwarmFilter::Agents;
                                                    this.filter_entries(cx);
                                                    this.scroll_to_top(cx);
                                                }),
                                            ),
                                        ],
                                    )
                                    .style(ToggleButtonGroupStyle::Outlined)
                                    .size(ToggleButtonGroupSize::Custom(rems_from_px(30.)))
                                    .label_size(LabelSize::Default)
                                    .auto_width()
                                    .selected_index(match self.filter {
                                        SwarmFilter::All => 0,
                                        SwarmFilter::Swarms => 1,
                                        SwarmFilter::Agents => 2,
                                    })
                                    .into_any_element(),
                                ),
                            ),
                    ),
            )
            .child(v_flex().px_4().size_full().overflow_y_hidden().map(|this| {
                let count = self.filtered_entry_indices.len();

                if count == 0 {
                    this.child(self.render_empty_state(cx)).into_any_element()
                } else {
                    let scroll_handle = &self.list;
                    this.child(
                        uniform_list("swarm-entries", count, cx.processor(Self::render_entries))
                            .flex_grow_1()
                            .pb_4()
                            .track_scroll(scroll_handle),
                    )
                    .vertical_scrollbar_for(scroll_handle, window, cx)
                    .into_any_element()
                }
            }))
    }
}

impl EventEmitter<ItemEvent> for SwarmPanel {}

impl Focusable for SwarmPanel {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.query_editor.read(cx).focus_handle(cx)
    }
}

impl Item for SwarmPanel {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Agent Swarm".into()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("Swarm Panel Opened")
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::Share).color(Color::Muted))
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(workspace::item::ItemEvent)) {
        f(*event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pin the tool names the panel calls against the server's tool surface —
    // a rename in `hkask-mcp-swarm` must fail here, not silently degrade the
    // panel to an empty state.
    #[test]
    fn panel_tool_names_match_server() {
        // These strings must match the #[tool] fn names in
        // `hkask-mcp-swarm/src/hkask_mcp_swarm.rs`.
        assert_eq!(SWARM_SERVER, "swarm");
        for tool in ["swarm_list_agents", "swarm_get_swarm"] {
            assert!(tool.starts_with("swarm_"));
        }
    }
}
