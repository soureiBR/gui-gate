use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, List, ListItem, Paragraph, Row, Table, Widget,
    },
};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line as TermLine};
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};

use crate::app::{App, AppMode, SidebarFocus, SidebarItem, SplitLayout, PaletteItem};
use crate::terminal::TerminalSession;

// ── Paleta ────────────────────────────────────────────────────────────────────

// ── Paleta — dark theme inspirado no Catppuccin Mocha ─────────────────────────

const ACTIVE_BORDER: Color = Color::Rgb(137, 180, 250);  // Lavender
const INACTIVE_BORDER: Color = Color::Rgb(69, 71, 90);   // Surface1
const HIGHLIGHT_BG: Color = Color::Rgb(49, 50, 68);      // Surface0
const HIGHLIGHT_FG: Color = Color::Rgb(205, 214, 244);   // Text
const HEADER_BG: Color = Color::Rgb(17, 17, 27);         // Crust
const STATUS_BG: Color = Color::Rgb(30, 30, 46);         // Base
const TAB_ACTIVE_BG: Color = Color::Rgb(49, 50, 68);     // Surface0
const TAB_ACTIVE_FG: Color = Color::Rgb(137, 180, 250);  // Lavender
const TAB_BG: Color = Color::Rgb(24, 24, 37);            // Mantle
const TAB_FG: Color = Color::Rgb(88, 91, 112);           // Overlay0

const ACCENT: Color = Color::Rgb(137, 180, 250);         // Lavender
const TEXT: Color = Color::Rgb(205, 214, 244);            // Text
const SUBTEXT: Color = Color::Rgb(166, 173, 200);        // Subtext0
const DIMMED: Color = Color::Rgb(108, 112, 134);         // Overlay1
#[allow(dead_code)]
const SURFACE: Color = Color::Rgb(49, 50, 68);           // Surface0
const GREEN: Color = Color::Rgb(166, 227, 161);          // Green
const YELLOW: Color = Color::Rgb(249, 226, 175);         // Yellow
const RED: Color = Color::Rgb(255, 69, 58);              // Red (intenso)
const PEACH: Color = Color::Rgb(250, 179, 135);          // Peach
const MAUVE: Color = Color::Rgb(203, 166, 247);          // Mauve
const TEAL: Color = Color::Rgb(148, 226, 213);           // Teal

// ── Helpers ───────────────────────────────────────────────────────────────────

fn row_style(index: usize, selected: usize, is_focused: bool) -> Style {
    if index == selected && is_focused {
        Style::default()
            .fg(HIGHLIGHT_FG)
            .bg(HIGHLIGHT_BG)
            .add_modifier(Modifier::BOLD)
    } else if index == selected {
        Style::default().fg(TEXT).bg(Color::Rgb(40, 40, 55))
    } else {
        Style::default().fg(SUBTEXT)
    }
}

fn status_indicator(status: &str) -> (&'static str, Color) {
    match status.to_lowercase().as_str() {
        "deployed" | "online" | "running" | "active" => ("●", GREEN),
        "pending" | "deploying" | "provisioning" => ("◐", YELLOW),
        "error" | "failed" | "offline" | "stopped" => ("●", RED),
        "" => ("·", DIMMED),
        _ => ("○", DIMMED),
    }
}

/// Monta string compacta de serviços com indicadores
fn services_status(server: &crate::config::Server) -> String {
    let wg = svc_char(&server.wg_status);
    let zbx = svc_char(&server.zabbix_status);
    let fb = svc_char(&server.fluentbit_status);
    format!("WG{} ZB{} FB{}", wg, zbx, fb)
}

fn svc_char(status: &str) -> &'static str {
    match status.to_lowercase().as_str() {
        "deployed" | "online" | "active" | "installed" | "registered" => "✓",
        "pending" | "deploying" | "provisioning" => "~",
        "error" | "failed" | "offline" => "✗",
        "unknown" => "?",
        "" => "-",
        _ => "?",
    }
}

// ── Terminal default colors (16 ANSI colors) ──────────────────────────────────

/// Preto puro (ANSI Black = cinza na maioria dos temas, então usamos RGB)
const TRUE_BLACK: Color = Color::Rgb(0, 0, 0);

fn ansi_to_ratatui(color: AnsiColor, is_fg: bool) -> Color {
    match color {
        AnsiColor::Named(n) => match n {
            NamedColor::Black | NamedColor::DimBlack => TRUE_BLACK,
            NamedColor::Red | NamedColor::DimRed => Color::Red,
            NamedColor::Green | NamedColor::DimGreen => Color::Green,
            NamedColor::Yellow | NamedColor::DimYellow => Color::Yellow,
            NamedColor::Blue | NamedColor::DimBlue => Color::Blue,
            NamedColor::Magenta | NamedColor::DimMagenta => Color::Magenta,
            NamedColor::Cyan | NamedColor::DimCyan => Color::Cyan,
            NamedColor::White | NamedColor::DimWhite => Color::White,
            NamedColor::BrightBlack => Color::DarkGray,
            NamedColor::BrightRed => Color::LightRed,
            NamedColor::BrightGreen => Color::LightGreen,
            NamedColor::BrightYellow => Color::LightYellow,
            NamedColor::BrightBlue => Color::LightBlue,
            NamedColor::BrightMagenta => Color::LightMagenta,
            NamedColor::BrightCyan => Color::LightCyan,
            NamedColor::BrightWhite | NamedColor::BrightForeground => Color::White,
            NamedColor::Foreground | NamedColor::DimForeground => {
                if is_fg { Color::White } else { TRUE_BLACK }
            }
            NamedColor::Background => {
                if is_fg { Color::White } else { TRUE_BLACK }
            }
            NamedColor::Cursor => Color::White,
        },
        AnsiColor::Spec(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
        AnsiColor::Indexed(i) => Color::Indexed(i),
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // Pinta TODO o frame com a cor base do tema
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(17, 17, 27))),
        area,
    );

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Min(5),    // body
            Constraint::Length(1), // status bar
        ])
        .split(area);

    draw_titlebar(frame, outer[0], app);
    draw_body(frame, outer[1], app);
    draw_statusbar(frame, outer[2], app);

    // Detail popup overlay
    if app.mode == AppMode::Detail {
        if let Some(ref server) = app.detail_server {
            draw_detail_popup(frame, area, server);
        }
    }

    // Palette popup overlay
    if app.mode == AppMode::Palette {
        draw_palette(frame, area, app);
    }

    // Help popup overlay
    if app.mode == AppMode::Help {
        draw_help_popup(frame, area, app);
    }
}

