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

use crate::app::{App, AppMode, SidebarFocus, SidebarItem};
use crate::terminal::TerminalSession;

// ── Paleta ────────────────────────────────────────────────────────────────────

const ACTIVE_BORDER: Color = Color::Cyan;
const INACTIVE_BORDER: Color = Color::DarkGray;
const HIGHLIGHT_BG: Color = Color::Rgb(38, 79, 120);
const HIGHLIGHT_FG: Color = Color::White;
const HEADER_BG: Color = Color::Rgb(0, 0, 0);
const STATUS_BG: Color = Color::Rgb(0, 122, 204);
const TAB_ACTIVE_BG: Color = Color::Rgb(30, 30, 46);
const TAB_ACTIVE_FG: Color = Color::Cyan;
const TAB_BG: Color = Color::Rgb(20, 20, 30);
const TAB_FG: Color = Color::DarkGray;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn row_style(index: usize, selected: usize, is_focused: bool) -> Style {
    if index == selected && is_focused {
        Style::default()
            .fg(HIGHLIGHT_FG)
            .bg(HIGHLIGHT_BG)
            .add_modifier(Modifier::BOLD)
    } else if index == selected {
        Style::default().fg(Color::White).bg(Color::Rgb(50, 50, 50))
    } else {
        Style::default().fg(Color::Gray)
    }
}

fn status_indicator(status: &str) -> (&'static str, Color) {
    match status.to_lowercase().as_str() {
        "deployed" | "online" | "running" | "active" => ("●", Color::Green),
        "pending" | "deploying" | "provisioning" => ("◐", Color::Yellow),
        "error" | "failed" | "offline" | "stopped" => ("●", Color::Red),
        "" => (" ", Color::DarkGray),
        _ => ("○", Color::DarkGray),
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

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Pinta TODO o frame de preto puro (Rgb evita o "cinza" do ANSI Black)
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0))),
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
}

// ── Title bar ─────────────────────────────────────────────────────────────────

fn draw_titlebar(frame: &mut Frame, area: Rect, app: &App) {
    let cat_name = app
        .config
        .categories
        .get(app.category_index)
        .map(|c| c.name.as_str())
        .unwrap_or("");

    let title = Line::from(vec![
        Span::styled(
            format!(" SoureiGate v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" > "),
        Span::styled(cat_name, Style::default().fg(Color::Yellow)),
    ]);

    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(HEADER_BG)),
        area,
    );
}

// ── Body ──────────────────────────────────────────────────────────────────────

