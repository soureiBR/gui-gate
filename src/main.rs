mod app;
#[cfg(feature = "api")]
mod api;
mod config;
mod filter;
mod terminal;
mod ui;
#[cfg(feature = "api")]
mod updater;

use std::borrow::Cow;
use std::io;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers, MouseEventKind, MouseButton},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::{App, AppMode, SplitLayout};
use config::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Panic hook para restaurar o terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        disable_raw_mode().ok();
        execute!(io::stderr(), crossterm::event::DisableMouseCapture, LeaveAlternateScreen).ok();
        original_hook(panic);
    }));

    // ── CLI args ──────────────────────────────────────────────────────
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--install") {
        return self_install();
    }

    if args.iter().any(|a| a == "--version" || a == "-v") {
        #[cfg(feature = "api")]
        eprintln!("gate v{}", updater::current_version());
        #[cfg(not(feature = "api"))]
        eprintln!("gate v{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    #[cfg(feature = "api")]
    if args.iter().any(|a| a == "--update") {
        return updater::run_update().map_err(|e| e.into());
    }

    // ── Check de update silencioso ──────────────────────────────────────
    #[cfg(feature = "api")]
    updater::check_update_quiet();

    #[allow(unused_mut)]
    let mut config = Config::load()?;
    #[cfg(feature = "api")]
    let mut _api_client: Option<api::ApiClient> = None;

    // ── First-run: se não tem config, pede a URL da API ──────────────────
    #[cfg(feature = "api")]
    if config.needs_setup() {
        // Checa se passou --url como argumento
        let url = std::env::args()
            .skip(1)
            .find(|a| a.starts_with("http"))
            .or_else(|| {
                // Pergunta interativamente
                eprintln!("╔══════════════════════════════════════╗");
                eprintln!("║    SoureiGate — First Setup           ║");
                eprintln!("╚══════════════════════════════════════╝");
                eprintln!();
                eprintln!("Enter Gate API URL:");
                eprint!("> ");
                let mut input = String::new();
                io::stdin().read_line(&mut input).ok()?;
                let url = input.trim().to_string();
                if url.is_empty() { None } else { Some(url) }
            });

        match url {
            Some(u) => {
                config.setup_api(&u);
                config.save_to_config_dir()?;
                eprintln!("✓ Config saved to ~/.config/soureigate/servers.toml\n");
            }
            None => {
                eprintln!("Usage: gate <API_URL>");
                eprintln!("  Ex: gate https://gate.sourei.dev.br");
                std::process::exit(1);
            }
        }
    }

    // ── Modo API: auth via passkey + fetch dados ──────────────────────────
    #[cfg(feature = "api")]
    if config.is_api_mode() {
        let api_url = config.api_url().unwrap().to_string();
        eprintln!("╔══════════════════════════════════════╗");
        eprintln!("║       SoureiGate — Login              ║");
        eprintln!("╚══════════════════════════════════════╝");
        eprintln!();

        let client = api::ApiClient::login(&api_url)?;

        eprintln!("Loading servers...");
        let categories = client.fetch_categories()?;
        let total: usize = categories.iter().map(|c| c.servers.len()).sum();
        eprintln!("✓ {} categorias, {} servidores carregados\n", categories.len(), total);

        // Baixa a SSH key do admin logado via API
        eprintln!("Downloading SSH key...");
        let ssh_key = match client.fetch_and_save_ssh_key() {
            Ok(path) => {
                eprintln!("✓ SSH key configured\n");
                path.to_string_lossy().into_owned()
            }
            Err(e) => {
                eprintln!("⚠ Could not download SSH key: {}", e);
                eprintln!("  SSH connections may fail without key.\n");
                String::new()
            }
        };

        config = Config::from_api(categories, ssh_key);
        // Guarda o client pra uso durante a sessão (será movido pro App)
        _api_client = Some(client);

        // Aguarda Enter antes de abrir TUI — garante foco na janela do terminal
        eprintln!("Press Enter to open SoureiGate...");
        let mut _buf = String::new();
        io::stdin().read_line(&mut _buf).ok();
    }

    // ── Modo TOML: validação da chave SSH (só quando NÃO veio da API) ────
    if !config.loaded_from_api {
        let key_path = &config.settings.ssh_key;
        if !key_path.is_empty() && !std::path::Path::new(key_path).exists() {
            eprintln!("SSH key not found: {}", key_path);
            eprintln!("Edit servers.toml and adjust ssh_key");
            std::process::exit(1);
        }
    }

    // ── TUI ───────────────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config);

    #[cfg(feature = "api")]
    if let Some(client) = _api_client.take() {
        app.set_api_client(client);
    }

    let mut last_size = terminal.size()?;
    let mut last_click: Option<(u16, u16, std::time::Instant)> = None;
    #[cfg(feature = "api")]
    let mut last_refresh = std::time::Instant::now();
    #[cfg(feature = "api")]
    let mut last_gate_check = std::time::Instant::now();

    loop {
        // Auto-reconnect: detect dead session and store reconnect info
        if app.mode == AppMode::Terminal {
            app.check_dead_sessions();
        }

        // Auto-refresh in background (every 5 minutes, only in Browse mode with API)
        #[cfg(feature = "api")]
        if app.is_api_mode() {
            // Gate status ping every 60 seconds
            if last_gate_check.elapsed() > std::time::Duration::from_secs(60) {
                app.check_gate_status();
                last_gate_check = std::time::Instant::now();
            }
            // Full refresh every 5 minutes (non-terminal mode)
            if app.mode != AppMode::Terminal && last_refresh.elapsed() > std::time::Duration::from_secs(300) {
                app.check_gate_status();
                let _ = app.refresh_from_api();
                last_refresh = std::time::Instant::now();
                last_gate_check = std::time::Instant::now();
            }
        }
        terminal.draw(|f| ui::draw(f, &mut app))?;

        // Ajusta terminal embutido ao tamanho atual
        let size = terminal.size()?;
        if size != last_size {
            last_size = size;
            if app.mode == AppMode::Terminal {
                let sidebar_w = ((size.width as f32 * 0.22).max(20.0).min(30.0) as u16) + 2;
                let term_cols = size.width.saturating_sub(sidebar_w + 2);
                let term_rows = size.height.saturating_sub(4);

                if let Some(ref split) = app.split {
                    let (pane_w, pane_h) = match split.layout {
                        SplitLayout::Vertical2 => (term_cols / 2 - 1, term_rows - 2),
                        SplitLayout::Horizontal2 => (term_cols - 2, term_rows / 2 - 1),
                        SplitLayout::Triple => (term_cols / 2 - 1, term_rows / 2 - 1),
                        SplitLayout::Quad => (term_cols / 2 - 1, term_rows / 2 - 1),
                    };
                    let pane_indices: Vec<usize> = split.panes.clone();
                    for &tab_idx in &pane_indices {
                        if let Some(session) = app.tabs.get_mut(tab_idx) {
                            session.resize(pane_w, pane_h);
                        }
                    }
                } else {
                    app.resize_active_terminal(term_cols, term_rows);
                }
            }
        }

        if event::poll(std::time::Duration::from_millis(16))? {
            let ev = event::read()?;

            // ── Mouse events ──────────────────────────────────────────
            if let Event::Mouse(mouse) = ev {
                let (mx, my) = (mouse.column, mouse.row);
                let (sx, sy, sw, sh) = app.mouse_sidebar_area;
                let (lx, ly, lw, lh) = app.mouse_serverlist_area;

                let in_sidebar = mx >= sx && mx < sx + sw && my >= sy && my < sy + sh;
                let in_serverlist = mx >= lx && mx < lx + lw && my >= ly && my < ly + lh;

                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        // Se tem context menu aberto, trata clique nele
                        if app.context_menu.is_some() {
                            app.context_menu_click(mx, my);
                            continue;
                        }

                        // Limpa seleção de texto ao clicar (se tinha)
                        if app.mouse_select_start.is_some() {
                            app.mouse_select_start = None;
                            app.mouse_select_end = None;
                        }

                        // Detecta duplo clique (< 400ms, mesma posição)
                        let is_dblclick = last_click
                            .map(|(lx, ly, t)| {
                                lx == mx && ly == my
                                    && t.elapsed() < std::time::Duration::from_millis(400)
                            })
                            .unwrap_or(false);

                        if is_dblclick {
                            last_click = None;
                            // Duplo clique: abre/entra
                            if in_sidebar {
                                app.mouse_click_sidebar(my);
                                app.enter_nav();
                                if app.mode == AppMode::Terminal {
                                    terminal.clear()?;
                                }
                            } else if in_serverlist && app.mode == AppMode::Browse {
                                app.mouse_click_serverlist(my);
                                app.enter_nav();
                                if app.mode == AppMode::Terminal {
                                    terminal.clear()?;
                                }
                            }
                        } else {
                            last_click = Some((mx, my, std::time::Instant::now()));
                            // Clique simples
                            if in_sidebar {
                                app.mouse_click_sidebar(my);
                            } else if in_serverlist && app.mode == AppMode::Browse {
                                app.mouse_click_serverlist(my);
                            } else if app.mode == AppMode::Terminal {
                                // Clique na tab bar
                                let in_tab_bar = app.mouse_tab_bar
                                    .as_ref()
                                    .map(|(ty, _, _)| my == *ty)
                                    .unwrap_or(false);

                                if in_tab_bar {
                                    app.mouse_click_tab(mx);
                                } else if app.is_split() {
                                    app.mouse_click_split_pane(mx, my);
                                }
                            }
                        }
                    }
                    MouseEventKind::ScrollUp => {
                        if app.mode == AppMode::Terminal && !in_sidebar {
                            // Scroll up no terminal
                            if let Some(session) = app.active_session_mut() {
                                session.scroll_up(3);
                            }
                        } else if in_sidebar {
                            app.mouse_scroll_sidebar(true);
                        } else if in_serverlist {
                            app.mouse_scroll_serverlist(true);
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        if app.mode == AppMode::Terminal && !in_sidebar {
                            // Scroll down no terminal
                            if let Some(session) = app.active_session_mut() {
                                session.scroll_down(3);
                            }
                        } else if in_sidebar {
                            app.mouse_scroll_sidebar(false);
                        } else if in_serverlist {
                            app.mouse_scroll_serverlist(false);
                        }
                    }
                    MouseEventKind::Down(MouseButton::Right) => {
                        // Context menu: clique direito
                        // Se já tem menu aberto, tenta clicar nele primeiro
                        if app.context_menu_click(mx, my) {
                            continue;
                        }

                        if app.mode == AppMode::Terminal && !in_sidebar {
                            app.open_context_menu_terminal(mx, my);
                        } else if in_serverlist && app.mode == AppMode::Browse {
                            app.mouse_click_serverlist(my); // seleciona o item
                            app.open_context_menu_serverlist(mx, my);
                        } else if in_sidebar {
                            app.mouse_click_sidebar(my);
                            app.open_context_menu_sidebar(mx, my);
                        }
                    }
                    MouseEventKind::Drag(MouseButton::Left) => {
                        // Mouse drag: seleção de texto no terminal
                        if app.mode == AppMode::Terminal && !in_sidebar {
                            let (lx, ly, _, _) = app.mouse_serverlist_area;
                            let rel_col = mx.saturating_sub(lx + 1); // +1 pela borda
                            let rel_row = my.saturating_sub(ly + 1);

                            if !app.mouse_selecting {
                                app.mouse_selecting = true;
                                app.mouse_select_start = Some((rel_col, rel_row));
                            }
                            app.mouse_select_end = Some((rel_col, rel_row));
                        }
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        // Mouse up: finaliza drag mas MANTÉM seleção visível
                        if app.mouse_selecting {
                            app.mouse_selecting = false;
                            // Seleção persiste — não limpa start/end
                            // Será limpa no próximo clique simples
                        }
                    }
                    _ => {}
                }
                continue;
            }

            if let Event::Key(key) = ev {
                // Context menu: intercepta TODAS as teclas enquanto aberto
                if app.context_menu.is_some() {
                    match key.code {
                        KeyCode::Esc => app.close_context_menu(),
                        KeyCode::Up | KeyCode::Char('k') => app.context_menu_up(),
                        KeyCode::Down | KeyCode::Char('j') => app.context_menu_down(),
                        KeyCode::Enter => app.context_menu_execute(),
                        _ => app.close_context_menu(),
                    }
                    continue; // SEMPRE continua — nunca cai no match de modo
                }

                match app.mode {
                    AppMode::Terminal => {
                        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                        let alt = key.modifiers.contains(KeyModifiers::ALT);

                        // Limpa mensagem de clipboard em qualquer tecla
                        app.clear_clipboard_msg();

                        // If active session is dead, handle reconnect/dismiss
                        let session_dead = app.active_session().map(|s| s.is_dead()).unwrap_or(false);
                        if session_dead {
                            match key.code {
                                KeyCode::Enter => {
                                    app.reconnect_active();
                                    terminal.clear()?;
                                    continue;
                                }
                                KeyCode::Esc => {
                                    app.dismiss_dead();
                                    terminal.clear()?;
                                    continue;
                                }
                                _ => { continue; }
                            }
                        }

                        // F1 = Help
                        if key.code == KeyCode::F(1) {
                            app.show_help();
                            continue;
                        }

                        // Scroll: Shift+PageUp/Down OU Shift+Up/Down
                        if shift {
                            match key.code {
                                KeyCode::PageUp | KeyCode::Up => {
                                    if let Some(session) = app.active_session_mut() {
                                        session.scroll_up(if key.code == KeyCode::PageUp { 10 } else { 3 });
                                    }
                                    continue;
                                }
                                KeyCode::PageDown | KeyCode::Down => {
                                    if let Some(session) = app.active_session_mut() {
                                        session.scroll_down(if key.code == KeyCode::PageDown { 10 } else { 3 });
                                    }
                                    continue;
                                }
                                _ => {}
                            }
                        }

                        // F-keys pra split (não conflitam com SSH)
                        match key.code {
                            KeyCode::F(2) => {
                                app.toggle_split();
                                if let Some(ref split) = app.split {
                                    let sz = terminal.size()?;
                                    let sidebar_w = ((sz.width as f32 * 0.22).max(20.0).min(30.0) as u16) + 2;
                                    let term_cols = sz.width.saturating_sub(sidebar_w + 2);
                                    let term_rows = sz.height.saturating_sub(4);
                                    let (pane_w, pane_h) = match split.layout {
                                        SplitLayout::Vertical2 => (term_cols / 2 - 1, term_rows - 2),
                                        SplitLayout::Horizontal2 => (term_cols - 2, term_rows / 2 - 1),
                                        SplitLayout::Triple => (term_cols / 2 - 1, term_rows / 2 - 1),
                                        SplitLayout::Quad => (term_cols / 2 - 1, term_rows / 2 - 1),
                                    };
                                    let pane_indices: Vec<usize> = split.panes.clone();
                                    for &tab_idx in &pane_indices {
                                        if let Some(session) = app.tabs.get_mut(tab_idx) {
                                            session.resize(pane_w, pane_h);
                                        }
                                    }
                                }
                                terminal.clear()?;
                                continue;
                            }
                            KeyCode::F(3) => {
                                if app.is_split() {
                                    // Foco próximo painel (cicla)
                                    if let Some(ref mut s) = app.split {
                                        s.focused_pane = (s.focused_pane + 1) % s.panes.len();
                                        app.active_tab = Some(s.panes[s.focused_pane]);
                                    }
                                }
                                continue;
                            }
                            KeyCode::F(4) => {
                                if app.is_split() {
                                    if let Some(ref mut s) = app.split {
                                        s.focused_pane = if s.focused_pane == 0 { s.panes.len() - 1 } else { s.focused_pane - 1 };
                                        app.active_tab = Some(s.panes[s.focused_pane]);
                                    }
                                }
                                continue;
                            }
                            KeyCode::F(5) => {
                                app.toggle_broadcast();
                                continue;
                            }
                            KeyCode::F(n) if n >= 6 && n <= 12 => {
                                // F9/F10 scroll if no command mapped
                                let key_name = format!("F{}", n);
                                if let Some(snippet) = app.config.commands.iter().find(|c| c.key == key_name).cloned() {
                                    if App::is_dangerous_command(&snippet.command) {
                                        app.danger_command = Some(snippet.command);
                                        app.danger_broadcast = app.broadcast;
                                        app.mode = AppMode::ConfirmDanger;
                                    } else {
                                        let cmd = format!("{}\n", snippet.command);
                                        let bytes = cmd.into_bytes();
                                        if app.broadcast {
                                            app.write_input_all(&bytes);
                                        } else if let Some(session) = app.active_session_mut() {
                                            session.write_input(bytes);
                                        }
                                    }
                                    continue;
                                }
                                // Default scroll for F9/F10 if not mapped
                                match n {
                                    9 => {
                                        if let Some(session) = app.active_session_mut() {
                                            session.scroll_up(10);
                                        }
                                    }
                                    10 => {
                                        if let Some(session) = app.active_session_mut() {
                                            session.scroll_down(10);
                                        }
                                    }
                                    12 => {
                                        if let Some(session) = app.active_session() {
                                            let text = session.copy_lines(50);
                                            if app::copy_to_clipboard(&text) {
                                                app.clipboard_msg = Some("50 lines copied!".into());
                                            } else {
                                                app.clipboard_msg = Some("Failed: install xclip (sudo apt install xclip)".into());
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                                continue;
                            }
                            _ => {}
                        }

                        if ctrl {
                            match key.code {
                                KeyCode::Char('b') => {
                                    app.switch_to_browse();
                                    continue;
                                }
                                KeyCode::Char('p') => {
                                    app.open_palette();
                                    continue;
                                }
                                KeyCode::Char('x') => {
                                    app.open_command_input();
                                    continue;
                                }
                                KeyCode::Char('w') => {
                                    app.close_active_tab();
                                    terminal.clear()?;
                                    continue;
                                }
                                KeyCode::Tab => {
                                    app.next_tab();
                                    continue;
                                }
                                _ if key.code == KeyCode::BackTab => {
                                    app.prev_tab();
                                    continue;
                                }
                                _ => {}
                            }
                        }

                        let bytes: Option<Cow<'static, [u8]>> =
                            key_to_bytes(key.code, ctrl, shift, alt);

                        if let Some(b) = bytes {
                            // Intercepta Enter — checa comando perigoso na linha atual
                            if key.code == KeyCode::Enter {
                                let current_cmd = app.active_session()
                                    .map(|s| s.current_line())
                                    .unwrap_or_default();
                                if !current_cmd.is_empty() && App::is_dangerous_command(&current_cmd) {
                                    app.danger_command = Some(current_cmd);
                                    app.danger_broadcast = app.broadcast;
                                    app.mode = AppMode::ConfirmDanger;
                                    continue;
                                }
                            }

                            if app.broadcast {
                                app.write_input_all(&b);
                                for session in &mut app.tabs {
                                    session.scroll_reset();
                                }
                            } else if let Some(session) = app.active_session_mut() {
                                session.write_input(b);
                                session.scroll_reset();
                            }
                        }
                    }

                    AppMode::ConfirmDanger => match key.code {
                        KeyCode::Char('y') | KeyCode::Enter => { app.confirm_danger(); continue; }
                        KeyCode::Char('n') | KeyCode::Esc => { app.cancel_danger(); continue; }
                        _ => { continue; }
                    },

                    AppMode::CommandInput => match key.code {
                        KeyCode::Esc => app.cancel_command_input(),
                        KeyCode::Enter => app.execute_command_input(),
                        KeyCode::Backspace => app.command_input_backspace(),
                        KeyCode::Char(c) => app.command_input_push(c),
                        _ => {}
                    },

                    AppMode::Help => match key.code {
                        KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('q') => app.close_help(),
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.help_scroll = app.help_scroll.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            app.help_scroll = app.help_scroll.saturating_add(1);
                        }
                        KeyCode::PageUp => {
                            app.help_scroll = app.help_scroll.saturating_sub(10);
                        }
                        KeyCode::PageDown => {
                            app.help_scroll = app.help_scroll.saturating_add(10);
                        }
                        _ => {}
                    },

                    AppMode::Search => match key.code {
                        KeyCode::F(1) => app.show_help(),
                        KeyCode::Esc => app.go_back_nav(),
                        KeyCode::Enter => {
                            if app.is_global_search && !app.global_results.is_empty() {
                                app.global_connect();
                                terminal.clear()?;
                            } else {
                                app.confirm_search();
                            }
                        }
                        KeyCode::Backspace => app.search_backspace(),
                        KeyCode::Up => app.global_move_up(),
                        KeyCode::Down => app.global_move_down(),
                        KeyCode::Char(c) => app.search_push(c),
                        _ => {}
                    },

                    AppMode::Detail => match key.code {
                        KeyCode::Esc | KeyCode::Char('i') => app.close_detail(),
                        _ => {}
                    },

                    AppMode::Palette => match key.code {
                        KeyCode::Esc => app.close_palette(),
                        KeyCode::Enter => {
                            app.palette_execute();
                            if app.mode == AppMode::Terminal {
                                terminal.clear()?;
                            }
                        }
                        KeyCode::Up => app.palette_move_up(),
                        KeyCode::Down => app.palette_move_down(),
                        KeyCode::Backspace => app.palette_backspace(),
                        KeyCode::Char(c) => app.palette_push(c),
                        _ => {}
                    },

                    AppMode::Browse => {
                        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

                        // Clear clipboard message on any keypress
                        app.clear_clipboard_msg();

                        // Ctrl+ combos primeiro (antes do match geral)
                        if ctrl {
                            match key.code {
                                KeyCode::Char('c') => break,
                                KeyCode::Char('p') => { app.open_palette(); continue; }
                                KeyCode::Char('t') => { app.switch_to_terminal(); continue; }
                                #[cfg(feature = "api")]
                                KeyCode::Char('l') => { app.logout(); break; }
                                _ => {}
                            }
                        }

                        match key.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Char('j') | KeyCode::Down => app.move_down(),
                            KeyCode::Char('k') | KeyCode::Up => app.move_up(),
                            KeyCode::Char('g') => app.jump_top(),
                            KeyCode::Char('G') => app.jump_bottom(),
                            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                                let prev_mode = app.mode;
                                app.enter_nav();
                                if prev_mode != app.mode {
                                    terminal.clear()?;
                                }
                            }
                            KeyCode::Char('h') | KeyCode::Left | KeyCode::Esc => {
                                app.go_back_nav()
                            }
                            KeyCode::Char(' ') => app.toggle_select(),
                            KeyCode::Char('i') => app.show_detail(),
                            KeyCode::Char('c') => app.copy_ip(),
                            KeyCode::Tab => app.toggle_sidebar_focus(),
                            KeyCode::F(1) => app.show_help(),
                            #[cfg(feature = "api")]
                            KeyCode::F(5) => {
                                if let Err(e) = app.refresh_from_api() {
                                    eprintln!("Refresh failed: {}", e);
                                }
                                last_refresh = std::time::Instant::now();
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if !app.running {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), crossterm::event::DisableMouseCapture, LeaveAlternateScreen)?;
    Ok(())
}

/// Converte um crossterm KeyCode em bytes ANSI para o PTY
fn key_to_bytes(
    code: KeyCode,
    ctrl: bool,
    shift: bool,
    alt: bool,
) -> Option<Cow<'static, [u8]>> {
    let bytes: &'static [u8] = match code {
        KeyCode::Enter => b"\r",
        KeyCode::Backspace => b"\x7f",
        KeyCode::Delete => b"\x1b[3~",
        KeyCode::Tab => {
            if shift { b"\x1b[Z" } else { b"\t" }
        }
        KeyCode::Esc => b"\x1b",
        KeyCode::Up => b"\x1b[A",
        KeyCode::Down => b"\x1b[B",
        KeyCode::Right => b"\x1b[C",
        KeyCode::Left => b"\x1b[D",
        KeyCode::Home => b"\x1b[H",
        KeyCode::End => b"\x1b[F",
        KeyCode::PageUp => b"\x1b[5~",
        KeyCode::PageDown => b"\x1b[6~",
        KeyCode::F(1) => b"\x1bOP",
        KeyCode::F(2) => b"\x1bOQ",
        KeyCode::F(3) => b"\x1bOR",
        KeyCode::F(4) => b"\x1bOS",
        KeyCode::F(5) => b"\x1b[15~",
        KeyCode::F(6) => b"\x1b[17~",
        KeyCode::F(7) => b"\x1b[18~",
        KeyCode::F(8) => b"\x1b[19~",
        KeyCode::F(9) => b"\x1b[20~",
        KeyCode::F(10) => b"\x1b[21~",
        KeyCode::F(11) => b"\x1b[23~",
        KeyCode::F(12) => b"\x1b[24~",
        KeyCode::Char(c) => {
            if ctrl {
                if c.is_ascii_alphabetic() {
                    let b = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a').wrapping_add(1);
                    return Some(Cow::Owned(vec![b]));
                }
                if c == '[' {
                    return Some(Cow::Borrowed(b"\x1b"));
                }
            }
            if alt {
                let mut bytes = vec![0x1b];
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                bytes.extend_from_slice(s.as_bytes());
                return Some(Cow::Owned(bytes));
            }
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            return Some(Cow::Owned(s.as_bytes().to_vec()));
        }
        _ => return None,
    };

    Some(Cow::Borrowed(bytes))
}

/// Auto-instala o binário no PATH do sistema
fn self_install() -> Result<(), Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;

    #[cfg(unix)]
    {
        let home = dirs::home_dir().ok_or("HOME not found")?;
        let install_dir = home.join(".local").join("bin");
        std::fs::create_dir_all(&install_dir)?;

        let dest = install_dir.join("gate");
        if exe != dest {
            std::fs::copy(&exe, &dest)?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
        }

        eprintln!("✓ Instalado em {}", dest.display());

        // Verifica se ~/.local/bin já está no PATH
        let path = std::env::var("PATH").unwrap_or_default();
        if !path.contains(".local/bin") {
            let export_line = "export PATH=\"$HOME/.local/bin:$PATH\"";

            // Detecta shell rc file
            let rc_files = [
                home.join(".bashrc"),
                home.join(".zshrc"),
                home.join(".profile"),
            ];

            let mut added = false;
            for rc in &rc_files {
                if rc.exists() {
                    let content = std::fs::read_to_string(rc).unwrap_or_default();
                    if !content.contains(".local/bin") {
                        let addition = format!("\n# SoureiGate\n{}\n", export_line);
                        std::fs::OpenOptions::new()
                            .append(true)
                            .open(rc)
                            .and_then(|mut f| {
                                use std::io::Write;
                                f.write_all(addition.as_bytes())
                            })
                            .ok();
                        eprintln!("✓ PATH added to {}", rc.display());
                        added = true;
                        break;
                    }
                }
            }

            if !added {
                eprintln!();
                eprintln!("Add to PATH manually:");
                eprintln!("  {}", export_line);
            }
        } else {
            eprintln!("✓ PATH already configured");
        }
    }

    #[cfg(windows)]
    {
        let install_dir = dirs::data_local_dir()
            .ok_or("LOCALAPPDATA not found")?
            .join("SoureiGate");
        std::fs::create_dir_all(&install_dir)?;

        let dest = install_dir.join("gate.exe");
        if exe != dest {
            std::fs::copy(&exe, &dest)?;
        }

        eprintln!("✓ Instalado em {}", dest.display());

        // Adiciona ao PATH do usuário (permanente via registro)
        let path_str = install_dir.to_string_lossy().to_string();
        let current_path = std::env::var("PATH").unwrap_or_default();
        if !current_path.contains(&path_str) {
            let output = std::process::Command::new("powershell")
                .args([
                    "-Command",
                    &format!(
                        "$p = [Environment]::GetEnvironmentVariable('Path','User'); \
                         if ($p -notlike '*{}*') {{ \
                             [Environment]::SetEnvironmentVariable('Path', \"$p;{}\", 'User'); \
                             Write-Host 'PATH atualizado' \
                         }} else {{ Write-Host 'PATH ja configurado' }}",
                        path_str.replace('\\', "\\\\"),
                        path_str.replace('\\', "\\\\"),
                    ),
                ])
                .output();

            match output {
                Ok(o) => eprintln!("{}", String::from_utf8_lossy(&o.stdout).trim()),
                Err(e) => eprintln!("⚠ Could not update PATH: {}", e),
            }
        }
    }

    eprintln!();
    eprintln!("Close and reopen the terminal, then type: gate");
    Ok(())
}