// ── Title bar ─────────────────────────────────────────────────────────────────

fn draw_titlebar(frame: &mut Frame, area: Rect, app: &App) {
    let cat_name = app
        .config
        .categories
        .get(app.category_index)
        .map(|c| c.name.as_str())
        .unwrap_or("");

    let server_count: usize = app.config.categories.iter().map(|c| c.servers.len()).sum();

    let title = Line::from(vec![
        Span::styled(" ◆ ", Style::default().fg(MAUVE).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("SoureiGate v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(DIMMED)),
        Span::styled(cat_name, Style::default().fg(PEACH)),
        Span::styled(
            format!("  {} servidores", server_count),
            Style::default().fg(DIMMED),
        ),
    ]);

    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(HEADER_BG)),
        area,
    );
}

// ── Body ──────────────────────────────────────────────────────────────────────

fn draw_body(frame: &mut Frame, area: Rect, app: &mut App) {
    let sidebar_w = (area.width as f32 * 0.22).max(20.0).min(30.0) as u16;

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(sidebar_w), Constraint::Min(30)])
        .split(area);

    // Salva áreas pra detecção de clique do mouse
    let sb = chunks[0];
    app.mouse_sidebar_area = (sb.x, sb.y, sb.width, sb.height);
    app.mouse_sidebar_offset_y = sb.y + 1; // +1 pela borda do block
    let sl = chunks[1];
    app.mouse_serverlist_area = (sl.x, sl.y, sl.width, sl.height);

    draw_sidebar(frame, chunks[0], app);

    match app.mode {
        AppMode::Terminal => {
            let has_split = app.split.is_some();
            let active_idx = app.active_tab;

            if has_split {
                draw_split_terminal_by_ref(frame, chunks[1], app);
            } else if let Some(idx) = active_idx {
                if idx < app.tabs.len() {
                    draw_terminal_panel_by_idx(frame, chunks[1], app, idx);
                } else {
                    draw_server_list(frame, chunks[1], app);
                }
            } else {
                draw_server_list(frame, chunks[1], app);
            }
        }
        AppMode::Search if app.is_global_search => {
            draw_global_search(frame, chunks[1], app);
        }
        _ => draw_server_list(frame, chunks[1], app),
    }
}

// ── Sidebar ───────────────────────────────────────────────────────────────────

