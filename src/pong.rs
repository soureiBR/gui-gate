//! SoureiGate PONG — Multiplayer Pong over UDP
//! Activated with :pong or :pong <host_ip> in command input

use std::process::Command;

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::net::UdpSocket;

// ── Colors ──────────────────────────────────────────────────────────────────

const BG: Color = Color::Rgb(5, 15, 5);
const PADDLE_COLOR: Color = Color::Rgb(200, 200, 255);
const BALL_COLOR: Color = Color::Rgb(255, 255, 100);
const SCORE_COLOR: Color = Color::Rgb(255, 255, 255);
const NET_COLOR: Color = Color::Rgb(40, 80, 40);
const WAITING_COLOR: Color = Color::Rgb(255, 200, 100);
const WIN_COLOR: Color = Color::Rgb(100, 255, 100);

const PONG_PORT: u16 = 9999;
const WIN_SCORE: u8 = 11;

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
pub enum PongRole {
    Host,
    Client,
}

pub struct PongGame {
    pub role: PongRole,
    pub socket: Option<UdpSocket>,
    pub peer_addr: Option<std::net::SocketAddr>,

    // Game state
    pub ball_x: f32,
    pub ball_y: f32,
    pub ball_dx: f32,
    pub ball_dy: f32,
    pub paddle_left_y: f32,
    pub paddle_right_y: f32,
    pub score_left: u8,
    pub score_right: u8,
    pub game_active: bool,
    pub waiting_for_player: bool,
    pub winner: Option<u8>, // 1 = left, 2 = right

    // Dimensions
    pub width: f32,
    pub height: f32,

    pub frame: u64,
}