fn draw_body(frame: &mut Frame, area: Rect, app: &App) {
    let sidebar_w = (area.width as f32 * 0.22).max(20.0).min(30.0) as u16;

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(sidebar_w), Constraint::Min(30)])
        .split(area);

    draw_sidebar(frame, chunks[0], app);

    match app.mode {
        AppMode::Terminal => {
            if let Some(session) = app.active_session() {
                draw_terminal_panel(frame, chunks[1], app, session);
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
    let border_color = if app.mode == AppMode::Browse
        && app.sidebar_focus == SidebarFocus::Sidebar
    {
        ACTIVE_BORDER
    } else {
        INACTIVE_BORDER
    };

    let block = Block::default()
        .title(" Categories ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let items: Vec<ListItem> = app
        .sidebar_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == app.sidebar_index
                && app.mode == AppMode::Browse
                && app.sidebar_focus == SidebarFocus::Sidebar;

            let (text, indent) = match item {
                SidebarItem::Category(ci) => {
                    let cat = &app.config.categories[*ci];
                    (format!("{} ({})", cat.name, cat.servers.len()), false)
                }
                SidebarItem::GroupHeader { prefix, expanded, count } => {
                    let arrow = if *expanded { "▾" } else { "▸" };
                    (format!("{} {} ({})", arrow, prefix, count), false)
                }
                SidebarItem::GroupChild(ci) => {
                    let cat = &app.config.categories[*ci];
                    // Remove prefixo "VMs > " do nome
                    let short_name = cat.name.strip_prefix("VMs > ").unwrap_or(&cat.name);
                    (format!("{} ({})", short_name, cat.servers.len()), true)
                }
            };

            let prefix = if is_selected {
                if indent { "  ▸ " } else { "▸ " }
            } else if indent {
                "    "
            } else {
                "  "
            };

            let style = if is_selected {
                Style::default()
                    .fg(HIGHLIGHT_FG)
                    .bg(HIGHLIGHT_BG)
                    .add_modifier(Modifier::BOLD)
            } else if indent {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray)
            };

            ListItem::new(format!("{}{}", prefix, text)).style(style)
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
        // ── Layout largo: Status | Nome | Mesh IP | IP Público | Subnet | User ──
        let header = Row::new(vec![
            Cell::from(""),
            Cell::from("Nome").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Mesh IP").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("IP Público").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Subnet").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("User").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .style(Style::default().fg(Color::Cyan).bg(HEADER_BG))
        .height(1);

        let rows: Vec<Row> = filtered.iter().enumerate().map(|(i, server)| {
            let style = row_style(i, app.server_index, is_focused);
            let status = status_indicator(&server.status);

            Row::new(vec![
                Cell::from(status.0).style(Style::default().fg(status.1)),
                Cell::from(server.name.as_str()),
                Cell::from(server.display_addr()),
                Cell::from(if server.ip_public.is_empty() { "—" } else { &server.ip_public }),
                Cell::from(if server.subnet.is_empty() { "—" } else { &server.subnet }),
                Cell::from(server.user.as_str()),
            ]).style(style)
        }).collect();

        let status_w = 2u16;
        let user_w = 8u16;
        let pub_w = 16u16;
        let sub_w = 18u16;
        let addr_w = 22u16;
        let name_w = inner_w.saturating_sub(status_w + addr_w + pub_w + sub_w + user_w);

        let table = Table::new(rows, [
            Constraint::Length(status_w),
            Constraint::Length(name_w),
            Constraint::Length(addr_w),
            Constraint::Length(pub_w),
            Constraint::Length(sub_w),
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
        .style(Style::default().fg(Color::Cyan).bg(HEADER_BG))
        .height(1);

        let rows: Vec<Row> = filtered.iter().enumerate().map(|(i, server)| {
            let style = row_style(i, app.server_index, is_focused);
            let status = status_indicator(&server.status);

            Row::new(vec![
                Cell::from(status.0).style(Style::default().fg(status.1)),
                Cell::from(server.name.as_str()),
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
        .style(Style::default().fg(Color::Cyan).bg(HEADER_BG))
        .height(1);

        let rows: Vec<Row> = filtered.iter().enumerate().map(|(i, server)| {
            let style = row_style(i, app.server_index, is_focused);
            Row::new(vec![
                Cell::from(server.name.as_str()),
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
    .style(Style::default().fg(Color::Cyan).bg(HEADER_BG))
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

fn draw_terminal_panel(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    session: &TerminalSession,
) {
    let block = Block::default()
        .title(format!(" {} ", session.name))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACTIVE_BORDER))
        .style(Style::default().bg(Color::Rgb(0, 0, 0)));

    // Tab bar (se tiver mais de 1 sessão)
    let inner = if app.tabs.len() > 1 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(block.inner(area));

        frame.render_widget(block, area);
        draw_tab_bar(frame, chunks[0], app);
        chunks[1]
    } else {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        inner
    };

    // Renderiza o grid do terminal
    let term_widget = TermWidget { session };
    frame.render_widget(term_widget, inner);
}

fn draw_tab_bar(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![];
    for (i, tab) in app.tabs.iter().enumerate() {
        let is_active = app.active_tab == Some(i);
        if is_active {
            spans.push(Span::styled(
                format!(" {} ", tab.name),
                Style::default()
                    .fg(TAB_ACTIVE_FG)
                    .bg(TAB_ACTIVE_BG)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!(" {} ", tab.name),
                Style::default().fg(TAB_FG).bg(TAB_BG),
            ));
        }
        spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
    }
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
        let cursor = grid.cursor.point;
        let cx = area.x + cursor.column.0 as u16;
        let cy = area.y + cursor.line.0 as u16;
        if cx < area.right() && cy < area.bottom() && cx < buf.area.right() && cy < buf.area.bottom() {
            if let Some(bc) = buf.cell_mut((cx, cy)) {
                bc.set_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
            }
        }
    }
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
            Line::from(vec![
                Span::styled(
                    " TERMINAL ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" {} ", tab_info)),
                key_hint("Ctrl+B"),
                Span::raw(" Sidebar "),
                key_hint("Ctrl+W"),
                Span::raw(" Fechar "),
                key_hint("Ctrl+Tab"),
                Span::raw(" Próxima "),
            ])
        }
        AppMode::Browse => {
            let mut hints = vec![
                key_hint("q"),
                Span::raw(" Sair "),
                key_hint("/"),
                Span::raw(" Buscar "),
                key_hint("Enter"),
                Span::raw(" Conectar "),
                key_hint("Tab"),
                Span::raw(" Painel "),
                key_hint("↑↓"),
                Span::raw(" Navegar "),
            ];
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
            .fg(Color::Black)
            .bg(Color::Rgb(200, 200, 200))
            .add_modifier(Modifier::BOLD),
    )
}