fn draw_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let border_color = if (app.mode == AppMode::Browse || app.mode == AppMode::Detail || app.mode == AppMode::Palette)
        && app.sidebar_focus == SidebarFocus::Sidebar
    {
        ACTIVE_BORDER
    } else {
        INACTIVE_BORDER
    };

    let block = Block::default()
        .title(Span::styled(" Navigator ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(Color::Rgb(24, 24, 37)));

    let type_icon = |ht: &str| -> &str {
        match ht {
            "pve" => "▪",
            "bm" => "▪",
            "pbs" => "▪",
            "monitor" => "▪",
            _ => "▪",
        }
    };

    let type_color = |ht: &str| -> Color {
        match ht {
            "pve" => ACCENT,
            "bm" => PEACH,
            "pbs" => TEAL,
            "monitor" => YELLOW,
            _ => DIMMED,
        }
    };

    let items: Vec<ListItem> = app
        .sidebar_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == app.sidebar_index
                && (app.mode == AppMode::Browse || app.mode == AppMode::Detail)
                && app.sidebar_focus == SidebarFocus::Sidebar;

            match item {
                SidebarItem::RecentHeader => {
                    let line = Line::from(vec![
                        Span::styled("  Recents", Style::default().fg(PEACH).add_modifier(Modifier::BOLD)),
                    ]);
                    ListItem::new(line).style(Style::default().fg(PEACH))
                }
                SidebarItem::Recent(idx) => {
                    let name = app.recent_connections.get(*idx)
                        .map(|s| s.name.as_str())
                        .unwrap_or("?");
                    let style = if is_selected {
                        Style::default().fg(HIGHLIGHT_FG).bg(HIGHLIGHT_BG).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(DIMMED)
                    };
                    let marker = if is_selected { "  ▸ " } else { "    " };
                    let line = Line::from(vec![
                        Span::raw(marker),
                        Span::styled(name, style),
                    ]);
                    ListItem::new(line).style(style)
                }
                SidebarItem::Category(ci) => {
                    let cat = &app.config.categories[*ci];
                    // Detecta host_type do primeiro servidor da categoria
                    let ht = cat.servers.first().map(|s| s.host_type.as_str()).unwrap_or("");
                    let icon_color = type_color(ht);
                    let total = cat.servers.len();

                    let style = if is_selected {
                        Style::default().fg(HIGHLIGHT_FG).bg(HIGHLIGHT_BG).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(SUBTEXT)
                    };

                    let marker = if is_selected { "▸ " } else { "  " };

                    // Online count: only show online/total when at least one server has status
                    let has_status = cat.servers.iter().any(|s| !s.status.is_empty());
                    let count_spans = if has_status {
                        let online = cat.servers.iter().filter(|s| s.status.to_lowercase() == "online").count();
                        let count_color = if online == total {
                            GREEN
                        } else if online == 0 {
                            RED
                        } else {
                            YELLOW
                        };
                        vec![
                            Span::styled(format!(" {}", online), Style::default().fg(count_color)),
                            Span::styled(format!("/{}", total), Style::default().fg(DIMMED)),
                        ]
                    } else {
                        vec![Span::styled(format!(" {}", total), Style::default().fg(DIMMED))]
                    };

                    let mut spans = vec![
                        Span::raw(marker),
                        Span::styled(type_icon(ht), Style::default().fg(icon_color)),
                        Span::raw(" "),
                        Span::styled(cat.name.as_str(), style),
                    ];
                    spans.extend(count_spans);
                    let line = Line::from(spans);
                    ListItem::new(line).style(style)
                }
                SidebarItem::GroupHeader { prefix, expanded, count } => {
                    let arrow = if *expanded { "▾" } else { "▸" };
                    let style = if is_selected {
                        Style::default().fg(HIGHLIGHT_FG).bg(HIGHLIGHT_BG).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(MAUVE)
                    };
                    let marker = if is_selected { "▸ " } else { "  " };
                    let line = Line::from(vec![
                        Span::raw(marker),
                        Span::styled(format!("{} ", arrow), Style::default().fg(MAUVE)),
                        Span::styled(prefix.as_str(), style),
                        Span::styled(format!(" {}", count), Style::default().fg(DIMMED)),
                    ]);
                    ListItem::new(line).style(style)
                }
                SidebarItem::GroupChild(ci) => {
                    let cat = &app.config.categories[*ci];
                    let short_name = cat.name.strip_prefix("VMs > ").unwrap_or(&cat.name);
                    let count = cat.servers.len();

                    let style = if is_selected {
                        Style::default().fg(HIGHLIGHT_FG).bg(HIGHLIGHT_BG).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(DIMMED)
                    };
                    let marker = if is_selected { "  ▸ " } else { "    " };
                    let line = Line::from(vec![
                        Span::raw(marker),
                        Span::styled("◦ ", Style::default().fg(MAUVE)),
                        Span::styled(short_name, style),
                        Span::styled(format!(" {}", count), Style::default().fg(Color::Rgb(69, 71, 90))),
                    ]);
                    ListItem::new(line).style(style)
                }
            }
        })
        .collect();

    // Open tabs list at bottom of sidebar
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if !app.tabs.is_empty() {
        // Split sidebar inner: category list | tabs
        let sidebar_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(app.tabs.len() as u16 + 2)])
            .split(inner);

        frame.render_widget(List::new(items), sidebar_chunks[0]);
        draw_sidebar_tabs(frame, sidebar_chunks[1], app);
    } else {
        frame.render_widget(List::new(items), inner);
    }
}

fn draw_sidebar_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Sessions ")
        .borders(Borders::TOP)
        .border_style(Style::default().fg(INACTIVE_BORDER));

    let items: Vec<ListItem> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let is_active = app.active_tab == Some(i);
            let marker = if is_active { "▸ " } else { "  " };
            let text = format!("{}{}", marker, tab.name);
            let style = if is_active {
                Style::default().fg(TAB_ACTIVE_FG).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TAB_FG)
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(List::new(items), inner);
}

// ── Server list ───────────────────────────────────────────────────────────────

