#[cfg(feature = "api")]
use crate::api::ApiClient;
use crate::config::{Config, Server};
use crate::filter::{GlobalMatch, filter_servers, global_search};
use crate::terminal::TerminalSession;

#[derive(PartialEq, Clone, Copy)]
pub enum SplitLayout {
    Vertical2,   // left | right
    Horizontal2, // top / bottom
    Triple,      // 1 top + 2 bottom
    Quad,        // 2x2 grid
}

pub struct SplitState {
    pub layout: SplitLayout,
    pub panes: Vec<usize>,     // tab indices
    pub focused_pane: usize,   // index into panes (0..panes.len())
}

#[derive(PartialEq, Clone, Copy)]
pub enum SidebarFocus {
    Sidebar,
    ServerList,
}

/// Item visível no sidebar (pode ser categoria ou header de grupo)
#[derive(Clone)]
pub enum SidebarItem {
    /// Categoria normal → category_index
    Category(usize),
    /// Header de grupo colapsável (ex: "VMs")
    GroupHeader { prefix: String, expanded: bool, count: usize },
    /// Sub-item de grupo → category_index
    GroupChild(usize),
    /// Conexão recente → index em recent_connections
    RecentHeader,
    Recent(usize),
}

#[derive(PartialEq, Clone, Copy)]
pub enum AppMode {
    /// Navegando no sidebar/lista de servidores
    Browse,
    /// Digitando na barra de busca
    Search,
    /// Terminal ativo recebendo input
    Terminal,
    /// Popup de detalhes do servidor
    Detail,
    /// Command palette overlay
    Palette,
    /// Help screen
    Help,
    /// Command input (mini prompt no terminal)
    CommandInput,
    /// Dangerous command confirmation
    ConfirmDanger,
    /// Easter egg 🎮
    Doom,
}

pub struct ContextMenu {
    pub x: u16,
    pub y: u16,
    pub items: Vec<ContextMenuItem>,
    pub selected: usize,
    #[allow(dead_code)]
    pub area: ContextArea,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ContextArea {
    Terminal,
    ServerList,
    Sidebar,
}

#[derive(Clone)]
pub struct ContextMenuItem {
    pub label: String,
    pub action: ContextAction,
}

#[derive(Clone)]
pub enum ContextAction {
    CopySelection,
    CopyLines(usize),
    CopyIp,
    Connect,
    Disconnect,
    ServerDetail,
    Split,
    Broadcast,
    Help,
    Reconnect,
}

pub struct PaletteItem {
    pub label: String,
    pub description: String,
    pub action: PaletteAction,
}

pub enum PaletteAction {
    Connect(Server),
    SwitchTab(usize),
    ToggleSplit,
    ToggleBroadcast,
    #[cfg(feature = "api")]
    Refresh,
    #[cfg(feature = "api")]
    Logout,
    CopyIp(String),
    RunCommand(String),
    ShowHelp,
    CloseTab,
    CopyLines(usize),
    Reconnect,
    ShowDetail,
}

pub struct App {
    pub config: Config,
    pub mode: AppMode,
    pub sidebar_focus: SidebarFocus,
    pub running: bool,

    // Sidebar
    pub category_index: usize,
    pub sidebar_items: Vec<SidebarItem>,
    pub sidebar_index: usize,
    pub vm_expanded: bool,

    // Server list
    pub server_index: usize,
    pub filtered_indices: Vec<usize>,
    pub search_query: String,

    // Busca global (/ no sidebar)
    pub global_results: Vec<GlobalMatch>,
    pub global_index: usize,
    pub is_global_search: bool,

    // Tabs de terminal
    pub tabs: Vec<TerminalSession>,
    pub active_tab: Option<usize>,

    // Detail popup
    pub detail_server: Option<Server>,

    // Recent connections (max 5)
    pub recent_connections: Vec<Server>,

    // Clipboard message (temporary)
    pub clipboard_msg: Option<String>,

    // Broadcast mode: input vai pra todas as tabs
    pub broadcast: bool,

    // Mouse: áreas clicáveis (atualizadas a cada draw)
    pub mouse_sidebar_area: (u16, u16, u16, u16),   // x, y, w, h
    pub mouse_serverlist_area: (u16, u16, u16, u16),
    pub mouse_sidebar_offset_y: u16, // y do primeiro item do sidebar
    pub mouse_tab_bar: Option<(u16, u16, Vec<(u16, u16)>)>, // (y, x_start, [(x_start, x_end) per tab])

    // Split terminal
    pub split: Option<SplitState>,

    // Command palette
    pub palette_query: String,
    pub palette_items: Vec<PaletteItem>,
    pub palette_filtered: Vec<usize>,
    pub palette_index: usize,

    // Multi-select servers
    pub selected_servers: Vec<usize>,

    // Auto-reconnect
    pub reconnect_server: Option<(usize, Server)>,

    // Help screen
    pub help_scroll: usize,
    pub prev_mode: AppMode,

    // Command input
    pub command_input: String,

    // Context menu (right-click)
    pub context_menu: Option<ContextMenu>,

    // Mouse text selection
    pub mouse_selecting: bool,
    pub mouse_select_start: Option<(u16, u16)>,  // (col, row) relativo ao terminal
    pub mouse_select_end: Option<(u16, u16)>,

    // Dangerous command confirmation
    pub danger_command: Option<String>,
    pub danger_broadcast: bool,

    // Doom Easter egg
    pub doom: Option<crate::doom::DoomGame>,

    // API client (JWT vive aqui na RAM)
    #[cfg(feature = "api")]
    pub api_client: Option<ApiClient>,

    // Gate API online status
    pub gate_online: bool,
}

impl App {
    pub fn new(config: Config) -> Self {
        let filtered = if config.categories.is_empty() {
            vec![]
        } else {
            (0..config.categories[0].servers.len()).collect()
        };

        let mut app = Self {
            config,
            mode: AppMode::Browse,
            sidebar_focus: SidebarFocus::Sidebar,
            running: true,
            category_index: 0,
            sidebar_items: Vec::new(),
            sidebar_index: 0,
            vm_expanded: false,
            server_index: 0,
            filtered_indices: filtered,
            search_query: String::new(),
            global_results: Vec::new(),
            global_index: 0,
            is_global_search: false,
            tabs: Vec::new(),
            active_tab: None,
            detail_server: None,
            recent_connections: Vec::new(),
            clipboard_msg: None,
            mouse_sidebar_area: (0, 0, 0, 0),
            mouse_serverlist_area: (0, 0, 0, 0),
            mouse_sidebar_offset_y: 0,
            mouse_tab_bar: None,
            broadcast: false,
            split: None,
            palette_query: String::new(),
            palette_items: Vec::new(),
            palette_filtered: Vec::new(),
            palette_index: 0,
            selected_servers: Vec::new(),
            reconnect_server: None,
            help_scroll: 0,
            prev_mode: AppMode::Browse,
            command_input: String::new(),
            context_menu: None,
            mouse_selecting: false,
            mouse_select_start: None,
            mouse_select_end: None,
            danger_command: None,
            danger_broadcast: false,
            doom: None,
            #[cfg(feature = "api")]
            api_client: None,
            gate_online: true,
        };
        app.rebuild_sidebar();
        app
    }

