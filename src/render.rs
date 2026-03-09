//! GPU rendering module for vault browser UI.
//!
//! Uses madori (app framework) + garasu (GPU primitives) + egaku (widgets).
//!
//! ## Layout
//!
//! ```text
//! +------------------------------------------+
//! |  Search bar (TextInput)                  |
//! +-------------+----------------------------+
//! | Vault list  |  Item list / Item detail   |
//! | (ListView)  |  (ListView -> detail view) |
//! |             |                            |
//! |             |  Fields:                   |
//! |             |    username: user@...       |
//! |             |    password: ******** [copy]|
//! |             |    url: https://...         |
//! +-------------+----------------------------+
//! ```
//!
//! ## Rendering flow
//!
//! 1. madori handles window + event loop + frame timing
//! 2. Our `RenderCallback` implementation renders:
//!    - Background (Nord polar night)
//!    - Vault sidebar via egaku `ListView`
//!    - Item list or detail view
//!    - Search overlay when active
//!    - Text via garasu `TextRenderer`
//! 3. Input events dispatched to focused widget via egaku `FocusManager`

use crate::config::AppearanceConfig;
use crate::vault::{Item, ItemSummary, Vault};
use egaku::{FocusManager, ListView, TextInput};
use garasu::GpuContext;

/// Current view mode for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewMode {
    /// Showing the vault list (top-level).
    VaultList,
    /// Showing items in a selected vault.
    ItemList,
    /// Showing detail for a single item.
    ItemDetail,
    /// Search overlay is active.
    Search,
}

/// Application state for the vault browser UI.
pub struct KagiState {
    /// Current view mode.
    pub mode: ViewMode,
    /// Previous mode (for back navigation).
    pub prev_mode: Option<ViewMode>,
    /// Loaded vaults.
    pub vaults: Vec<Vault>,
    /// Currently selected vault index.
    pub selected_vault: usize,
    /// Items in the currently selected vault.
    pub items: Vec<Item>,
    /// Summaries for list display.
    pub summaries: Vec<ItemSummary>,
    /// All items across all vaults (for global search).
    pub all_items: Vec<Item>,
    /// All summaries across all vaults.
    pub all_summaries: Vec<ItemSummary>,
    /// Vault list widget.
    pub vault_list: ListView,
    /// Item list widget.
    pub item_list: ListView,
    /// Focus manager for widget focus (reserved for future multi-pane layout).
    #[allow(dead_code)]
    pub focus: FocusManager,
    /// Search input widget.
    pub search_input: TextInput,
    /// Search results (filtered summaries).
    pub search_results: Vec<ItemSummary>,
    /// Whether to show only favorites.
    pub favorites_only: bool,
    /// Detail view: currently viewed item.
    pub detail_item: Option<Item>,
    /// Detail view: selected field index.
    pub detail_field_index: usize,
    /// Detail view: whether hidden fields are revealed.
    pub show_hidden: bool,
    /// Status message (e.g. "Copied password").
    pub status_message: Option<String>,
    /// Whether data is currently loading.
    pub loading: bool,
    /// Width of the window.
    pub width: u32,
    /// Height of the window.
    pub height: u32,
}