fn draw_server_list(frame: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.mode == AppMode::Browse
        && app.sidebar_focus == SidebarFocus::ServerList
    {
        ACTIVE_BORDER
    } else {
        INACTIVE_BORDER
    };

    let title = if app.mode == AppMode::Search {
        format!(" Servers [/: {}▎] ", app.search_query)
    } else if !app.search_query.is_empty() {
        format!(" Servers [filter: {}] ", app.search_query)
    } else {
        " Servers ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let filtered = app.filtered_servers();

    if filtered.is_empty() {
        let msg = if app.search_query.is_empty() {
            "No servers in this category"
        } else {
            "No matches found"
        };
        frame.render_widget(
            Paragraph::new(msg)
                .style(Style::default().fg(Color::DarkGray))
                .block(block),
            area,
        );
        return;
    }

    // Decide colunas baseado na largura disponível e tipo de servidor
    let inner_w = area.width.saturating_sub(4);
    let has_extra_info = filtered.iter().any(|s| !s.status.is_empty() || !s.ip_public.is_empty());
    let is_wide = inner_w > 80;

    let is_focused = app.mode == AppMode::Browse && app.sidebar_focus == SidebarFocus::ServerList;

    if is_wide && has_extra_info {
        // ── Layout largo: Status | Nome | Mesh IP | IP Público | Serviços | User ──
        let header = Row::new(vec![
            Cell::from(""),
            Cell::from("Nome").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Mesh IP").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("IP Público").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Serviços").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("User").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .style(Style::default().fg(ACCENT).bg(HEADER_BG))
        .height(1);

        let rows: Vec<Row> = filtered.iter().enumerate().map(|(i, server)| {
            let selected = app.is_selected(i);
            let mut style = row_style(i, app.server_index, is_focused);
            if selected {
                style = style.bg(Color::Rgb(30, 50, 30));
            }
            let status = status_indicator(&server.status);
            let svc = services_status(server);
            let name_display = if selected {
                format!("\u{2713} {}", server.name)
            } else {
                server.name.clone()
            };

            Row::new(vec![
                Cell::from(status.0).style(Style::default().fg(status.1)),
                Cell::from(name_display).style(if selected { Style::default().fg(GREEN) } else { Style::default() }),
                Cell::from(server.display_addr()),
                Cell::from(if server.ip_public.is_empty() { "—" } else { &server.ip_public }),
                Cell::from(svc),
                Cell::from(server.user.as_str()),
            ]).style(style)
        }).collect();

        let status_w = 2u16;
        let user_w = 8u16;
        let pub_w = 16u16;
        let svc_w = 15u16;
        let addr_w = 22u16;
        let name_w = inner_w.saturating_sub(status_w + addr_w + pub_w + svc_w + user_w);

        let table = Table::new(rows, [
            Constraint::Length(status_w),
            Constraint::Length(name_w),
            Constraint::Length(addr_w),
            Constraint::Length(pub_w),
            Constraint::Length(svc_w),
            Constraint::Length(user_w),
        ]).header(header).block(block);

        frame.render_widget(table, area);
    } else if has_extra_info {
        // ── Layout médio: Status | Nome | Endereço | User ──
        let header = Row::new(vec![
            Cell::from(""),
            Cell::from("Nome").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Endereço").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("User").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .style(Style::default().fg(ACCENT).bg(HEADER_BG))
        .height(1);

        let rows: Vec<Row> = filtered.iter().enumerate().map(|(i, server)| {
            let selected = app.is_selected(i);
            let mut style = row_style(i, app.server_index, is_focused);
            if selected {
                style = style.bg(Color::Rgb(30, 50, 30));
            }
            let status = status_indicator(&server.status);
            let name_display = if selected {
                format!("\u{2713} {}", server.name)
            } else {
                server.name.clone()
            };

            Row::new(vec![
                Cell::from(status.0).style(Style::default().fg(status.1)),
                Cell::from(name_display).style(if selected { Style::default().fg(GREEN) } else { Style::default() }),
                Cell::from(server.display_addr()),
                Cell::from(server.user.as_str()),
            ]).style(style)
        }).collect();

        let status_w = 2u16;
        let user_w = 8u16;
        let addr_w = 22u16;
        let name_w = inner_w.saturating_sub(status_w + addr_w + user_w);

        let table = Table::new(rows, [
            Constraint::Length(status_w),
            Constraint::Length(name_w),
            Constraint::Length(addr_w),
            Constraint::Length(user_w),
        ]).header(header).block(block);

        frame.render_widget(table, area);
    } else {
        // ── Layout simples (modo TOML): Nome | Endereço | User ──
        let header = Row::new(vec![
            Cell::from("Nome").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Endereço").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("User").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .style(Style::default().fg(ACCENT).bg(HEADER_BG))
        .height(1);

        let rows: Vec<Row> = filtered.iter().enumerate().map(|(i, server)| {
            let selected = app.is_selected(i);
            let mut style = row_style(i, app.server_index, is_focused);
            if selected {
                style = style.bg(Color::Rgb(30, 50, 30));
            }
            let name_display = if selected {
                format!("\u{2713} {}", server.name)
            } else {
                server.name.clone()
            };

            Row::new(vec![
                Cell::from(name_display).style(if selected { Style::default().fg(GREEN) } else { Style::default() }),
                Cell::from(server.display_addr()),
                Cell::from(server.user.as_str()),
            ]).style(style)
        }).collect();

        let addr_w = 22u16;
        let user_w = 10u16;
        let name_w = inner_w.saturating_sub(addr_w + user_w);

        let table = Table::new(rows, [
            Constraint::Length(name_w),
            Constraint::Length(addr_w),
            Constraint::Length(user_w),
        ]).header(header).block(block);

        frame.render_widget(table, area);
    }
}

// ── Global search results ─────────────────────────────────────────────────────

fn draw_global_search(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(format!(" Busca: {}▎ ", app.search_query))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACTIVE_BORDER));

    if app.global_results.is_empty() {
        let msg = if app.search_query.is_empty() {
            "Digite para buscar em todos os servidores..."
        } else {
            "Nenhum resultado encontrado"
        };
        frame.render_widget(
            Paragraph::new(msg)
                .style(Style::default().fg(Color::DarkGray))
                .block(block),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("Nome").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Endereço").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Categoria").style(Style::default().add_modifier(Modifier::BOLD)),
    ])
    .style(Style::default().fg(ACCENT).bg(HEADER_BG))
    .height(1);

    let rows: Vec<Row> = app
        .global_results
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let style = if i == app.global_index {
                Style::default()
                    .fg(HIGHLIGHT_FG)
                    .bg(HIGHLIGHT_BG)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            Row::new(vec![
                Cell::from(m.server.name.as_str()),
                Cell::from(m.server.display_addr()),
                Cell::from(m.category_name.as_str()),
            ])
            .style(style)
        })
        .collect();

    let inner_w = area.width.saturating_sub(4);
    let cat_w = 20u16;
    let addr_w = 22u16;
    let name_w = inner_w.saturating_sub(addr_w + cat_w);

    let count = app.global_results.len();
    let block = block.title_bottom(format!(" {} resultados ", count));

    let table = Table::new(rows, [
        Constraint::Length(name_w),
        Constraint::Length(addr_w),
        Constraint::Length(cat_w),
    ])
    .header(header)
    .block(block);

    frame.render_widget(table, area);
}

// ── Terminal panel ────────────────────────────────────────────────────────────

/// Wrapper que acessa tab por índice (evita borrow conflicts)
fn draw_terminal_panel_by_idx(frame: &mut Frame, area: Rect, app: &mut App, tab_idx: usize) {
    // Safety: precisamos de referências separadas pra tabs[idx] e app
    // Extraímos os dados necessários da session primeiro
    let session_name = app.tabs[tab_idx].name.clone();
    let tab_count = app.tabs.len();
    let title = if app.broadcast {
        format!(" {} ● BROADCAST ", session_name)
    } else {
        format!(" {} ", session_name)
    };
    let border_color = if app.broadcast { RED } else { ACTIVE_BORDER };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(Color::Rgb(0, 0, 0)));

    let inner = if tab_count > 1 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(block.inner(area));

        frame.render_widget(block, area);
        draw_tab_bar(frame, chunks[0], app);
        chunks[1]
    } else {
        app.mouse_tab_bar = None;
        let inner = block.inner(area);
        frame.render_widget(block, area);
        inner
    };

    let session = &app.tabs[tab_idx];
    if session.is_dead() {
        draw_dead_session_overlay(frame, inner, &session.name);
    } else {
        frame.render_widget(TermWidget { session }, inner);
    }
}