    // ─── Helpers ─────────────────────────────────────────────────────

    pub fn format_elapsed(instant: std::time::Instant) -> String {
        let secs = instant.elapsed().as_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m", secs / 60)
        } else {
            let h = secs / 3600;
            let m = (secs % 3600) / 60;
            if m > 0 {
                format!("{}h{}m", h, m)
            } else {
                format!("{}h", h)
            }
        }
    }

    // ─── Sidebar tree ──────────────────────────────────────────────

    /// Reconstrói a lista de items visíveis no sidebar
    pub fn rebuild_sidebar(&mut self) {
        self.sidebar_items.clear();

        // Recents section at the top
        if !self.recent_connections.is_empty() {
            self.sidebar_items.push(SidebarItem::RecentHeader);
            for i in 0..self.recent_connections.len() {
                self.sidebar_items.push(SidebarItem::Recent(i));
            }
        }

        let mut vm_total = 0usize;
        let mut vm_indices: Vec<usize> = Vec::new();

        for (i, cat) in self.config.categories.iter().enumerate() {
            if cat.name.starts_with("VMs > ") || cat.name == "VMs" {
                vm_total += cat.servers.len();
                vm_indices.push(i);
            } else {
                self.sidebar_items.push(SidebarItem::Category(i));
            }
        }

        // Adiciona grupo VMs se existir
        if !vm_indices.is_empty() {
            self.sidebar_items.push(SidebarItem::GroupHeader {
                prefix: "VMs".into(),
                expanded: self.vm_expanded,
                count: vm_total,
            });

            if self.vm_expanded {
                for idx in vm_indices {
                    self.sidebar_items.push(SidebarItem::GroupChild(idx));
                }
            }
        }
    }

    /// Retorna o category_index do item selecionado no sidebar
    fn selected_category_index(&self) -> Option<usize> {
        match self.sidebar_items.get(self.sidebar_index)? {
            SidebarItem::Category(i) | SidebarItem::GroupChild(i) => Some(*i),
            SidebarItem::GroupHeader { .. } | SidebarItem::RecentHeader | SidebarItem::Recent(_) => None,
        }
    }

    // ─── Servers / navegação ────────────────────────────────────────

    pub fn current_servers(&self) -> &[Server] {
        self.config
            .categories
            .get(self.category_index)
            .map(|c| c.servers.as_slice())
            .unwrap_or(&[])
    }

    pub fn filtered_servers(&self) -> Vec<&Server> {
        let servers = self.current_servers();
        self.filtered_indices
            .iter()
            .filter_map(|&i| servers.get(i))
            .collect()
    }

    pub fn refilter(&mut self) {
        let servers = self.current_servers();
        self.filtered_indices = filter_servers(servers, &self.search_query);
        if self.server_index >= self.filtered_indices.len() {
            self.server_index = self.filtered_indices.len().saturating_sub(1);
        }
    }

    pub fn select_category(&mut self, index: usize) {
        if index < self.config.categories.len() {
            self.category_index = index;
            self.server_index = 0;
            self.search_query.clear();
            self.refilter();
        }
    }