impl KagiState {
    /// Create a new empty state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mode: ViewMode::VaultList,
            prev_mode: None,
            vaults: Vec::new(),
            selected_vault: 0,
            items: Vec::new(),
            summaries: Vec::new(),
            all_items: Vec::new(),
            all_summaries: Vec::new(),
            vault_list: ListView::new(Vec::new(), 20),
            item_list: ListView::new(Vec::new(), 20),
            focus: FocusManager::new(vec![
                "vault_list".into(),
                "item_list".into(),
            ]),
            search_input: TextInput::new(),
            search_results: Vec::new(),
            favorites_only: false,
            detail_item: None,
            detail_field_index: 0,
            show_hidden: false,
            status_message: None,
            loading: false,
            width: 1280,
            height: 720,
        }
    }

    /// Set the vault list and update the widget.
    pub fn set_vaults(&mut self, vaults: Vec<Vault>) {
        let names: Vec<String> = vaults
            .iter()
            .map(|v| format!("{} ({})", v.name, v.items))
            .collect();
        self.vault_list.set_items(names);
        self.vaults = vaults;
        self.selected_vault = 0;
    }

    /// Set the items for the currently selected vault.
    pub fn set_items(&mut self, items: Vec<Item>) {
        let vault_name = self
            .vaults
            .get(self.selected_vault)
            .map(|v| v.name.as_str())
            .unwrap_or("");
        self.summaries = items
            .iter()
            .map(|i| ItemSummary::from_item_with_vault(i, vault_name))
            .collect();
        let display = format_summaries(&self.summaries, self.favorites_only);
        self.item_list.set_items(display);
        self.items = items;
    }

    /// Set all items from all vaults (for global search).
    pub fn set_all_items(&mut self, items: Vec<Item>, summaries: Vec<ItemSummary>) {
        self.all_items = items;
        self.all_summaries = summaries;
    }

    /// Navigate down in the current list.
    pub fn move_down(&mut self) {
        match self.mode {
            ViewMode::VaultList => self.vault_list.select_next(),
            ViewMode::ItemList | ViewMode::Search => self.item_list.select_next(),
            ViewMode::ItemDetail => {
                if let Some(ref item) = self.detail_item {
                    if self.detail_field_index + 1 < item.fields.len() {
                        self.detail_field_index += 1;
                    }
                }
            }
        }
    }

    /// Navigate up in the current list.
    pub fn move_up(&mut self) {
        match self.mode {
            ViewMode::VaultList => self.vault_list.select_prev(),
            ViewMode::ItemList | ViewMode::Search => self.item_list.select_prev(),
            ViewMode::ItemDetail => {
                if self.detail_field_index > 0 {
                    self.detail_field_index -= 1;
                }
            }
        }
    }

    /// Enter item detail view for the currently selected item.
    #[allow(dead_code)]
    pub fn enter_detail(&mut self) {
        let idx = self.item_list.selected_index();
        let item = if self.mode == ViewMode::Search {
            // Map search result back to full item
            self.search_results.get(idx).and_then(|s| {
                self.all_items.iter().find(|i| i.id == s.id).cloned()
            })
        } else {
            self.items.get(idx).cloned()
        };

        if let Some(item) = item {
            self.detail_item = Some(item);
            self.detail_field_index = 0;
            self.show_hidden = false;
            self.prev_mode = Some(self.mode.clone());
            self.mode = ViewMode::ItemDetail;
        }
    }

    /// Go back from detail or search mode.
    pub fn go_back(&mut self) {
        match self.mode {
            ViewMode::ItemDetail => {
                self.detail_item = None;
                self.mode = self.prev_mode.take().unwrap_or(ViewMode::ItemList);
            }
            ViewMode::Search => {
                self.mode = ViewMode::ItemList;
                self.search_input = TextInput::new();
                // Restore item list display
                let display = format_summaries(&self.summaries, self.favorites_only);
                self.item_list.set_items(display);
            }
            ViewMode::ItemList => {
                self.mode = ViewMode::VaultList;
                self.items.clear();
                self.summaries.clear();
                self.item_list.set_items(Vec::new());
            }
            ViewMode::VaultList => {
                // No further back from vault list
            }
        }
    }

    /// Enter search mode.
    pub fn enter_search(&mut self) {
        self.prev_mode = Some(self.mode.clone());
        self.mode = ViewMode::Search;
        self.search_input = TextInput::new();
        // Start with all items
        self.search_results = self.all_summaries.clone();
        let display = format_summaries(&self.search_results, false);
        self.item_list.set_items(display);
    }

    /// Filter items by the current search query using fuzzy scoring.
    pub fn apply_search(&mut self) {
        let query = self.search_input.text().to_lowercase();
        if query.is_empty() {
            self.search_results = self.all_summaries.clone();
        } else {
            // Score all items and sort by score descending
            let mut scored: Vec<(u32, &ItemSummary)> = self
                .all_items
                .iter()
                .zip(self.all_summaries.iter())
                .map(|(item, summary)| (item.fuzzy_score(&query), summary))
                .filter(|(score, _)| *score > 0)
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            self.search_results = scored.into_iter().map(|(_, s)| s.clone()).collect();
        }
        let display = format_summaries(&self.search_results, false);
        self.item_list.set_items(display);
    }

    /// Get the currently selected item (by list index).
    #[must_use]
    pub fn selected_item(&self) -> Option<&Item> {
        let idx = self.item_list.selected_index();
        if self.mode == ViewMode::Search {
            self.search_results.get(idx).and_then(|s| {
                self.all_items.iter().find(|i| i.id == s.id)
            })
        } else {
            self.items.get(idx)
        }
    }

    /// Get the currently selected item's vault_id and item_id.
    #[must_use]
    pub fn selected_ids(&self) -> Option<(String, String)> {
        self.selected_item().map(|i| (i.vault_id.clone(), i.id.clone()))
    }

    /// Get the currently selected detail field.
    #[must_use]
    pub fn selected_field_value(&self) -> Option<&str> {
        self.detail_item.as_ref().and_then(|item| {
            item.fields.get(self.detail_field_index).map(|f| f.value.as_str())
        })
    }

    /// Set a temporary status message.
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
    }

    /// Clear status message.
    #[allow(dead_code)]
    pub fn clear_status(&mut self) {
        self.status_message = None;
    }
}