/// Wrapper pra split que evita borrow conflicts
fn draw_split_terminal_by_ref(frame: &mut Frame, area: Rect, app: &mut App) {
    // Copia os dados do split pra evitar borrow
    let layout = match app.split {
        Some(ref s) => s.layout,
        None => return,
    };
    let panes: Vec<usize> = app.split.as_ref().unwrap().panes.clone();
    let focused = app.split.as_ref().unwrap().focused_pane;

    let pane_areas = split_layout_rects(area, layout);

    let broadcast = app.broadcast;
    for (pane_idx, &tab_idx) in panes.iter().enumerate() {
        if tab_idx < app.tabs.len() {
            let is_focused = pane_idx == focused;
            let session = &app.tabs[tab_idx];
            draw_split_pane(frame, pane_areas[pane_idx], session, is_focused, broadcast);
        }
    }

    app.mouse_tab_bar = None;
}

fn draw_tab_bar(frame: &mut Frame, area: Rect, app: &mut App) {
    let mut spans = vec![];
    let mut tab_ranges: Vec<(u16, u16)> = vec![];
    let mut x_pos = area.x;

    for (i, tab) in app.tabs.iter().enumerate() {
        let is_active = app.active_tab == Some(i);
        let label = format!(" {} ", tab.name);
        let label_len = label.len() as u16;
        let tab_start = x_pos;

        if is_active {
            spans.push(Span::styled(
                label,
                Style::default()
                    .fg(TAB_ACTIVE_FG)
                    .bg(TAB_ACTIVE_BG)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                label,
                Style::default().fg(TAB_FG).bg(TAB_BG),
            ));
        }
        x_pos += label_len;
        spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
        x_pos += 1;

        tab_ranges.push((tab_start, tab_start + label_len));
    }

    app.mouse_tab_bar = Some((area.y, area.x, tab_ranges));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ── TermWidget ────────────────────────────────────────────────────────────────

struct TermWidget<'a> {
    session: &'a TerminalSession,
}

impl Widget for TermWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let term = self.session.term.lock();
        let grid = term.grid();
        let screen_lines = term.screen_lines();
        let columns = term.columns();

        for row in 0..area.height as usize {
            if row >= screen_lines {
                break;
            }
            let line_idx = TermLine(row as i32);

            for col in 0..area.width as usize {
                if col >= columns {
                    break;
                }

                let cell = &grid[line_idx][Column(col)];
                let x = area.x + col as u16;
                let y = area.y + row as u16;

                if x >= buf.area.right() || y >= buf.area.bottom() {
                    continue;
                }

                let fg = ansi_to_ratatui(cell.fg, true);
                let bg = ansi_to_ratatui(cell.bg, false);

                use alacritty_terminal::term::cell::Flags;
                let mut style = Style::default().fg(fg).bg(bg);

                if cell.flags.contains(Flags::BOLD) {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if cell.flags.contains(Flags::ITALIC) {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if cell.flags.contains(Flags::UNDERLINE) {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if cell.flags.contains(Flags::INVERSE) {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                if cell.flags.contains(Flags::DIM) {
                    style = style.add_modifier(Modifier::DIM);
                }
                if cell.flags.contains(Flags::HIDDEN) {
                    style = style.add_modifier(Modifier::HIDDEN);
                }
                if cell.flags.contains(Flags::STRIKEOUT) {
                    style = style.add_modifier(Modifier::CROSSED_OUT);
                }

                // Não renderiza wide char spacers (espaço vazio de caractere largo)
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }

                let c = if cell.c == '\0' { ' ' } else { cell.c };

                let buf_cell = buf.cell_mut((x, y));
                if let Some(bc) = buf_cell {
                    bc.set_char(c);
                    bc.set_style(style);
                }
            }
        }

        // Renderiza o cursor
        let display_offset = grid.display_offset();
        let cursor = grid.cursor.point;
        let cx = area.x + cursor.column.0 as u16;
        let cy = area.y + cursor.line.0 as u16;
        if display_offset == 0 && cx < area.right() && cy < area.bottom() && cx < buf.area.right() && cy < buf.area.bottom() {
            if let Some(bc) = buf.cell_mut((cx, cy)) {
                bc.set_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
            }
        }

        // Scroll indicator
        if display_offset > 0 {
            let indicator = format!("[SCROLL +{}]", display_offset);
            let indicator_len = indicator.len() as u16;
            let start_x = area.right().saturating_sub(indicator_len + 1);
            let y = area.y;
            let style = Style::default().fg(YELLOW).bg(Color::Rgb(30, 30, 46)).add_modifier(Modifier::BOLD);
            for (i, ch) in indicator.chars().enumerate() {
                let x = start_x + i as u16;
                if x < area.right() && y < area.bottom() && x < buf.area.right() && y < buf.area.bottom() {
                    if let Some(bc) = buf.cell_mut((x, y)) {
                        bc.set_char(ch);
                        bc.set_style(style);
                    }
                }
            }
        }
    }
}

// ── Split terminal ────────────────────────────────────────────────────────────

fn split_layout_rects(area: Rect, layout: SplitLayout) -> Vec<Rect> {
    match layout {
        SplitLayout::Vertical2 => {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            vec![chunks[0], chunks[1]]
        }
        SplitLayout::Horizontal2 => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            vec![chunks[0], chunks[1]]
        }
        SplitLayout::Quad => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            let top = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[0]);
            let bot = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[1]);
            vec![top[0], top[1], bot[0], bot[1]]
        }
    }
}

