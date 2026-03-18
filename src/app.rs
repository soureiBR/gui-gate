#[cfg(feature = "api")]
use crate::api::ApiClient;
use crate::config::{Config, Server};
use crate::filter::{GlobalMatch, filter_servers, global_search};
use crate::terminal::TerminalSession;

#[derive(PartialEq, Clone, Copy)]
pub enum SplitLayout {
    Vertical2,   // left | right
    Horizontal2, // top / bottom
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

    // Mouse: áreas clicáveis (atualizadas a cada draw)
    pub mouse_sidebar_area: (u16, u16, u16, u16),   // x, y, w, h
    pub mouse_serverlist_area: (u16, u16, u16, u16),
    pub mouse_sidebar_offset_y: u16, // y do primeiro item do sidebar
    pub mouse_tab_bar: Option<(u16, u16, Vec<(u16, u16)>)>, // (y, x_start, [(x_start, x_end) per tab])

    // Split terminal
    pub split: Option<SplitState>,

    // API client (JWT vive aqui na RAM)
    #[cfg(feature = "api")]
    pub api_client: Option<ApiClient>,
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
            split: None,
            #[cfg(feature = "api")]
            api_client: None,
        };
        app.rebuild_sidebar();
        app
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
                if let Some(&real_index) = self.filtered_indices.get(self.server_index) {
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
                self.clipboard_msg = Some(format!("IP copiado: {}", ip));
            } else {
                self.clipboard_msg = Some("Falha ao copiar IP".to_string());
            }
        }
    }

    pub fn clear_clipboard_msg(&mut self) {
        self.clipboard_msg = None;
    }

    // ─── Search ─────────────────────────────────────────────────────

    pub fn enter_search(&mut self) {
        self.mode = AppMode::Search;
        self.search_query.clear();
        self.is_global_search = true;
        self.global_results.clear();
        self.global_index = 0;
        self.refilter();
    }

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
                eprintln!("Erro ao abrir terminal: {e}");
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

// ── Clipboard helper ─────────────────────────────────────────────────────────

fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[cfg(target_os = "linux")]
    {
        // Try xclip, then xsel, then wl-copy, then clip.exe (WSL)
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