impl PongGame {
    pub fn host(width: u16, height: u16) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(format!("0.0.0.0:{}", PONG_PORT))?;
        socket.set_nonblocking(true)?;
        let w = width as f32;
        let h = height as f32;
        Ok(Self {
            role: PongRole::Host,
            socket: Some(socket),
            peer_addr: None,
            ball_x: w / 2.0,
            ball_y: h / 2.0,
            ball_dx: 0.5,
            ball_dy: 0.3,
            paddle_left_y: h / 2.0,
            paddle_right_y: h / 2.0,
            score_left: 0,
            score_right: 0,
            game_active: false,
            waiting_for_player: true,
            winner: None,
            width: w,
            height: h,
            frame: 0,
        })
    }

    pub fn client(host_ip: &str, width: u16, height: u16) -> std::io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        let addr: std::net::SocketAddr = format!("{}:{}", host_ip, PONG_PORT)
            .parse()
            .map_err(|e| std::io::Error::other(format!("{}", e)))?;
        // Send initial hello packet
        socket.send_to(&[0u8; 1], addr)?;
        let w = width as f32;
        let h = height as f32;
        Ok(Self {
            role: PongRole::Client,
            socket: Some(socket),
            peer_addr: Some(addr),
            ball_x: w / 2.0,
            ball_y: h / 2.0,
            ball_dx: 0.5,
            ball_dy: 0.3,
            paddle_left_y: h / 2.0,
            paddle_right_y: h / 2.0,
            score_left: 0,
            score_right: 0,
            game_active: true,
            waiting_for_player: false,
            winner: None,
            width: w,
            height: h,
            frame: 0,
        })
    }

    pub fn tick(&mut self) {
        self.frame += 1;

        // Receive network data (non-blocking)
        self.recv_network();

        if !self.game_active || self.winner.is_some() {
            return;
        }

        // Only host calculates ball physics
        if self.role == PongRole::Host {
            // Move ball
            self.ball_x += self.ball_dx;
            self.ball_y += self.ball_dy;

            // Wall bounce (top/bottom)
            if self.ball_y <= 1.0 {
                self.ball_y = 1.0;
                self.ball_dy *= -1.0;
            }
            if self.ball_y >= self.height - 2.0 {
                self.ball_y = self.height - 2.0;
                self.ball_dy *= -1.0;
            }

            // Paddle collision - left paddle (host)
            if self.ball_x <= 4.0 && self.ball_dx < 0.0 {
                if (self.ball_y - self.paddle_left_y).abs() < 3.0 {
                    self.ball_dx *= -1.05;
                    self.ball_dy += (self.ball_y - self.paddle_left_y) * 0.2;
                }
            }

            // Right paddle (client)
            if self.ball_x >= self.width - 5.0 && self.ball_dx > 0.0 {
                if (self.ball_y - self.paddle_right_y).abs() < 3.0 {
                    self.ball_dx *= -1.05;
                    self.ball_dy += (self.ball_y - self.paddle_right_y) * 0.2;
                }
            }

            // Score - ball passes paddle
            if self.ball_x <= 0.0 {
                self.score_right += 1;
                if self.score_right >= WIN_SCORE {
                    self.winner = Some(2);
                } else {
                    self.reset_ball();
                }
            }
            if self.ball_x >= self.width {
                self.score_left += 1;
                if self.score_left >= WIN_SCORE {
                    self.winner = Some(1);
                } else {
                    self.reset_ball();
                }
            }

            // Clamp ball speed
            self.ball_dx = self.ball_dx.clamp(-1.5, 1.5);
            self.ball_dy = self.ball_dy.clamp(-1.0, 1.0);
        }

        // Send network data every other frame
        if self.frame % 2 == 0 {
            self.send_network();
        }
    }

    fn send_network(&self) {
        let socket = match &self.socket {
            Some(s) => s,
            None => return,
        };
        let addr = match &self.peer_addr {
            Some(a) => *a,
            None => return,
        };

        let mut buf = [0u8; 25];
        let my_paddle = match self.role {
            PongRole::Host => self.paddle_left_y,
            PongRole::Client => self.paddle_right_y,
        };
        buf[0..4].copy_from_slice(&my_paddle.to_le_bytes());
        buf[4..8].copy_from_slice(&self.ball_x.to_le_bytes());
        buf[8..12].copy_from_slice(&self.ball_y.to_le_bytes());
        buf[12..16].copy_from_slice(&self.ball_dx.to_le_bytes());
        buf[16..20].copy_from_slice(&self.ball_dy.to_le_bytes());
        buf[20] = self.score_left;
        buf[21] = self.score_right;
        buf[22] = if self.game_active && self.winner.is_none() {
            1
        } else {
            0
        };
        buf[23] = self.winner.unwrap_or(0);
        // buf[24] reserved

        let _ = socket.send_to(&buf, addr);
    }

    fn recv_network(&mut self) {
        let socket = match &self.socket {
            Some(s) => s,
            None => return,
        };
        let mut buf = [0u8; 25];

        while let Ok((n, addr)) = socket.recv_from(&mut buf) {
            // Host: first packet from client = connection
            if self.waiting_for_player && self.role == PongRole::Host {
                self.peer_addr = Some(addr);
                self.waiting_for_player = false;
                self.game_active = true;
                // Send initial state back
                self.send_network();
                continue;
            }

            if n < 23 {
                continue;
            }

            let paddle = f32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
            let ball_x = f32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
            let ball_y = f32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
            let ball_dx = f32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
            let ball_dy = f32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);

            match self.role {
                PongRole::Host => {
                    // From client: update right paddle only
                    self.paddle_right_y = paddle;
                }
                PongRole::Client => {
                    // From host: update everything
                    self.paddle_left_y = paddle;
                    self.ball_x = ball_x;
                    self.ball_y = ball_y;
                    self.ball_dx = ball_dx;
                    self.ball_dy = ball_dy;
                    self.score_left = buf[20];
                    self.score_right = buf[21];
                    self.game_active = buf[22] != 0;
                    if n >= 24 && buf[23] != 0 {
                        self.winner = Some(buf[23]);
                    }
                }
            }
        }
    }

    pub fn move_up(&mut self) {
        let paddle = match self.role {
            PongRole::Host => &mut self.paddle_left_y,
            PongRole::Client => &mut self.paddle_right_y,
        };
        *paddle = (*paddle - 2.0).max(3.0);
    }

    pub fn move_down(&mut self) {
        let paddle = match self.role {
            PongRole::Host => &mut self.paddle_left_y,
            PongRole::Client => &mut self.paddle_right_y,
        };
        *paddle = (*paddle + 2.0).min(self.height - 4.0);
    }

    fn reset_ball(&mut self) {
        self.ball_x = self.width / 2.0;
        self.ball_y = self.height / 2.0;
        self.ball_dx = if self.frame % 2 == 0 { 0.5 } else { -0.5 };
        self.ball_dy = 0.3;
    }

    pub fn restart(&mut self) {
        self.ball_x = self.width / 2.0;
        self.ball_y = self.height / 2.0;
        self.ball_dx = 0.5;
        self.ball_dy = 0.3;
        self.paddle_left_y = self.height / 2.0;
        self.paddle_right_y = self.height / 2.0;
        self.score_left = 0;
        self.score_right = 0;
        self.game_active = true;
        self.winner = None;
    }

    pub fn update_dimensions(&mut self, width: u16, height: u16) {
        self.width = width as f32;
        self.height = height as f32;
    }
}