fn draw_split_pane(
    frame: &mut Frame,
    area: Rect,
    session: &TerminalSession,
    is_focused: bool,
    broadcast: bool,
) {
    let border_color = if broadcast { RED } else if is_focused { ACTIVE_BORDER } else { INACTIVE_BORDER };

    let block = Block::default()
        .title(format!(" {} ", session.name))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(Color::Rgb(17, 17, 27)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if session.is_dead() {
        draw_dead_session_overlay(frame, inner, &session.name);
    } else {
        let term_widget = TermWidget { session };
        frame.render_widget(term_widget, inner);
    }
}

/// Draws a "disconnected" overlay when a terminal session has died
fn draw_dead_session_overlay(frame: &mut Frame, area: Rect, name: &str) {
    // Fill with dark background
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(17, 17, 27))),
        area,
    );

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("     ", Style::default()),
            Span::styled("●", Style::default().fg(RED).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {} (disconnected)", name), Style::default().fg(SUBTEXT)),
        ]),
        Line::from(""),
        Line::from(Span::styled("     Connection closed", Style::default().fg(SUBTEXT))),
        Line::from(""),
        Line::from(vec![
            Span::styled("     ", Style::default()),
            Span::styled("Enter", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled(" = Reconnect", Style::default().fg(SUBTEXT)),
        ]),
        Line::from(vec![
            Span::styled("     ", Style::default()),
            Span::styled("Esc", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled("   = Close tab", Style::default().fg(SUBTEXT)),
        ]),
    ];

    // Center vertically
    let text_h = lines.len() as u16;
    let y_offset = area.height.saturating_sub(text_h) / 2;
    let text_area = Rect::new(area.x, area.y + y_offset, area.width, text_h.min(area.height));

    frame.render_widget(Paragraph::new(lines), text_area);
}

// ── Detail popup ──────────────────────────────────────────────────────────────

fn draw_detail_popup(frame: &mut Frame, area: Rect, server: &crate::config::Server) {
    // Centered 60% x 60% popup
    let popup_w = (area.width as f32 * 0.6) as u16;
    let popup_h = (area.height as f32 * 0.6) as u16;
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    // Clear background
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(24, 24, 37))),
        popup_area,
    );

    let block = Block::default()
        .title(Span::styled(
            " Server Details ",
            Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MAUVE))
        .style(Style::default().bg(Color::Rgb(24, 24, 37)));

    let label_style = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
    let value_style = Style::default().fg(TEXT);
    let empty_style = Style::default().fg(DIMMED);

    let val = |s: &str| -> Span {
        if s.is_empty() {
            Span::styled("--", empty_style)
        } else {
            Span::styled(s.to_string(), value_style)
        }
    };

    let (status_icon, status_color) = status_indicator(&server.status);
    let (wg_icon, wg_color) = status_indicator(&server.wg_status);
    let (zbx_icon, zbx_color) = status_indicator(&server.zabbix_status);
    let (fb_icon, fb_color) = status_indicator(&server.fluentbit_status);

    let lines = vec![
        Line::from(vec![
            Span::styled("  Name:          ", label_style),
            val(&server.name),
        ]),
        Line::from(vec![
            Span::styled("  Hostname:      ", label_style),
            val(&server.hostname),
        ]),
        Line::from(vec![
            Span::styled("  Host Type:     ", label_style),
            val(&server.host_type),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Mesh IP:       ", label_style),
            val(&server.host),
        ]),
        Line::from(vec![
            Span::styled("  Public IP:     ", label_style),
            val(&server.ip_public),
        ]),
        Line::from(vec![
            Span::styled("  Subnet:        ", label_style),
            val(&server.subnet),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Port SSH:      ", label_style),
            Span::styled(server.port.to_string(), value_style),
        ]),
        Line::from(vec![
            Span::styled("  User:          ", label_style),
            val(&server.user),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Status:        ", label_style),
            Span::styled(format!("{} ", status_icon), Style::default().fg(status_color)),
            val(&server.status),
        ]),
        Line::from(vec![
            Span::styled("  WG Status:     ", label_style),
            Span::styled(format!("{} ", wg_icon), Style::default().fg(wg_color)),
            val(&server.wg_status),
        ]),
        Line::from(vec![
            Span::styled("  Zabbix Status: ", label_style),
            Span::styled(format!("{} ", zbx_icon), Style::default().fg(zbx_color)),
            val(&server.zabbix_status),
        ]),
        Line::from(vec![
            Span::styled("  FluentBit:     ", label_style),
            Span::styled(format!("{} ", fb_icon), Style::default().fg(fb_color)),
            val(&server.fluentbit_status),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Host Name:     ", label_style),
            val(&server.host_name),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, popup_area);
}

// ── Command Palette ───────────────────────────────────────────────────────────

fn draw_palette(frame: &mut Frame, area: Rect, app: &App) {
    let popup_w = (area.width as f32 * 0.70) as u16;
    let popup_h = (area.height as f32 * 0.50) as u16;
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    // Clear background
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(24, 24, 37))),
        popup_area,
    );

    let result_count = app.palette_filtered.len();
    let block = Block::default()
        .title(Span::styled(
            " Command Palette ",
            Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(format!(" {} resultados ", result_count))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MAUVE))
        .style(Style::default().bg(Color::Rgb(24, 24, 37)));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 2 || inner.width < 4 {
        return;
    }

    // Layout: search input (1 line) + results list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    // Search input
    let input_line = Line::from(vec![
        Span::styled(" > ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(&app.palette_query, Style::default().fg(TEXT)),
        Span::styled("_", Style::default().fg(ACCENT)),
    ]);
    frame.render_widget(Paragraph::new(input_line), chunks[0]);

    // Results list
    let list_area = chunks[1];
    let visible_count = list_area.height as usize;

    // Scroll offset to keep selected item visible
    let scroll_offset = if app.palette_index >= visible_count {
        app.palette_index - visible_count + 1
    } else {
        0
    };

    let items: Vec<ListItem> = app.palette_filtered
        .iter()
        .skip(scroll_offset)
        .take(visible_count)
        .enumerate()
        .map(|(display_idx, &item_idx)| {
            let item = &app.palette_items[item_idx];
            let is_selected = display_idx + scroll_offset == app.palette_index;
            palette_list_item(item, is_selected)
        })
        .collect();

    frame.render_widget(List::new(items), list_area);
}