    pub fn move_up(&mut self) {
        match self.sidebar_focus {
            SidebarFocus::Sidebar => {
                if self.sidebar_index > 0 {
                    self.sidebar_index -= 1;
                    self.sync_category_from_sidebar();
                }
            }
            SidebarFocus::ServerList => {
                if self.server_index > 0 {
                    self.server_index -= 1;
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.sidebar_focus {
            SidebarFocus::Sidebar => {
                if self.sidebar_index + 1 < self.sidebar_items.len() {
                    self.sidebar_index += 1;
                    self.sync_category_from_sidebar();
                }
            }
            SidebarFocus::ServerList => {
                if self.server_index + 1 < self.filtered_indices.len() {
                    self.server_index += 1;
                }
            }
        }
    }

    pub fn jump_top(&mut self) {
        match self.sidebar_focus {
            SidebarFocus::Sidebar => {
                self.sidebar_index = 0;
                self.sync_category_from_sidebar();
            }
            SidebarFocus::ServerList => self.server_index = 0,
        }
    }

    pub fn jump_bottom(&mut self) {
        match self.sidebar_focus {
            SidebarFocus::Sidebar => {
                self.sidebar_index = self.sidebar_items.len().saturating_sub(1);
                self.sync_category_from_sidebar();
            }
            SidebarFocus::ServerList => {
                self.server_index = self.filtered_indices.len().saturating_sub(1);
            }
        }
    }

    /// Sincroniza category_index quando muda sidebar_index
    fn sync_category_from_sidebar(&mut self) {
        if let Some(ci) = self.selected_category_index() {
            self.select_category(ci);
        }
    }

    pub fn enter_nav(&mut self) {
        match self.sidebar_focus {
            SidebarFocus::Sidebar => {
                match self.sidebar_items.get(self.sidebar_index) {
                    // GroupHeader: toggle expand/collapse
                    Some(SidebarItem::GroupHeader { .. }) => {
                        self.vm_expanded = !self.vm_expanded;
                        self.rebuild_sidebar();
                        return;
                    }
                    // RecentHeader: non-interactive
                    Some(SidebarItem::RecentHeader) => {}
                    // Recent item: connect directly
                    Some(SidebarItem::Recent(idx)) => {
                        let idx = *idx;
                        if let Some(server) = self.recent_connections.get(idx).cloned() {
                            self.open_terminal(&server);
                        }
                    }
                    // Category or GroupChild: focus server list
                    _ => {
                        if self.selected_category_index().is_some() {
                            self.sidebar_focus = SidebarFocus::ServerList;
                            self.server_index = 0;
                        }
                    }
                }
            }
            SidebarFocus::ServerList => {
                if self.has_selection() {
                    self.connect_selected();
                } else if let Some(&real_index) = self.filtered_indices.get(self.server_index) {
                    let server = self.current_servers()[real_index].clone();
                    self.open_terminal(&server);
                }
            }
        }
    }

    pub fn go_back_nav(&mut self) {
        match self.mode {
            AppMode::Search => {
                self.mode = AppMode::Browse;
            }
            AppMode::Detail => {
                self.close_detail();
            }
            AppMode::Browse => {
                if self.sidebar_focus == SidebarFocus::ServerList {
                    self.sidebar_focus = SidebarFocus::Sidebar;
                }
            }
            AppMode::Terminal => {}
            AppMode::Palette => {
                self.close_palette();
            }
            AppMode::Help => {
                self.close_help();
            }
            AppMode::CommandInput => {
                self.cancel_command_input();
            }
            AppMode::ConfirmDanger => {
                self.cancel_danger();
            }
            AppMode::Doom => {
                self.mode = AppMode::Terminal;
            }
        }
    }

    pub fn toggle_sidebar_focus(&mut self) {
        self.sidebar_focus = match self.sidebar_focus {
            SidebarFocus::Sidebar => SidebarFocus::ServerList,
            SidebarFocus::ServerList => SidebarFocus::Sidebar,
        };
    }

    // ─── Detail popup ─────────────────────────────────────────────

    pub fn show_detail(&mut self) {
        if let Some(&real_index) = self.filtered_indices.get(self.server_index) {
            let server = self.current_servers()[real_index].clone();
            self.detail_server = Some(server);
            self.mode = AppMode::Detail;
        }
    }

    pub fn close_detail(&mut self) {
        self.detail_server = None;
        self.mode = AppMode::Browse;
    }

    // ─── Recent connections ─────────────────────────────────────────

    fn push_recent(&mut self, server: &Server) {
        // Remove duplicate by name
        self.recent_connections.retain(|s| s.name != server.name);
        // Push to front
        self.recent_connections.insert(0, server.clone());
        // Keep max 5
        self.recent_connections.truncate(5);
        self.rebuild_sidebar();
    }

    #[allow(dead_code)]
    pub fn recent_servers(&self) -> &[Server] {
        &self.recent_connections
    }

    // ─── Clipboard ──────────────────────────────────────────────────

    pub fn copy_ip(&mut self) {
        // Clear any previous message
        self.clipboard_msg = None;

        if let Some(&real_index) = self.filtered_indices.get(self.server_index) {
            let server = &self.current_servers()[real_index];
            let ip = if !server.host.is_empty() {
                server.host.clone()
            } else {
                return;
            };

            if copy_to_clipboard(&ip) {
                self.clipboard_msg = Some(format!("IP copied: {}", ip));
            } else {
                self.clipboard_msg = Some("Failed to copy IP".to_string());
            }
        }
    }

    pub fn clear_clipboard_msg(&mut self) {
        self.clipboard_msg = None;
    }

    // ─── Multi-select ────────────────────────────────────────────────

    pub fn toggle_select(&mut self) {
        if let Some(&_real_index) = self.filtered_indices.get(self.server_index) {
            let idx = self.server_index;
            if let Some(pos) = self.selected_servers.iter().position(|&i| i == idx) {
                self.selected_servers.remove(pos);
            } else {
                self.selected_servers.push(idx);
            }
        }
    }

    pub fn connect_selected(&mut self) {
        let indices: Vec<usize> = self.selected_servers.drain(..).collect();
        let servers: Vec<Server> = indices
            .iter()
            .filter_map(|&i| {
                self.filtered_indices
                    .get(i)
                    .and_then(|&real| self.current_servers().get(real).cloned())
            })
            .collect();
        let count = servers.len();
        for server in &servers {
            self.open_terminal(server);
        }
        // Auto-split baseado na quantidade conectada
        if count >= 4 {
            self.split = Some(SplitState {
                layout: SplitLayout::Quad,
                panes: self.pick_panes(4),
                focused_pane: 0,
            });
        } else if count == 3 {
            self.split = Some(SplitState {
                layout: SplitLayout::Triple,
                panes: self.pick_panes(3),
                focused_pane: 0,
            });
        } else if count == 2 {
            self.split = Some(SplitState {
                layout: SplitLayout::Vertical2,
                panes: self.pick_panes(2),
                focused_pane: 0,
            });
        }
    }

    pub fn has_selection(&self) -> bool {
        !self.selected_servers.is_empty()
    }

    pub fn is_selected(&self, idx: usize) -> bool {
        self.selected_servers.contains(&idx)
    }

    // ─── Auto-reconnect ────────────────────────────────────────────

    /// Check if the active session is dead and store reconnect info
    pub fn check_dead_sessions(&mut self) {
        if self.reconnect_server.is_some() {
            return;
        }
        if let Some(idx) = self.active_tab {
            if let Some(session) = self.tabs.get(idx) {
                if session.is_dead() {
                    // Find the server info - search by name in recent connections
                    let name = session.name.clone();
                    let server = self.recent_connections.iter().find(|s| s.name == name).cloned()
                        .unwrap_or_else(|| {
                            // Fallback: create a minimal Server from what we know
                            Server { name, ..Default::default() }
                        });
                    self.reconnect_server = Some((idx, server));
                }
            }
        }
    }

    /// Reconnect: replace the dead tab with a new session for the same server
    pub fn reconnect_active(&mut self) {
        if let Some((tab_idx, server)) = self.reconnect_server.take() {
            let key = self.config.settings.ssh_key.clone();
            match TerminalSession::new(&server, &key, 220, 50) {
                Ok(session) => {
                    if tab_idx < self.tabs.len() {
                        self.tabs[tab_idx] = session;
                    } else {
                        self.tabs.push(session);
                        self.active_tab = Some(self.tabs.len() - 1);
                    }
                    self.mode = AppMode::Terminal;
                }
                Err(_e) => {
                    // Failed to reconnect, dismiss instead
                    self.dismiss_dead();
                }
            }
        }
    }

    /// Dismiss the dead tab (close it)
    pub fn dismiss_dead(&mut self) {
        self.reconnect_server = None;
        self.close_active_tab();
    }

    // ─── Help screen ───────────────────────────────────────────────

    pub fn show_help(&mut self) {
        self.prev_mode = self.mode;
        self.help_scroll = 0;
        self.mode = AppMode::Help;
    }

    pub fn close_help(&mut self) {
        self.mode = self.prev_mode;
    }

    // ─── Command input ──────────────────────────────────────────────

    pub fn open_command_input(&mut self) {
        self.command_input.clear();
        self.mode = AppMode::CommandInput;
    }

    pub fn command_input_push(&mut self, c: char) {
        self.command_input.push(c);
    }

    pub fn command_input_backspace(&mut self) {
        self.command_input.pop();
    }

    pub fn execute_command_input(&mut self) {
        let input = self.command_input.trim().to_string();
        self.command_input.clear();
        self.mode = AppMode::Terminal;

        if input.is_empty() {
            return;
        }

        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let arg = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match cmd.as_str() {
            // :copy N — copia últimas N linhas (default: tela inteira)
            "copy" | "c" | "cp" => {
                let lines: usize = arg.parse().unwrap_or(50);
                if let Some(session) = self.active_session() {
                    let text = session.copy_lines(lines);
                    if copy_to_clipboard(&text) {
                        self.clipboard_msg = Some(
                            if lines > 0 { format!("{} lines copied!", lines) }
                            else { "Screen copied!".into() }
                        );
                    } else {
                        self.clipboard_msg = Some("Failed: install xclip (sudo apt install xclip)".into());
                    }
                }
            }
            // :scroll N — scroll up N linhas
            "scroll" | "s" => {
                let lines: usize = arg.parse().unwrap_or(10);
                if let Some(session) = self.active_session_mut() {
                    session.scroll_up(lines);
                }
            }
            // :run CMD — executa comando no terminal
            "run" | "r" | "!" => {
                if !arg.is_empty() {
                    if Self::is_dangerous_command(arg) {
                        self.danger_command = Some(arg.to_string());
                        self.danger_broadcast = self.broadcast;
                        self.mode = AppMode::ConfirmDanger;
                    } else {
                        let shell_cmd = format!("{}\n", arg);
                        let bytes = shell_cmd.into_bytes();
                        if self.broadcast {
                            self.write_input_all(&bytes);
                        } else if let Some(session) = self.active_session_mut() {
                            session.write_input(bytes);
                        }
                    }
                }
            }
            // :doom — Easter egg 🎮
            "doom" => {
                self.doom = Some(crate::doom::DoomGame::new(80));
                self.mode = AppMode::Doom;
            }
            // :split — toggle split
            "split" | "sp" => { self.toggle_split(); }
            // :broadcast — toggle broadcast
            "broadcast" | "bc" => { self.toggle_broadcast(); }
            // :close — fecha tab
            "close" | "q" => { self.close_active_tab(); }
            // :help — mostra help
            "help" | "h" | "?" => { self.show_help(); }
            // Fallback: execute as shell command
            _ => {
                if Self::is_dangerous_command(&input) {
                    self.danger_command = Some(input);
                    self.danger_broadcast = self.broadcast;
                    self.mode = AppMode::ConfirmDanger;
                } else {
                    let shell_cmd = format!("{}\n", input);
                    let bytes = shell_cmd.into_bytes();
                    if self.broadcast {
                        self.write_input_all(&bytes);
                    } else if let Some(session) = self.active_session_mut() {
                        session.write_input(bytes);
                    }
                }
            }
        }
    }

    pub fn cancel_command_input(&mut self) {
        self.command_input.clear();
        self.mode = AppMode::Terminal;
    }

    // ─── Search ─────────────────────────────────────────────────────

    pub fn search_push(&mut self, c: char) {
        self.search_query.push(c);
        if self.is_global_search {
            self.global_results = global_search(&self.config.categories, &self.search_query);
            self.global_index = 0;
        } else {
            self.refilter();
        }
    }

    pub fn search_backspace(&mut self) {
        self.search_query.pop();
        if self.is_global_search {
            self.global_results = global_search(&self.config.categories, &self.search_query);
            self.global_index = 0;
        } else {
            self.refilter();
        }
    }

    pub fn confirm_search(&mut self) {
        self.mode = AppMode::Browse;

        // Se busca global: navega pra categoria/server selecionado
        if self.is_global_search {
            if let Some(m) = self.global_results.get(self.global_index) {
                self.category_index = m.category_index;
                self.server_index = m.server_index;
                self.search_query.clear();
                self.refilter();
                self.sidebar_focus = SidebarFocus::ServerList;
            }
            self.is_global_search = false;
            self.global_results.clear();
        }
    }

    pub fn global_move_up(&mut self) {
        if self.global_index > 0 {
            self.global_index -= 1;
        }
    }

    pub fn global_move_down(&mut self) {
        if self.global_index + 1 < self.global_results.len() {
            self.global_index += 1;
        }
    }

    /// Conecta direto ao server selecionado na busca global
    pub fn global_connect(&mut self) {
        if let Some(m) = self.global_results.get(self.global_index) {
            let server = m.server.clone();
            self.mode = AppMode::Browse;
            self.is_global_search = false;
            self.global_results.clear();
            self.open_terminal(&server);
        }
    }


    // ─── Tabs de terminal ────────────────────────────────────────────

    pub fn open_terminal(&mut self, server: &Server) {
        self.push_recent(server);
        let key = self.config.settings.ssh_key.clone();
        // Dimensões iniciais razoáveis — serão ajustadas no primeiro resize
        match TerminalSession::new(server, &key, 220, 50) {
            Ok(session) => {
                self.tabs.push(session);
                self.active_tab = Some(self.tabs.len() - 1);
                self.mode = AppMode::Terminal;
            }
            Err(e) => {
                // TODO: mostrar erro no status bar
                eprintln!("Error opening terminal: {e}");
            }
        }
    }

    pub fn close_active_tab(&mut self) {
        if let Some(idx) = self.active_tab {
            self.tabs.remove(idx);
            if self.tabs.is_empty() {
                self.active_tab = None;
                self.mode = AppMode::Browse;
                self.split = None;
            } else {
                let new_idx = idx.saturating_sub(1);
                self.active_tab = Some(new_idx.min(self.tabs.len() - 1));

                // Validate split panes after closing a tab
                if let Some(ref mut split) = self.split {
                    let tab_len = self.tabs.len();
                    // Adjust pane indices: any index >= idx needs to shift down
                    for p in split.panes.iter_mut() {
                        if *p >= idx {
                            *p = p.saturating_sub(1);
                        }
                        // Clamp to valid range
                        if *p >= tab_len {
                            *p = tab_len - 1;
                        }
                    }
                    // Remove duplicate panes
                    split.panes.dedup();
                    // Disable split if fewer than 2 unique panes remain
                    if split.panes.len() < 2 || tab_len < 2 {
                        self.split = None;
                    } else {
                        if split.focused_pane >= split.panes.len() {
                            split.focused_pane = split.panes.len() - 1;
                        }
                        self.active_tab = Some(split.panes[split.focused_pane]);
                    }
                }
            }
        }
    }

    pub fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let idx = self.active_tab.unwrap_or(0);
        let new_idx = (idx + 1) % self.tabs.len();
        self.active_tab = Some(new_idx);
        if let Some(ref mut split) = self.split {
            split.panes[split.focused_pane] = new_idx;
        }
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let idx = self.active_tab.unwrap_or(0);
        let new_idx = if idx == 0 { self.tabs.len() - 1 } else { idx - 1 };
        self.active_tab = Some(new_idx);
        if let Some(ref mut split) = self.split {
            split.panes[split.focused_pane] = new_idx;
        }
    }

    pub fn switch_to_browse(&mut self) {
        self.mode = AppMode::Browse;
    }

    pub fn switch_to_terminal(&mut self) {
        if self.active_tab.is_some() {
            self.mode = AppMode::Terminal;
        }
    }

    pub fn active_session_mut(&mut self) -> Option<&mut TerminalSession> {
        self.active_tab.and_then(|i| self.tabs.get_mut(i))
    }

    pub fn active_session(&self) -> Option<&TerminalSession> {
        self.active_tab.and_then(|i| self.tabs.get(i))
    }

    /// Envia input pra todas as tabs (broadcast mode)
    pub fn write_input_all(&mut self, bytes: &[u8]) {
        for session in &mut self.tabs {
            session.write_input(bytes.to_vec());
        }
    }

    pub fn toggle_broadcast(&mut self) {
        self.broadcast = !self.broadcast;
    }

    pub fn resize_active_terminal(&mut self, cols: u16, rows: u16) {
        if let Some(session) = self.active_session_mut() {
            session.resize(cols, rows);
        }
    }

    // ─── Split terminal ───────────────────────────────────────────────

    pub fn is_split(&self) -> bool {
        self.split.is_some()
    }

    fn pick_panes(&self, count: usize) -> Vec<usize> {
        let n = self.tabs.len();
        if n == 0 {
            return vec![];
        }
        let start = self.active_tab.unwrap_or(0);
        let mut panes = Vec::with_capacity(count);
        for i in 0..count {
            panes.push((start + i) % n);
        }
        panes
    }

    pub fn toggle_split(&mut self) {
        let tab_count = self.tabs.len();
        let next = match &self.split {
            None => {
                if tab_count >= 2 {
                    Some(SplitLayout::Vertical2)
                } else {
                    None
                }
            }
            Some(s) => match s.layout {
                SplitLayout::Vertical2 => {
                    if tab_count >= 2 {
                        Some(SplitLayout::Horizontal2)
                    } else {
                        None
                    }
                }
                SplitLayout::Horizontal2 => {
                    if tab_count >= 3 {
                        Some(SplitLayout::Triple)
                    } else {
                        None
                    }
                }
                SplitLayout::Triple => {
                    if tab_count >= 4 {
                        Some(SplitLayout::Quad)
                    } else {
                        None
                    }
                }
                SplitLayout::Quad => None,
            },
        };

        match next {
            Some(layout) => {
                let pane_count = match layout {
                    SplitLayout::Vertical2 | SplitLayout::Horizontal2 => 2,
                    SplitLayout::Triple => 3,
                    SplitLayout::Quad => 4,
                };
                let panes = self.pick_panes(pane_count);
                self.active_tab = Some(panes[0]);
                self.split = Some(SplitState {
                    layout,
                    panes,
                    focused_pane: 0,
                });
            }
            None => {
                self.split = None;
            }
        }
    }


    // ─── Dangerous command detection ─────────────────────────────────

    pub fn is_dangerous_command(cmd: &str) -> bool {
        let lower = cmd.to_lowercase();
        let patterns = [
            "reboot", "shutdown", "poweroff", "halt",
            "rm -rf", "rm -r /", "rmdir",
            "mkfs", "dd if=", "fdisk",
            "iptables -f", "iptables -x",
            "systemctl stop", "systemctl disable",
            "kill -9", "killall",
            "chmod 777", "chmod -r",
            "> /dev/sda", "> /dev/null",
        ];
        patterns.iter().any(|p| lower.contains(p))
    }

    pub fn confirm_danger(&mut self) {
        if let Some(cmd) = self.danger_command.take() {
            // Se o comando veio do :run ou palette, manda cmd+\n
            // Se veio da interceptação do Enter (já digitado no terminal), manda só \n
            let is_from_terminal = self.mode == AppMode::ConfirmDanger;
            let bytes: Vec<u8> = if cmd.contains('\n') || !is_from_terminal {
                format!("{}\n", cmd).into_bytes()
            } else {
                // Comando já tá digitado no terminal, só manda Enter
                b"\r".to_vec()
            };
            if self.danger_broadcast {
                self.write_input_all(&bytes);
            } else if let Some(session) = self.active_session_mut() {
                session.write_input(bytes);
            }
        }
        self.danger_broadcast = false;
        self.mode = AppMode::Terminal;
    }

    pub fn cancel_danger(&mut self) {
        self.danger_command = None;
        self.danger_broadcast = false;
        self.mode = AppMode::Terminal;
    }

    // ─── Command Palette ──────────────────────────────────────────────

    pub fn open_palette(&mut self) {
        self.palette_query.clear();
        self.palette_items.clear();
        self.palette_index = 0;

        // All servers from all categories
        for cat in &self.config.categories {
            for server in &cat.servers {
                self.palette_items.push(PaletteItem {
                    label: format!("Connect: {} ({})", server.name, server.display_addr()),
                    description: cat.name.clone(),
                    action: PaletteAction::Connect(server.clone()),
                });
            }
        }

        // All open tabs
        for (i, tab) in self.tabs.iter().enumerate() {
            self.palette_items.push(PaletteItem {
                label: format!("Tab: {}", tab.name),
                description: format!("Switch to tab {}", i + 1),
                action: PaletteAction::SwitchTab(i),
            });
        }

        // Actions
        self.palette_items.push(PaletteItem {
            label: "Split: Toggle".to_string(),
            description: "Toggle split terminal layout".to_string(),
            action: PaletteAction::ToggleSplit,
        });
        self.palette_items.push(PaletteItem {
            label: "Broadcast: Toggle".to_string(),
            description: "Toggle broadcast mode".to_string(),
            action: PaletteAction::ToggleBroadcast,
        });

        #[cfg(feature = "api")]
        if self.is_api_mode() {
            self.palette_items.push(PaletteItem {
                label: "Refresh servers".to_string(),
                description: "Reload servers from API".to_string(),
                action: PaletteAction::Refresh,
            });
            self.palette_items.push(PaletteItem {
                label: "Logout".to_string(),
                description: "Logout and exit".to_string(),
                action: PaletteAction::Logout,
            });
        }

        // Quick commands from config
        for snippet in &self.config.commands {
            self.palette_items.push(PaletteItem {
                label: format!("Run: {} [{}]", snippet.name, snippet.key),
                description: snippet.command.clone(),
                action: PaletteAction::RunCommand(snippet.command.clone()),
            });
        }

        // Additional palette actions
        self.palette_items.push(PaletteItem {
            label: "Copy: Screen (50 lines)".to_string(),
            description: "Copy last 50 lines to clipboard".to_string(),
            action: PaletteAction::CopyLines(50),
        });
        self.palette_items.push(PaletteItem {
            label: "Copy: Last 100 lines".to_string(),
            description: "Copy last 100 lines to clipboard".to_string(),
            action: PaletteAction::CopyLines(100),
        });
        self.palette_items.push(PaletteItem {
            label: "View: Server details".to_string(),
            description: "Show details for current server".to_string(),
            action: PaletteAction::ShowDetail,
        });
        self.palette_items.push(PaletteItem {
            label: "View: Help (F1)".to_string(),
            description: "Show keyboard shortcuts".to_string(),
            action: PaletteAction::ShowHelp,
        });
        self.palette_items.push(PaletteItem {
            label: "Session: Close tab (Ctrl+W)".to_string(),
            description: "Close active terminal tab".to_string(),
            action: PaletteAction::CloseTab,
        });
        self.palette_items.push(PaletteItem {
            label: "Session: Reconnect".to_string(),
            description: "Reconnect dead session".to_string(),
            action: PaletteAction::Reconnect,
        });

        // Copy IP for all servers
        for cat in &self.config.categories {
            for server in &cat.servers {
                if !server.host.is_empty() {
                    self.palette_items.push(PaletteItem {
                        label: format!("Copy IP: {} ({})", server.name, server.host),
                        description: cat.name.clone(),
                        action: PaletteAction::CopyIp(server.host.clone()),
                    });
                }
            }
        }

        self.palette_filtered = (0..self.palette_items.len()).collect();
        self.mode = AppMode::Palette;
    }

    pub fn palette_filter(&mut self) {
        let query = self.palette_query.to_lowercase();
        if query.is_empty() {
            self.palette_filtered = (0..self.palette_items.len()).collect();
        } else {
            self.palette_filtered = self.palette_items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    let haystack = format!("{} {}", item.label, item.description).to_lowercase();
                    query.split_whitespace().all(|word| haystack.contains(word))
                })
                .map(|(i, _)| i)
                .collect();
        }
        if self.palette_index >= self.palette_filtered.len() {
            self.palette_index = 0;
        }
    }