impl Default for KagiState {
    fn default() -> Self {
        Self::new()
    }
}

/// Format summaries into display strings for the list view.
fn format_summaries(summaries: &[ItemSummary], favorites_only: bool) -> Vec<String> {
    summaries
        .iter()
        .filter(|s| !favorites_only || s.favorite)
        .map(|s| {
            let star = if s.favorite { "*" } else { " " };
            let totp = if s.has_totp { " [TOTP]" } else { "" };
            let user = s.username.as_deref().unwrap_or("");
            let vault = if s.vault_name.is_empty() {
                String::new()
            } else {
                format!(" ({})", s.vault_name)
            };
            format!("{star} {}{vault} [{}{totp}] {user}", s.title, s.category)
        })
        .collect()
}

/// Collect lines to render based on current view mode.
/// Returns a list of (text, is_selected, is_accent).
fn collect_lines(state: &KagiState) -> Vec<(String, bool, bool)> {
    let mut lines: Vec<(String, bool, bool)> = Vec::new();

    if state.loading {
        lines.push(("Loading...".into(), false, true));
        return lines;
    }

    match &state.mode {
        ViewMode::VaultList => {
            lines.push(("Kagi \u{2014} Select Vault".into(), false, true));
            lines.push((String::new(), false, false));

            for (i, item) in state.vault_list.visible_items().iter().enumerate() {
                let real_idx = state.vault_list.offset() + i;
                let selected = real_idx == state.vault_list.selected_index();
                let prefix = if selected { "> " } else { "  " };
                lines.push((format!("{prefix}{item}"), selected, false));
            }

            if state.vaults.is_empty() {
                lines.push(("  (no vaults found)".into(), false, true));
                lines.push(("  Configure `op` CLI or Connect API in ~/.config/kagi/kagi.yaml".into(), false, true));
            }

            lines.push((String::new(), false, false));
            lines.push((
                "[j/k] navigate  [Enter] open  [/] search  [q] quit".into(),
                false,
                true,
            ));
        }
        ViewMode::ItemList | ViewMode::Search => {
            let vault_name = state
                .vaults
                .get(state.selected_vault)
                .map(|v| v.name.as_str())
                .unwrap_or("Items");
            let title = if state.mode == ViewMode::Search {
                format!("Search: {}", state.search_input.text())
            } else {
                format!("Kagi \u{2014} {vault_name}")
            };
            lines.push((title, false, true));
            lines.push((String::new(), false, false));

            for (i, item) in state.item_list.visible_items().iter().enumerate() {
                let real_idx = state.item_list.offset() + i;
                let selected = real_idx == state.item_list.selected_index();
                let prefix = if selected { "> " } else { "  " };
                lines.push((format!("{prefix}{item}"), selected, false));
            }

            if state.item_list.is_empty() {
                let msg = if state.mode == ViewMode::Search {
                    "  (no results)"
                } else {
                    "  (no items)"
                };
                lines.push((msg.into(), false, true));
            }

            lines.push((String::new(), false, false));
            let help = if state.mode == ViewMode::Search {
                "[type] search  [j/k] navigate  [Enter] detail  [Esc] cancel"
            } else {
                "[j/k] navigate  [Enter] detail  [p] password  [u] username  [t] TOTP  [/] search  [f] favorites  [Tab] vault  [Esc] back  [q] quit"
            };
            lines.push((help.into(), false, true));
        }
        ViewMode::ItemDetail => {
            if let Some(ref item) = state.detail_item {
                lines.push((format!("Kagi \u{2014} {}", item.title), false, true));
                lines.push((format!("  Category: {}", item.category), false, true));

                if !item.tags.is_empty() {
                    lines.push((format!("  Tags: {}", item.tags.join(", ")), false, true));
                }

                if let Some(url) = item.primary_url() {
                    lines.push((format!("  URL: {url}"), false, true));
                }

                lines.push((String::new(), false, false));
                lines.push(("  Fields:".into(), false, true));

                for (i, field) in item.fields.iter().enumerate() {
                    if field.label.is_empty() && field.value.is_empty() {
                        continue;
                    }
                    let selected = i == state.detail_field_index;
                    let prefix = if selected { "> " } else { "  " };
                    let value = if field.field_type == crate::vault::FieldType::Concealed
                        && !state.show_hidden
                    {
                        "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}".to_string()
                    } else if field.field_type == crate::vault::FieldType::Otp {
                        "(press 't' to copy TOTP)".to_string()
                    } else {
                        field.value.as_str().to_string()
                    };
                    let label = if field.label.is_empty() {
                        format!("{:?}", field.field_type)
                    } else {
                        field.label.clone()
                    };
                    lines.push((format!("{prefix}  {label}: {value}"), selected, false));
                }

                if !item.urls.is_empty() && item.urls.len() > 1 {
                    lines.push((String::new(), false, false));
                    lines.push(("  URLs:".into(), false, true));
                    for url in &item.urls {
                        let primary = if url.primary { " (primary)" } else { "" };
                        lines.push((format!("    {}{primary}", url.href), false, false));
                    }
                }

                lines.push((String::new(), false, false));
                lines.push((
                    "[j/k] navigate  [Enter/y] copy field  [p] password  [u] username  [t] TOTP  [H] toggle hidden  [Esc] back".into(),
                    false,
                    true,
                ));
            }
        }
    }

    if let Some(ref msg) = state.status_message {
        lines.push((String::new(), false, false));
        lines.push((msg.clone(), false, true));
    }

    lines
}