fn palette_list_item(item: &PaletteItem, is_selected: bool) -> ListItem<'static> {
    let style = if is_selected {
        Style::default().fg(HIGHLIGHT_FG).bg(HIGHLIGHT_BG).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let label_style = if is_selected {
        Style::default().fg(HIGHLIGHT_FG).bg(HIGHLIGHT_BG).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ACCENT)
    };

    let desc_style = if is_selected {
        Style::default().fg(HIGHLIGHT_FG).bg(HIGHLIGHT_BG)
    } else {
        Style::default().fg(DIMMED)
    };

    let marker = if is_selected { " > " } else { "   " };

    let line = Line::from(vec![
        Span::styled(marker.to_string(), style),
        Span::styled(item.label.clone(), label_style),
        Span::styled(format!("  {}", item.description), desc_style),
    ]);

    ListItem::new(line).style(style)
}

// ── Help popup ────────────────────────────────────────────────────────────────

fn draw_help_popup(frame: &mut Frame, area: Rect, app: &App) {
    let popup_w = (area.width as f32 * 0.80).min(80.0) as u16;
    let popup_h = (area.height as f32 * 0.80) as u16;
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    // Clear background
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(24, 24, 37))),
        popup_area,
    );

    let block = Block::default()
        .title(Span::styled(
            " Keyboard Shortcuts ",
            Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MAUVE))
        .style(Style::default().bg(Color::Rgb(24, 24, 37)));

    let key_style = Style::default().fg(PEACH).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(SUBTEXT);
    let section_style = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);

    let help_entry = |key: &str, desc: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("  {:<16}", key), key_style),
            Span::styled(desc.to_string(), desc_style),
        ])
    };

    let section = |title: &str| -> Line<'static> {
        Line::from(Span::styled(
            format!("  -- {} {}", title, "-".repeat(40usize.saturating_sub(title.len() + 5))),
            section_style,
        ))
    };

    let all_lines: Vec<Line<'static>> = vec![
        Line::from(""),
        section("Navigation"),
        help_entry("Up/Down / j/k", "Navigate lists"),
        help_entry("Enter / l", "Select / Connect"),
        help_entry("Esc / h", "Go back"),
        help_entry("Tab", "Switch panel focus"),
        help_entry("/", "Global search"),
        help_entry("g / G", "Jump to top / bottom"),
        Line::from(""),
        section("Server Actions"),
        help_entry("i", "Server details"),
        help_entry("c", "Copy IP to clipboard"),
        help_entry("Space", "Select/deselect server"),
        help_entry("Ctrl+P", "Command palette"),
        Line::from(""),
        section("Terminal"),
        help_entry("Ctrl+B", "Back to sidebar"),
        help_entry("Ctrl+W", "Close tab"),
        help_entry("Ctrl+Tab", "Next tab"),
        help_entry("F2", "Toggle split (V/H/Quad)"),
        help_entry("F3 / F4", "Next/prev split pane"),
        help_entry("F5", "Toggle broadcast"),
        help_entry("F6", "Run: htop"),
        help_entry("F7", "Run: docker ps"),
        help_entry("F8", "Run: journalctl -f"),
        help_entry("Shift+PgUp/PgDn", "Scroll terminal history"),
        Line::from(""),
        section("General"),
        help_entry("F1", "This help"),
        help_entry("Ctrl+L", "Logout (API mode)"),
        help_entry("F5", "Refresh (Browse, API)"),
        help_entry("q", "Quit"),
        Line::from(""),
    ];

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let visible_h = inner.height as usize;
    let max_scroll = all_lines.len().saturating_sub(visible_h);
    let scroll = app.help_scroll.min(max_scroll);

    let visible_lines: Vec<Line<'static>> = all_lines
        .into_iter()
        .skip(scroll)
        .take(visible_h)
        .collect();

    frame.render_widget(Paragraph::new(visible_lines), inner);
}