// ── Render ───────────────────────────────────────────────────────────────────

/// Busca IP da mesh WireGuard (10.254.*) via `ip addr` ou `ifconfig`
fn get_mesh_ip() -> Option<String> {
    // Tenta `ip addr show` (Linux)
    if let Ok(output) = Command::new("ip").args(["addr", "show"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("inet ") && trimmed.contains("10.254.") {
                // "inet 10.254.1.2/24 ..." → extrair IP
                if let Some(ip) = trimmed.split_whitespace().nth(1) {
                    if let Some(ip) = ip.split('/').next() {
                        return Some(ip.to_string());
                    }
                }
            }
        }
    }

    // Fallback: tenta conectar num IP da mesh pra descobrir o local
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("10.254.0.1:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                let ip = addr.ip().to_string();
                if ip.starts_with("10.254.") {
                    return Some(ip);
                }
            }
        }
    }

    None
}

pub fn draw_pong(frame: &mut Frame, area: Rect, game: &PongGame) {
    let buf = frame.buffer_mut();

    // Clear with dark green background
    for row in area.top()..area.bottom() {
        for col in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((col, row)) {
                cell.set_char(' ');
                cell.set_style(Style::default().bg(BG));
            }
        }
    }

    let w = area.width;
    let h = area.height;

    // Center net (dashed vertical line)
    let center_x = area.x + w / 2;
    for row in area.top()..area.bottom() {
        if (row - area.top()) % 2 == 0 {
            put_char(buf, area, w / 2, row.saturating_sub(area.y), '\u{00a6}', NET_COLOR);
        }
    }

    // Score display at top center
    let score_str = format!("{}  -  {}", game.score_left, game.score_right);
    let score_x = w / 2 - score_str.len() as u16 / 2;
    put_str(buf, area, score_x, 1, &score_str, SCORE_COLOR);

    // Role indicator
    let role_str = match game.role {
        PongRole::Host => "HOST (Left)",
        PongRole::Client => "CLIENT (Right)",
    };
    let role_x = w / 2 - role_str.len() as u16 / 2;
    put_str(buf, area, role_x, 2, role_str, Color::Rgb(120, 120, 150));

    // Left paddle (5 chars tall)
    let left_y = game.paddle_left_y as u16;
    for dy in 0..5u16 {
        let py = left_y.saturating_sub(2) + dy;
        put_char(buf, area, 2, py, '\u{2588}', PADDLE_COLOR);
        put_char(buf, area, 3, py, '\u{2588}', PADDLE_COLOR);
    }

    // Right paddle (5 chars tall)
    let right_y = game.paddle_right_y as u16;
    for dy in 0..5u16 {
        let py = right_y.saturating_sub(2) + dy;
        put_char(buf, area, w.saturating_sub(4), py, '\u{2588}', PADDLE_COLOR);
        put_char(buf, area, w.saturating_sub(3), py, '\u{2588}', PADDLE_COLOR);
    }

    // Ball
    if game.game_active {
        let bx = game.ball_x as u16;
        let by = game.ball_y as u16;
        put_char(buf, area, bx, by, '\u{25cf}', BALL_COLOR);
    }

    // Waiting for player overlay
    if game.waiting_for_player {
        let cy = h / 2;
        let msg1 = "Waiting for player...";
        let msg2 = format!("Listening on port {}", PONG_PORT);
        let msg3 = "Share your mesh IP with your opponent";
        put_str_centered(buf, area, area.y + cy - 1, msg1, WAITING_COLOR);
        put_str_centered(buf, area, area.y + cy + 1, &msg2, WAITING_COLOR);
        put_str_centered(buf, area, area.y + cy + 3, msg3, Color::Rgb(150, 150, 100));

        // Pega o IP da mesh WireGuard (10.254.*)
        let mesh_ip = get_mesh_ip().unwrap_or_else(|| "unknown".to_string());
        let ip_msg = format!("Your IP: {}:{}", mesh_ip, PONG_PORT);
        put_str_centered(buf, area, area.y + cy + 5, &ip_msg, Color::Rgb(100, 255, 100));
        put_str_centered(buf, area, area.y + cy + 7, &format!(":pong {}", mesh_ip), Color::Rgb(150, 150, 150));
    }

    // Winner overlay
    if let Some(winner) = game.winner {
        let cy = h / 2;
        let winner_str = if winner == 1 {
            "Player 1 (Left) Wins!"
        } else {
            "Player 2 (Right) Wins!"
        };
        let border = "\u{2550}".repeat(winner_str.len() + 4);

        put_str_centered(buf, area, area.y + cy - 2, &border, WIN_COLOR);
        put_str_centered(buf, area, area.y + cy, winner_str, WIN_COLOR);
        put_str_centered(buf, area, area.y + cy + 2, &border, WIN_COLOR);
        put_str_centered(
            buf,
            area,
            area.y + cy + 4,
            "Press R to restart or Esc to quit",
            SCORE_COLOR,
        );
    }

    // Avoid unused variable warning
    let _ = center_x;
}