    pub fn palette_push(&mut self, c: char) {
        self.palette_query.push(c);
        self.palette_filter();
    }

    pub fn palette_backspace(&mut self) {
        self.palette_query.pop();
        self.palette_filter();
    }

    pub fn palette_move_up(&mut self) {
        if self.palette_index > 0 {
            self.palette_index -= 1;
        }
    }

    pub fn palette_move_down(&mut self) {
        if self.palette_index + 1 < self.palette_filtered.len() {
            self.palette_index += 1;
        }
    }

    pub fn palette_execute(&mut self) {
        if let Some(&item_idx) = self.palette_filtered.get(self.palette_index) {
            // We need to extract the action data before mutating self
            // Use indices/cloned data to avoid borrow conflicts
            let action_data = match &self.palette_items[item_idx].action {
                PaletteAction::Connect(server) => PaletteActionData::Connect(server.clone()),
                PaletteAction::SwitchTab(i) => PaletteActionData::SwitchTab(*i),
                PaletteAction::ToggleSplit => PaletteActionData::ToggleSplit,
                PaletteAction::ToggleBroadcast => PaletteActionData::ToggleBroadcast,
                #[cfg(feature = "api")]
                PaletteAction::Refresh => PaletteActionData::Refresh,
                #[cfg(feature = "api")]
                PaletteAction::Logout => PaletteActionData::Logout,
                PaletteAction::CopyIp(ip) => PaletteActionData::CopyIp(ip.clone()),
                PaletteAction::RunCommand(cmd) => PaletteActionData::RunCommand(cmd.clone()),
                PaletteAction::ShowHelp => PaletteActionData::ShowHelp,
                PaletteAction::CloseTab => PaletteActionData::CloseTab,
                PaletteAction::CopyLines(n) => PaletteActionData::CopyLines(*n),
                PaletteAction::Reconnect => PaletteActionData::Reconnect,
                PaletteAction::ShowDetail => PaletteActionData::ShowDetail,
            };

            self.close_palette();

            match action_data {
                PaletteActionData::Connect(server) => {
                    self.open_terminal(&server);
                }
                PaletteActionData::SwitchTab(i) => {
                    if i < self.tabs.len() {
                        self.active_tab = Some(i);
                        self.mode = AppMode::Terminal;
                    }
                }
                PaletteActionData::ToggleSplit => {
                    self.toggle_split();
                }
                PaletteActionData::ToggleBroadcast => {
                    self.toggle_broadcast();
                }
                #[cfg(feature = "api")]
                PaletteActionData::Refresh => {
                    let _ = self.refresh_from_api();
                }
                #[cfg(feature = "api")]
                PaletteActionData::Logout => {
                    self.logout();
                }
                PaletteActionData::CopyIp(ip) => {
                    if copy_to_clipboard(&ip) {
                        self.clipboard_msg = Some(format!("IP copied: {}", ip));
                    } else {
                        self.clipboard_msg = Some("Failed to copy IP".to_string());
                    }
                }
                PaletteActionData::RunCommand(cmd) => {
                    if Self::is_dangerous_command(&cmd) {
                        self.danger_command = Some(cmd);
                        self.danger_broadcast = self.broadcast;
                        self.mode = AppMode::ConfirmDanger;
                    } else {
                        let bytes = format!("{}\n", cmd).into_bytes();
                        if self.broadcast {
                            self.write_input_all(&bytes);
                        } else if let Some(session) = self.active_session_mut() {
                            session.write_input(bytes);
                        }
                        if self.active_tab.is_some() {
                            self.mode = AppMode::Terminal;
                        }
                    }
                }
                PaletteActionData::ShowHelp => {
                    self.show_help();
                }
                PaletteActionData::CloseTab => {
                    self.close_active_tab();
                }
                PaletteActionData::CopyLines(n) => {
                    if let Some(session) = self.active_session() {
                        let text = session.copy_lines(n);
                        if copy_to_clipboard(&text) {
                            self.clipboard_msg = Some(format!("{} lines copied!", n));
                        } else {
                            self.clipboard_msg = Some("Failed: install xclip (sudo apt install xclip)".into());
                        }
                    }
                }
                PaletteActionData::Reconnect => {
                    self.reconnect_active();
                }
                PaletteActionData::ShowDetail => {
                    self.show_detail();
                }
            }
        } else {
            self.close_palette();
        }
    }