/// GPU renderer for the kagi vault browser.
pub struct KagiRenderer {
    /// Application state.
    pub state: KagiState,
    /// Background clear color.
    bg_color: wgpu::Color,
    /// Font size in pixels.
    font_size: f32,
    /// Line height in pixels.
    line_height: f32,
}

impl KagiRenderer {
    /// Create a new renderer with the given appearance config.
    #[must_use]
    pub fn new(appearance: &AppearanceConfig) -> Self {
        let bg = egaku::theme::hex_to_rgba(&appearance.background)
            .unwrap_or([0.180, 0.204, 0.251, 1.0]);

        Self {
            state: KagiState::new(),
            bg_color: wgpu::Color {
                r: f64::from(bg[0]),
                g: f64::from(bg[1]),
                b: f64::from(bg[2]),
                a: f64::from(bg[3]),
            },
            font_size: 16.0,
            line_height: 24.0,
        }
    }
}

impl madori::RenderCallback for KagiRenderer {
    fn init(&mut self, _gpu: &GpuContext) {
        tracing::info!("kagi GPU renderer initialized");
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.state.width = width;
        self.state.height = height;
        // Update visible count based on window height
        let visible = ((height as f32 - 48.0) / self.line_height).floor() as usize;
        self.state.vault_list = ListView::new(
            self.state.vault_list.visible_items().to_vec(),
            visible.max(5),
        );
        self.state.item_list = ListView::new(
            self.state.item_list.visible_items().to_vec(),
            visible.max(5),
        );
    }