// ── Status bar ────────────────────────────────────────────────────────────────

pub fn draw_statusbar(frame: &mut Frame, area: Rect, app: &App) {
    let content = match app.mode {
        AppMode::Search => Line::from(vec![
            Span::styled(
                " SEARCH ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" {} ", app.search_query)),
            Span::styled(
                "(Enter confirma, Esc cancela)",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        AppMode::Terminal => {
            let tab_info = app
                .active_tab
                .map(|i| format!("Tab {}/{}", i + 1, app.tabs.len()))
                .unwrap_or_default();
            let mut spans = vec![
                Span::styled(
                    " TERMINAL ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" {} ", tab_info)),
            ];
            if let Some(ref split) = app.split {
                let layout_name = match split.layout {
                    SplitLayout::Vertical2 => "V-Split",
                    SplitLayout::Horizontal2 => "H-Split",
                    SplitLayout::Quad => "Quad",
                };
                spans.push(Span::styled(
                    format!(" {} ", layout_name),
                    Style::default().fg(TEAL).add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(
                    format!("Pane {}/{} ", split.focused_pane + 1, split.panes.len()),
                    Style::default().fg(SUBTEXT),
                ));
            }
            spans.extend([
                key_hint("Ctrl+B"),
                Span::raw(" Sidebar "),
                key_hint("Ctrl+W"),
                Span::raw(" Fechar "),
                key_hint("Ctrl+Tab"),
                Span::raw(" Proxima "),
                key_hint("F2"),
                Span::raw(" Split "),
                key_hint("F5"),
                Span::raw(" Broadcast "),
            ]);
            spans.extend([
                key_hint("F6"),
                Span::raw(" htop "),
                key_hint("F7"),
                Span::raw(" docker "),
                key_hint("F8"),
                Span::raw(" logs "),
            ]);
            if app.broadcast {
                spans.push(Span::styled(
                    " ● BROADCAST ",
                    Style::default().fg(Color::Rgb(17, 17, 27)).bg(RED).add_modifier(Modifier::BOLD),
                ));
            }
            if app.is_split() {
                spans.push(key_hint("F3/F4"));
                spans.push(Span::raw(" Painel "));
            }
            Line::from(spans)
        }
        AppMode::Detail => Line::from(vec![
            Span::styled(
                " DETAIL ",
                Style::default()
                    .fg(Color::Black)
                    .bg(MAUVE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            key_hint("Esc"),
            Span::raw(" Fechar "),
            key_hint("i"),
            Span::raw(" Fechar "),
        ]),
        AppMode::Browse if app.clipboard_msg.is_some() => {
            let msg = app.clipboard_msg.as_ref().unwrap();
            Line::from(vec![
                Span::styled(
                    " CLIPBOARD ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(GREEN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {} ", msg),
                    Style::default().fg(GREEN),
                ),
            ])
        }
        AppMode::Palette => Line::from(vec![
            Span::styled(
                " PALETTE ",
                Style::default()
                    .fg(Color::Black)
                    .bg(MAUVE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            key_hint("Esc"),
            Span::raw(" fechar "),
            key_hint("Enter"),
            Span::raw(" executar "),
            key_hint("↑↓"),
            Span::raw(" navegar "),
        ]),
        AppMode::Help => Line::from(vec![
            Span::styled(
                " HELP ",
                Style::default()
                    .fg(Color::Black)
                    .bg(MAUVE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            key_hint("Esc/F1/q"),
            Span::raw(" Fechar "),
            key_hint("↑↓"),
            Span::raw(" Scroll "),
        ]),
        AppMode::Browse => {
            let mut hints = vec![];
            if app.has_selection() {
                hints.push(Span::styled(
                    format!(" {} selected ", app.selected_servers.len()),
                    Style::default().fg(Color::Rgb(17, 17, 27)).bg(GREEN).add_modifier(Modifier::BOLD),
                ));
                hints.push(Span::raw(" "));
                hints.push(key_hint("Enter"));
                hints.push(Span::raw(" Conectar selecionados "));
                hints.push(key_hint("Space"));
                hints.push(Span::raw(" Selecionar "));
            } else {
                hints.extend([
                    key_hint("q"),
                    Span::raw(" Sair "),
                    key_hint("/"),
                    Span::raw(" Buscar "),
                    key_hint("Enter"),
                    Span::raw(" Conectar "),
                    key_hint("Space"),
                    Span::raw(" Selecionar "),
                    key_hint("i"),
                    Span::raw(" Info "),
                    key_hint("c"),
                    Span::raw(" Copiar "),
                ]);
            }
            hints.extend([
                key_hint("Ctrl+P"),
                Span::raw(" Palette "),
                key_hint("F1"),
                Span::raw(" Help "),
            ]);
            if app.is_api_mode() {
                hints.push(key_hint("Ctrl+L"));
                hints.push(Span::raw(" Logout "));
                hints.push(key_hint("F5"));
                hints.push(Span::raw(" Refresh "));
            }
            if !app.tabs.is_empty() {
                hints.push(Span::styled(
                    format!(" [{}] sessões", app.tabs.len()),
                    Style::default().fg(Color::Cyan),
                ));
            }
            Line::from(hints)
        }
    };

    frame.render_widget(
        Paragraph::new(content).style(Style::default().bg(STATUS_BG)),
        area,
    );
}

fn key_hint(k: &str) -> Span<'static> {
    Span::styled(
        format!(" {k} "),
        Style::default()
            .fg(Color::Rgb(17, 17, 27))
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD),
    )
}