pub fn draw_pong_statusbar(game: &PongGame) -> Line<'static> {
    let role_str = match game.role {
        PongRole::Host => "Host",
        PongRole::Client => "Client",
    };
    Line::from(vec![
        Span::styled(
            " PONG ",
            Style::default()
                .fg(Color::Rgb(17, 17, 27))
                .bg(Color::Rgb(100, 255, 100))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} - {} ", game.score_left, game.score_right),
            Style::default()
                .fg(SCORE_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} ", role_str),
            Style::default().fg(Color::Rgb(148, 226, 213)),
        ),
        Span::styled(
            " \u{2191}\u{2193}/WS ",
            Style::default()
                .fg(Color::Rgb(137, 180, 250))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Move ", Style::default().fg(Color::Rgb(205, 214, 244))),
        Span::styled(
            " R ",
            Style::default()
                .fg(Color::Rgb(137, 180, 250))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Restart ",
            Style::default().fg(Color::Rgb(205, 214, 244)),
        ),
        Span::styled(
            " Esc ",
            Style::default()
                .fg(Color::Rgb(137, 180, 250))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Quit", Style::default().fg(Color::Rgb(205, 214, 244))),
    ])
}

// ── Render helpers ───────────────────────────────────────────────────────────

fn put_char(buf: &mut Buffer, area: Rect, x: u16, y: u16, ch: char, color: Color) {
    let ax = area.x + x;
    let ay = area.y + y;
    if ax < area.right() && ay < area.bottom() && ax >= area.left() && ay >= area.top() {
        if let Some(cell) = buf.cell_mut((ax, ay)) {
            cell.set_char(ch);
            cell.set_style(Style::default().fg(color).bg(BG));
        }
    }
}

fn put_str(buf: &mut Buffer, area: Rect, x: u16, y: u16, s: &str, color: Color) {
    for (i, ch) in s.chars().enumerate() {
        put_char(buf, area, x + i as u16, y, ch, color);
    }
}

fn put_str_centered(buf: &mut Buffer, area: Rect, y: u16, s: &str, color: Color) {
    let x = area.width.saturating_sub(s.len() as u16) / 2;
    put_str(buf, area, x, y.saturating_sub(area.y), s, color);
}