    fn render(&mut self, ctx: &mut madori::RenderContext<'_>) {
        let mut encoder = ctx.gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("kagi_render"),
            },
        );

        // Pass 1: clear background
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kagi_clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: ctx.surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.bg_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        // Pass 2: text rendering
        let lines = collect_lines(&self.state);
        let padding = 16.0_f32;

        // Colors
        let normal_color = glyphon::Color::rgba(236, 239, 244, 255); // #eceff4
        let accent_color = glyphon::Color::rgba(136, 192, 208, 255); // #88c0d0
        let selected_color = glyphon::Color::rgba(163, 190, 140, 255); // #a3be8c (green)

        let mut buffers = Vec::new();
        for (text, selected, is_accent) in &lines {
            let color = if *selected {
                selected_color
            } else if *is_accent {
                accent_color
            } else {
                normal_color
            };
            let attrs = glyphon::Attrs::new().color(color);
            let mut buf = ctx.text.create_buffer(text, self.font_size, self.line_height);
            buf.set_text(&mut ctx.text.font_system, text, &attrs, glyphon::Shaping::Advanced);
            buf.shape_until_scroll(&mut ctx.text.font_system, false);
            buffers.push(buf);
        }

        let mut text_areas: Vec<glyphon::TextArea<'_>> = Vec::new();
        for (i, buffer) in buffers.iter().enumerate() {
            let y = padding + (i as f32 * self.line_height);
            text_areas.push(glyphon::TextArea {
                buffer,
                left: padding,
                top: y,
                scale: 1.0,
                bounds: glyphon::TextBounds {
                    left: 0,
                    top: 0,
                    right: ctx.width as i32,
                    bottom: ctx.height as i32,
                },
                default_color: normal_color,
                custom_glyphs: &[],
            });
        }

        if let Err(e) = ctx.text.prepare(
            &ctx.gpu.device,
            &ctx.gpu.queue,
            ctx.width,
            ctx.height,
            text_areas,
        ) {
            tracing::warn!("text prepare error: {e}");
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kagi_text"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: ctx.surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if let Err(e) = ctx.text.render(&mut pass) {
                tracing::warn!("text render error: {e}");
            }
        }

        ctx.gpu.queue.submit(std::iter::once(encoder.finish()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{Field, FieldPurpose, FieldType, ItemCategory, SecretValue};

    #[test]
    fn kagi_state_default_mode_is_vault_list() {
        let state = KagiState::new();
        assert_eq!(state.mode, ViewMode::VaultList);
    }

    #[test]
    fn set_vaults_updates_list() {
        let mut state = KagiState::new();
        state.set_vaults(vec![
            Vault {
                id: "v1".into(),
                name: "Personal".into(),
                description: None,
                items: 5,
            },
            Vault {
                id: "v2".into(),
                name: "Work".into(),
                description: None,
                items: 3,
            },
        ]);
        assert_eq!(state.vault_list.len(), 2);
        assert_eq!(state.vaults.len(), 2);
    }

    #[test]
    fn set_items_updates_list() {
        let mut state = KagiState::new();
        let items = vec![Item {
            id: "i1".into(),
            title: "GitHub".into(),
            vault_id: "v1".into(),
            category: ItemCategory::Login,
            urls: vec![],
            fields: vec![],
            tags: vec![],
            favorite: false,
            last_edited_by: None,
            created_at: None,
            updated_at: None,
        }];
        state.set_items(items);
        assert_eq!(state.item_list.len(), 1);
        assert_eq!(state.summaries.len(), 1);
    }

    #[test]
    fn move_down_and_up() {
        let mut state = KagiState::new();
        state.set_vaults(vec![
            Vault { id: "v1".into(), name: "A".into(), description: None, items: 0 },
            Vault { id: "v2".into(), name: "B".into(), description: None, items: 0 },
        ]);
        assert_eq!(state.vault_list.selected_index(), 0);
        state.move_down();
        assert_eq!(state.vault_list.selected_index(), 1);
        state.move_up();
        assert_eq!(state.vault_list.selected_index(), 0);
    }

    #[test]
    fn enter_and_exit_search() {
        let mut state = KagiState::new();
        state.mode = ViewMode::ItemList;
        state.enter_search();
        assert_eq!(state.mode, ViewMode::Search);
        state.go_back();
        assert_eq!(state.mode, ViewMode::ItemList);
    }

    #[test]
    fn go_back_from_detail() {
        let mut state = KagiState::new();
        state.mode = ViewMode::ItemList;
        state.detail_item = Some(Item {
            id: "i1".into(),
            title: "Test".into(),
            vault_id: "v1".into(),
            category: ItemCategory::Login,
            urls: vec![],
            fields: vec![],
            tags: vec![],
            favorite: false,
            last_edited_by: None,
            created_at: None,
            updated_at: None,
        });
        state.prev_mode = Some(ViewMode::ItemList);
        state.mode = ViewMode::ItemDetail;
        state.go_back();
        assert_eq!(state.mode, ViewMode::ItemList);
        assert!(state.detail_item.is_none());
    }

    #[test]
    fn search_filters_items() {
        let mut state = KagiState::new();
        let items = vec![
            Item {
                id: "i1".into(),
                title: "GitHub".into(),
                vault_id: "v1".into(),
                category: ItemCategory::Login,
                urls: vec![],
                fields: vec![Field {
                    id: "f1".into(),
                    label: "username".into(),
                    value: SecretValue::new("user@github.com"),
                    purpose: Some(FieldPurpose::Username),
                    field_type: FieldType::String,
                }],
                tags: vec![],
                favorite: false,
                last_edited_by: None,
                created_at: None,
                updated_at: None,
            },
            Item {
                id: "i2".into(),
                title: "AWS Console".into(),
                vault_id: "v1".into(),
                category: ItemCategory::Login,
                urls: vec![],
                fields: vec![],
                tags: vec![],
                favorite: false,
                last_edited_by: None,
                created_at: None,
                updated_at: None,
            },
        ];
        let summaries: Vec<ItemSummary> = items.iter().map(ItemSummary::from).collect();
        state.set_all_items(items, summaries);
        state.enter_search();
        state.search_input.insert_char('g');
        state.search_input.insert_char('i');
        state.search_input.insert_char('t');
        state.apply_search();
        assert_eq!(state.search_results.len(), 1);
        assert_eq!(state.search_results[0].title, "GitHub");
    }

    #[test]
    fn set_status_message() {
        let mut state = KagiState::new();
        state.set_status("Copied password");
        assert_eq!(state.status_message.as_deref(), Some("Copied password"));
    }

    #[test]
    fn renderer_creates_with_defaults() {
        let appearance = crate::config::AppearanceConfig::default();
        let renderer = KagiRenderer::new(&appearance);
        assert_eq!(renderer.state.mode, ViewMode::VaultList);
    }

    #[test]
    fn collect_lines_vault_list() {
        let state = KagiState::new();
        let lines = collect_lines(&state);
        assert!(!lines.is_empty());
        assert!(lines[0].0.contains("Select Vault"));
    }

    #[test]
    fn collect_lines_loading() {
        let mut state = KagiState::new();
        state.loading = true;
        let lines = collect_lines(&state);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].0.contains("Loading"));
    }

    #[test]
    fn collect_lines_item_detail() {
        let mut state = KagiState::new();
        state.mode = ViewMode::ItemDetail;
        state.detail_item = Some(Item {
            id: "i1".into(),
            title: "Test Item".into(),
            vault_id: "v1".into(),
            category: ItemCategory::Login,
            urls: vec![],
            fields: vec![Field {
                id: "f1".into(),
                label: "password".into(),
                value: SecretValue::new("secret"),
                purpose: Some(FieldPurpose::Password),
                field_type: FieldType::Concealed,
            }],
            tags: vec![],
            favorite: false,
            last_edited_by: None,
            created_at: None,
            updated_at: None,
        });
        let lines = collect_lines(&state);
        assert!(lines.iter().any(|(l, _, _)| l.contains("Test Item")));
        // Concealed field should show dots
        assert!(lines.iter().any(|(l, _, _)| l.contains('\u{2022}')));
    }

    #[test]
    fn format_summaries_favorites_filter() {
        let summaries = vec![
            ItemSummary {
                id: "1".into(),
                title: "Fav".into(),
                category: ItemCategory::Login,
                vault_id: "v".into(),
                vault_name: String::new(),
                url: None,
                username: None,
                favorite: true,
                has_totp: false,
            },
            ItemSummary {
                id: "2".into(),
                title: "NotFav".into(),
                category: ItemCategory::Login,
                vault_id: "v".into(),
                vault_name: String::new(),
                url: None,
                username: None,
                favorite: false,
                has_totp: false,
            },
        ];
        let all = format_summaries(&summaries, false);
        assert_eq!(all.len(), 2);
        let favs = format_summaries(&summaries, true);
        assert_eq!(favs.len(), 1);
        assert!(favs[0].contains("Fav"));
    }

    #[test]
    fn selected_ids_returns_correct_pair() {
        let mut state = KagiState::new();
        let items = vec![Item {
            id: "item1".into(),
            title: "Test".into(),
            vault_id: "vault1".into(),
            category: ItemCategory::Login,
            urls: vec![],
            fields: vec![],
            tags: vec![],
            favorite: false,
            last_edited_by: None,
            created_at: None,
            updated_at: None,
        }];
        state.set_items(items);
        let ids = state.selected_ids().unwrap();
        assert_eq!(ids.0, "vault1");
        assert_eq!(ids.1, "item1");
    }
}