    pub fn close_palette(&mut self) {
        self.mode = AppMode::Browse;
        self.palette_query.clear();
        self.palette_items.clear();
        self.palette_filtered.clear();
        self.palette_index = 0;
    }

    // ─── API ─────────────────────────────────────────────────────────

    #[cfg(feature = "api")]
    pub fn set_api_client(&mut self, client: ApiClient) {
        self.api_client = Some(client);
    }

    /// Logout — invalida JWT no servidor, fecha tudo, sai
    #[cfg(feature = "api")]
    pub fn logout(&mut self) {
        // Fecha todas as sessões de terminal
        self.tabs.clear();
        self.active_tab = None;

        // Invalida JWT no servidor
        if let Some(client) = self.api_client.take() {
            client.logout();
        }

        self.running = false;
    }

    /// Refresh — recarrega hosts/VMs da API
    #[cfg(feature = "api")]
    pub fn refresh_from_api(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref client) = self.api_client {
            let categories = client.refresh_categories()?;
            self.config.categories = categories;
            self.category_index = 0;
            self.sidebar_index = 0;
            self.server_index = 0;
            self.search_query.clear();
            self.rebuild_sidebar();
            self.refilter();
        }
        Ok(())
    }

    /// Verifica se a Gate API esta online e tenta renovar token se necessario
    #[cfg(feature = "api")]
    pub fn check_gate_status(&mut self) {
        if let Some(ref mut client) = self.api_client {
            let online = client.check_api_status();
            self.gate_online = online;
            // Se a API esta online, tenta renovar o token proativamente
            if online {
                client.auto_refresh();
            }
        }
    }

    pub fn is_api_mode(&self) -> bool {
        #[cfg(feature = "api")]
        { self.api_client.is_some() }
        #[cfg(not(feature = "api"))]
        { false }
    }

    // ─── Mouse ────────────────────────────────────────────────────────

    /// Clique no sidebar — seleciona item
    pub fn mouse_click_sidebar(&mut self, row: u16) {
        let offset_y = self.mouse_sidebar_offset_y;
        if row < offset_y { return; }
        let idx = (row - offset_y) as usize;
        if idx < self.sidebar_items.len() {
            self.sidebar_index = idx;
            self.sidebar_focus = SidebarFocus::Sidebar;
            self.mode = AppMode::Browse;
            self.sync_category_from_sidebar();
        }
    }

    /// Clique na lista de servidores — seleciona
    pub fn mouse_click_serverlist(&mut self, row: u16) {
        let (_, sy, _, _) = self.mouse_serverlist_area;
        // -1 pro header da tabela
        if row <= sy + 1 { return; }
        let idx = (row - sy - 2) as usize;
        if idx < self.filtered_indices.len() {
            self.server_index = idx;
            self.sidebar_focus = SidebarFocus::ServerList;
            self.mode = AppMode::Browse;
        }
    }

    /// Scroll na lista de servidores
    pub fn mouse_scroll_serverlist(&mut self, up: bool) {
        if up {
            if self.server_index > 0 { self.server_index -= 1; }
        } else if self.server_index + 1 < self.filtered_indices.len() {
            self.server_index += 1;
        }
    }

    /// Clique na tab bar — troca pra aba clicada
    pub fn mouse_click_tab(&mut self, mx: u16) {
        if let Some((_, _x_start, ref ranges)) = self.mouse_tab_bar {
            for (i, &(tab_x_start, tab_x_end)) in ranges.iter().enumerate() {
                if mx >= tab_x_start && mx < tab_x_end && i < self.tabs.len() {
                    self.active_tab = Some(i);
                    return;
                }
            }
        }
    }

    /// Clique num split pane — troca foco pro painel clicado
    pub fn mouse_click_split_pane(&mut self, mx: u16, my: u16) {
        let split = match self.split.as_mut() {
            Some(s) => s,
            None => return,
        };

        let (lx, ly, lw, lh) = self.mouse_serverlist_area;
        // Calcula em qual painel o clique caiu
        let rel_x = mx.saturating_sub(lx);
        let rel_y = my.saturating_sub(ly);
        let half_w = lw / 2;
        let half_h = lh / 2;

        let pane = match split.layout {
            SplitLayout::Vertical2 => {
                if rel_x < half_w { 0 } else { 1 }
            }
            SplitLayout::Horizontal2 => {
                if rel_y < half_h { 0 } else { 1 }
            }
            SplitLayout::Triple => {
                // pane 0 = top full, pane 1 = bottom-left, pane 2 = bottom-right
                if rel_y < half_h { 0 }
                else if rel_x < half_w { 1 }
                else { 2 }
            }
            SplitLayout::Quad => {
                let col = if rel_x < half_w { 0 } else { 1 };
                let row = if rel_y < half_h { 0 } else { 1 };
                row * 2 + col
            }
        };

        if pane < split.panes.len() {
            split.focused_pane = pane;
            self.active_tab = Some(split.panes[pane]);
        }
    }

    // ─── Context menu ─────────────────────────────────────────────

    pub fn open_context_menu_terminal(&mut self, x: u16, y: u16) {
        let mut items = vec![];
        let is_dead = self.active_session().map(|s| s.is_dead()).unwrap_or(false);

        if is_dead {
            items.push(ContextMenuItem { label: "Reconnect".into(), action: ContextAction::Reconnect });
            items.push(ContextMenuItem { label: "Close tab".into(), action: ContextAction::Disconnect });
        } else {
            if self.mouse_select_start.is_some() && self.mouse_select_end.is_some() {
                items.push(ContextMenuItem { label: "Copy".into(), action: ContextAction::CopySelection });
            }
            items.push(ContextMenuItem { label: "Copy 50 lines".into(), action: ContextAction::CopyLines(50) });
            items.push(ContextMenuItem { label: "Copy 100 lines".into(), action: ContextAction::CopyLines(100) });
            items.push(ContextMenuItem { label: "Close tab".into(), action: ContextAction::Disconnect });
            if self.tabs.len() >= 2 {
                items.push(ContextMenuItem { label: "Split".into(), action: ContextAction::Split });
            }
            items.push(ContextMenuItem { label: "Broadcast".into(), action: ContextAction::Broadcast });
        }
        items.push(ContextMenuItem { label: "Help".into(), action: ContextAction::Help });

        self.context_menu = Some(ContextMenu { x, y, items, selected: 0, area: ContextArea::Terminal });
    }

    pub fn open_context_menu_serverlist(&mut self, x: u16, y: u16) {
        let items = vec![
            ContextMenuItem { label: "Connect".into(), action: ContextAction::Connect },
            ContextMenuItem { label: "Copy IP".into(), action: ContextAction::CopyIp },
            ContextMenuItem { label: "Details".into(), action: ContextAction::ServerDetail },
            ContextMenuItem { label: "Help".into(), action: ContextAction::Help },
        ];
        self.context_menu = Some(ContextMenu { x, y, items, selected: 0, area: ContextArea::ServerList });
    }

    pub fn open_context_menu_sidebar(&mut self, x: u16, y: u16) {
        let items = vec![
            ContextMenuItem { label: "Open".into(), action: ContextAction::Connect },
            ContextMenuItem { label: "Help".into(), action: ContextAction::Help },
        ];
        self.context_menu = Some(ContextMenu { x, y, items, selected: 0, area: ContextArea::Sidebar });
    }

    pub fn context_menu_up(&mut self) {
        if let Some(ref mut menu) = self.context_menu {
            if menu.selected > 0 { menu.selected -= 1; }
        }
    }

    pub fn context_menu_down(&mut self) {
        if let Some(ref mut menu) = self.context_menu {
            if menu.selected + 1 < menu.items.len() { menu.selected += 1; }
        }
    }

    pub fn context_menu_execute(&mut self) {
        let menu = match self.context_menu.take() {
            Some(m) => m,
            None => return,
        };
        let action = match menu.items.get(menu.selected) {
            Some(item) => item.action.clone(),
            None => return,
        };

        match action {
            ContextAction::CopySelection => {
                if let (Some(start), Some(end)) = (self.mouse_select_start, self.mouse_select_end) {
                    if let Some(session) = self.active_session() {
                        let text = session.copy_selection(start, end);
                        if !text.trim().is_empty() && copy_to_clipboard(&text) {
                            self.clipboard_msg = Some("Selection copied!".into());
                        }
                    }
                }
                self.mouse_select_start = None;
                self.mouse_select_end = None;
            }
            ContextAction::CopyLines(n) => {
                if let Some(session) = self.active_session() {
                    let text = session.copy_lines(n);
                    if copy_to_clipboard(&text) {
                        self.clipboard_msg = Some(format!("{} lines copied!", n));
                    }
                }
            }
            ContextAction::CopyIp => { self.copy_ip(); }
            ContextAction::Connect => { self.enter_nav(); }
            ContextAction::Disconnect => { self.close_active_tab(); }
            ContextAction::ServerDetail => { self.show_detail(); }
            ContextAction::Split => { self.toggle_split(); }
            ContextAction::Broadcast => { self.toggle_broadcast(); }
            ContextAction::Help => { self.show_help(); }
            ContextAction::Reconnect => { self.reconnect_active(); }
        }
    }

    pub fn close_context_menu(&mut self) {
        self.context_menu = None;
    }

    pub fn context_menu_click(&mut self, mx: u16, my: u16) -> bool {
        if let Some(ref menu) = self.context_menu {
            let menu_w = 22u16;
            let menu_h = menu.items.len() as u16 + 2; // +2 bordas
            let menu_x = menu.x;
            let menu_y = menu.y;

            if mx >= menu_x && mx < menu_x + menu_w && my >= menu_y && my < menu_y + menu_h {
                let idx = (my.saturating_sub(menu_y + 1)) as usize; // +1 pela borda top
                if idx < menu.items.len() {
                    // Clone the needed info before mutable borrow
                    let selected = idx;
                    if let Some(ref mut m) = self.context_menu {
                        m.selected = selected;
                    }
                    self.context_menu_execute();
                    return true;
                }
            }
            // Clique fora do menu — fecha
            self.context_menu = None;
            return true;
        }
        false
    }

    /// Scroll no sidebar
    pub fn mouse_scroll_sidebar(&mut self, up: bool) {
        if up {
            if self.sidebar_index > 0 {
                self.sidebar_index -= 1;
                self.sync_category_from_sidebar();
            }
        } else if self.sidebar_index + 1 < self.sidebar_items.len() {
            self.sidebar_index += 1;
            self.sync_category_from_sidebar();
        }
    }
}

// ── Internal helper for palette execution (avoids borrow conflicts) ──────────

enum PaletteActionData {
    Connect(Server),
    SwitchTab(usize),
    ToggleSplit,
    ToggleBroadcast,
    #[cfg(feature = "api")]
    Refresh,
    #[cfg(feature = "api")]
    Logout,
    CopyIp(String),
    RunCommand(String),
    ShowHelp,
    CloseTab,
    CopyLines(usize),
    Reconnect,
    ShowDetail,
}

// ── Clipboard helper ─────────────────────────────────────────────────────────

/// Base64 encode simples (sem crate extra)
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Tenta copiar via OSC 52 (escape sequence pro terminal pai)
/// Funciona em: Alacritty, Kitty, WezTerm, Windows Terminal, foot, etc.
fn try_osc52(text: &str) -> bool {
    use std::io::Write;
    let b64 = base64_encode(text.as_bytes());
    // ESC ] 52 ; c ; <base64> BEL
    let seq = format!("\x1b]52;c;{}\x07", b64);
    // Escreve direto no /dev/tty (terminal pai, não stdout que o ratatui controla)
    #[cfg(unix)]
    {
        if let Ok(mut tty) = std::fs::OpenOptions::new().write(true).open("/dev/tty") {
            return tty.write_all(seq.as_bytes()).is_ok();
        }
        return false;
    }
    #[cfg(windows)]
    {
        let _ = std::io::stderr().write_all(seq.as_bytes());
        return true;
    }
    #[cfg(not(any(unix, windows)))]
    { false }
}

/// Tenta copiar via comandos externos (xclip, xsel, wl-copy, etc)
fn try_external_clipboard(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[cfg(target_os = "linux")]
    {
        let commands: &[(&str, &[&str])] = &[
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
            ("wl-copy", &[]),
            ("clip.exe", &[]),
        ];
        for &(cmd, args) in commands {
            if let Ok(mut child) = Command::new(cmd)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                return child.wait().map(|s| s.success()).unwrap_or(false);
            }
        }
        false
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(mut child) = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
        {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            return child.wait().map(|s| s.success()).unwrap_or(false);
        }
        false
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(mut child) = Command::new("clip.exe")
            .stdin(Stdio::piped())
            .spawn()
        {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            return child.wait().map(|s| s.success()).unwrap_or(false);
        }
        false
    }
}

/// Copia pro clipboard — tenta OSC 52 primeiro, depois comandos externos
pub fn copy_to_clipboard(text: &str) -> bool {
    // 1. Tenta OSC 52 (funciona sem instalar nada em terminais modernos)
    //    Sempre tenta — se o terminal não suporta, ignora silenciosamente
    let osc52_sent = try_osc52(text);

    // 2. Tenta comandos externos como backup (xclip, xsel, wl-copy, etc)
    let external = try_external_clipboard(text);

    // Sucesso se qualquer um funcionou
    osc52_sent || external
}
