//! SoureiGate DOOM — Space Invaders Easter Egg
//! Ativado com :doom no command input

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

// ── Cores ─────────────────────────────────────────────────────────────────────

const BG: Color = Color::Rgb(5, 5, 15);
const PLAYER_COLOR: Color = Color::Rgb(100, 255, 100);
const ENEMY_COLOR1: Color = Color::Rgb(255, 80, 80);
const ENEMY_COLOR2: Color = Color::Rgb(255, 150, 50);
const ENEMY_COLOR3: Color = Color::Rgb(200, 100, 255);
const BULLET_COLOR: Color = Color::Rgb(255, 255, 100);
const ENEMY_BULLET: Color = Color::Rgb(255, 50, 50);
const SHIELD_COLOR: Color = Color::Rgb(80, 80, 255);
const TEXT_COLOR: Color = Color::Rgb(200, 200, 220);
const SCORE_COLOR: Color = Color::Rgb(255, 255, 100);
const TITLE_COLOR: Color = Color::Rgb(255, 60, 60);

// ── Estado do jogo ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Enemy {
    pub x: f32,
    pub y: f32,
    pub alive: bool,
    pub tier: u8, // 0=normal, 1=medio, 2=forte
}

#[derive(Clone)]
pub struct Bullet {
    pub x: f32,
    pub y: f32,
    pub is_enemy: bool,
}

#[derive(Clone)]
pub struct Explosion {
    pub x: f32,
    pub y: f32,
    pub frame: u8,
}

#[derive(Clone)]
pub struct Shield {
    pub x: u16,
    pub y: u16,
    pub hp: u8,
}

#[derive(PartialEq, Clone, Copy)]
pub enum GameState {
    Playing,
    GameOver,
    Victory,
}

pub struct DoomGame {
    pub player_x: f32,
    pub lives: u8,
    pub score: u32,
    pub wave: u8,
    pub frame: u64,
    pub enemies: Vec<Enemy>,
    pub bullets: Vec<Bullet>,
    pub explosions: Vec<Explosion>,
    pub shields: Vec<Shield>,
    pub enemy_dir: f32,    // 1.0 = right, -1.0 = left
    pub enemy_speed: f32,
    pub state: GameState,
    pub invincible_frames: u16,
    pub shields_spawned: bool,
}

impl DoomGame {
    pub fn new(width: u16) -> Self {
        let mut game = Self {
            player_x: width as f32 / 2.0,
            lives: 3,
            score: 0,
            wave: 1,
            frame: 0,
            enemies: vec![],
            bullets: vec![],
            explosions: vec![],
            shields: vec![],
            enemy_dir: 1.0,
            enemy_speed: 0.3,
            state: GameState::Playing,
            invincible_frames: 0,
            shields_spawned: false,
        };
        game.spawn_wave(width);
        game
    }

    fn spawn_wave(&mut self, width: u16) {
        self.enemies.clear();
        let cols = ((width / 10) as usize).max(5).min(14);
        let rows = 3 + (self.wave as usize / 2).min(3);
        let spacing = 9.0; // bem espaçados
        let total_w = cols as f32 * spacing;
        let start_x = (width as f32 - total_w) / 2.0 + spacing / 2.0;

        for row in 0..rows {
            for col in 0..cols {
                self.enemies.push(Enemy {
                    x: start_x + col as f32 * spacing,
                    y: 5.0 + row as f32 * 3.0, // mais espaço vertical
                    alive: true,
                    tier: if row == 0 { 2 } else if row <= 1 { 1 } else { 0 },
                });
            }
        }
        self.enemy_speed = 0.3 + self.wave as f32 * 0.1;
    }

    fn spawn_shields(&mut self, width: u16, height: u16) {
        self.shields.clear();
        let count = 8;
        let spacing = width / (count as u16 + 1);
        let shield_y = height.saturating_sub(8); // perto da nave
        for i in 0..count {
            let x = spacing * (i as u16 + 1);
            for dx in 0..9 {
                for dy in 0..3 {
                    self.shields.push(Shield {
                        x: x + dx - 4,
                        y: shield_y + dy,
                        hp: 3,
                    });
                }
            }
        }
    }

    pub fn tick(&mut self, width: u16, height: u16) {
        if self.state != GameState::Playing {
            return;
        }

        // Spawn shields na primeira frame com a altura real da tela
        if !self.shields_spawned {
            self.spawn_shields(width, height);
            self.shields_spawned = true;
        }

        self.frame += 1;

        if self.invincible_frames > 0 {
            self.invincible_frames -= 1;
        }

        // Move bullets — rápidas
        self.bullets.retain_mut(|b| {
            if b.is_enemy {
                b.y += 0.8;
                b.y < height as f32
            } else {
                b.y -= 1.5;
                b.y > 0.0
            }
        });

        // Move enemies
        let mut should_reverse = false;
        for e in &self.enemies {
            if !e.alive { continue; }
            if (e.x + self.enemy_dir * self.enemy_speed) < 2.0
                || (e.x + self.enemy_dir * self.enemy_speed) > width as f32 - 3.0
            {
                should_reverse = true;
                break;
            }
        }

        if should_reverse {
            self.enemy_dir *= -1.0;
            for e in &mut self.enemies {
                if e.alive { e.y += 0.5; }
            }
        } else {
            for e in &mut self.enemies {
                if e.alive { e.x += self.enemy_dir * self.enemy_speed; }
            }
        }

        // Enemy shooting
        if self.frame % (20u64.saturating_sub(self.wave as u64 * 2).max(5)) == 0 {
            let alive: Vec<&Enemy> = self.enemies.iter().filter(|e| e.alive).collect();
            if !alive.is_empty() {
                let idx = (self.frame as usize * 7 + 13) % alive.len();
                self.bullets.push(Bullet {
                    x: alive[idx].x,
                    y: alive[idx].y + 1.0,
                    is_enemy: true,
                });
            }
        }

        // Bullet vs enemy collision
        let mut kills = vec![];
        for (bi, bullet) in self.bullets.iter().enumerate() {
            if bullet.is_enemy { continue; }
            for (ei, enemy) in self.enemies.iter().enumerate() {
                if !enemy.alive { continue; }
                if (bullet.x - enemy.x).abs() < 2.0 && (bullet.y - enemy.y).abs() < 1.0 {
                    kills.push((bi, ei));
                }
            }
        }
        // Remove duplicates and process
        kills.sort_by(|a, b| b.0.cmp(&a.0));
        kills.dedup_by_key(|k| k.0);
        for (bi, ei) in &kills {
            if *bi < self.bullets.len() {
                self.bullets.remove(*bi);
            }
            if *ei < self.enemies.len() && self.enemies[*ei].alive {
                let e = &self.enemies[*ei];
                self.explosions.push(Explosion { x: e.x, y: e.y, frame: 0 });
                self.score += match e.tier {
                    2 => 300,
                    1 => 200,
                    _ => 100,
                };
                self.enemies[*ei].alive = false;
            }
        }

        // Bullet vs shield collision
        self.bullets.retain(|b| {
            let mut hit = false;
            for s in &mut self.shields {
                if s.hp > 0 && (b.x - s.x as f32).abs() < 1.0 && (b.y - s.y as f32).abs() < 1.0 {
                    s.hp -= 1;
                    hit = true;
                    break;
                }
            }
            !hit
        });

        // Enemy bullet vs player
        if self.invincible_frames == 0 {
            self.bullets.retain(|b| {
                if b.is_enemy && (b.x - self.player_x).abs() < 2.0 && b.y >= height as f32 - 5.0 {
                    self.lives = self.lives.saturating_sub(1);
                    self.invincible_frames = 60;
                    if self.lives == 0 {
                        self.state = GameState::GameOver;
                    }
                    false
                } else {
                    true
                }
            });
        }

        // Enemy reaches player
        for e in &self.enemies {
            if e.alive && e.y >= height as f32 - 6.0 {
                self.state = GameState::GameOver;
                break;
            }
        }

        // Explosions decay
        self.explosions.retain_mut(|e| {
            e.frame += 1;
            e.frame < 6
        });

        // Wave clear
        if self.enemies.iter().all(|e| !e.alive) {
            self.wave += 1;
            if self.wave > 10 {
                self.state = GameState::Victory;
            } else {
                self.spawn_wave(width);
            }
        }
    }

    pub fn move_left(&mut self) {
        if self.player_x > 3.0 { self.player_x -= 3.0; }
    }

    pub fn move_right(&mut self, width: u16) {
        if self.player_x < width as f32 - 4.0 { self.player_x += 3.0; }
    }

    pub fn shoot(&mut self, height: u16) {
        // Max 5 bullets at a time
        let player_bullets = self.bullets.iter().filter(|b| !b.is_enemy).count();
        if player_bullets < 5 {
            self.bullets.push(Bullet {
                x: self.player_x,
                y: height as f32 - 5.0,
                is_enemy: false,
            });
        }
    }

    pub fn restart(&mut self, width: u16) {
        *self = Self::new(width);
    }
}

// ── Render ────────────────────────────────────────────────────────────────────

pub fn draw_doom(frame: &mut Frame, area: Rect, game: &DoomGame) {
    let buf = frame.buffer_mut();

    // Clear with dark bg
    for row in area.top()..area.bottom() {
        for col in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((col, row)) {
                cell.set_char(' ');
                cell.set_style(Style::default().bg(BG));
            }
        }
    }

    // Stars background
    for i in 0..30 {
        let sx = ((i * 37 + game.frame as usize * (i % 3 + 1) / 8) % area.width as usize) as u16 + area.x;
        let sy = ((i * 13 + 7) % area.height.saturating_sub(6) as usize) as u16 + area.y;
        if sx < area.right() && sy < area.bottom() {
            if let Some(cell) = buf.cell_mut((sx, sy)) {
                let star = if i % 3 == 0 { '·' } else { '.' };
                cell.set_char(star);
                cell.set_style(Style::default().fg(Color::Rgb(60, 60, 80)).bg(BG));
            }
        }
    }

    let w = area.width;
    let h = area.height;

    // Title
    put_str(buf, area, w / 2 - 11, 0, "☠ SOUREIGATE INVADERS ☠", TITLE_COLOR);

    // HUD
    let lives_str = "♥".repeat(game.lives as usize);
    put_str(buf, area, 2, 2, &format!("SCORE: {:<8} WAVE: {}/10  LIVES: {}", game.score, game.wave, lives_str), SCORE_COLOR);

    // Shields
    for s in &game.shields {
        if s.hp > 0 {
            let x = s.x;
            let y = s.y;
            let ch = match s.hp { 3 => '█', 2 => '▓', 1 => '░', _ => ' ' };
            let color = match s.hp { 3 => SHIELD_COLOR, 2 => Color::Rgb(60, 60, 200), _ => Color::Rgb(40, 40, 150) };
            put_char(buf, area, x, y, ch, color);
        }
    }

    // Enemies
    for e in &game.enemies {
        if !e.alive { continue; }
        let x = e.x as u16;
        let y = e.y as u16;
        let (sprite, color) = match e.tier {
            2 => ("╔███╗", ENEMY_COLOR3),
            1 => ("▄███▄", ENEMY_COLOR2),
            _ => ("▼███▼", ENEMY_COLOR1),
        };
        for (i, ch) in sprite.chars().enumerate() {
            put_char(buf, area, x.saturating_sub(2) + i as u16, y, ch, color);
        }
    }

    // Explosions
    let explosion_sprites = ["*", "✦", "✧", "·", ".", " "];
    for exp in &game.explosions {
        let x = exp.x as u16;
        let y = exp.y as u16;
        let idx = (exp.frame as usize).min(explosion_sprites.len() - 1);
        let ch = explosion_sprites[idx].chars().next().unwrap_or(' ');
        let brightness = 255 - (exp.frame as u8 * 40).min(200);
        put_char(buf, area, x, y, ch, Color::Rgb(brightness, brightness / 2, 0));
        // Explosion spread
        if exp.frame < 3 {
            put_char(buf, area, x + 1, y, ch, Color::Rgb(brightness, brightness / 3, 0));
            put_char(buf, area, x.saturating_sub(1), y, ch, Color::Rgb(brightness, brightness / 3, 0));
        }
    }

    // Bullets
    for b in &game.bullets {
        let x = b.x as u16;
        let y = b.y as u16;
        if b.is_enemy {
            put_char(buf, area, x, y, '▼', ENEMY_BULLET);
        } else {
            put_char(buf, area, x, y, '│', BULLET_COLOR);
            if y > 0 { put_char(buf, area, x, y - 1, '·', BULLET_COLOR); }
        }
    }

    // Player
    let px = game.player_x as u16;
    let py = h.saturating_sub(4);
    let blink = game.invincible_frames > 0 && game.frame % 4 < 2;
    if !blink {
        put_str(buf, area, px.saturating_sub(3), py,     "╱║ ▲ ║╲", PLAYER_COLOR);
        put_str(buf, area, px.saturating_sub(3), py + 1, "╚══╦══╝", PLAYER_COLOR);
    }

    // Game Over / Victory overlay
    match game.state {
        GameState::GameOver => {
            let cy = area.y + h / 2;
            put_str_centered(buf, area, cy - 1, "══════════════════════", TITLE_COLOR);
            put_str_centered(buf, area, cy,     "   ☠  GAME OVER  ☠   ", TITLE_COLOR);
            put_str_centered(buf, area, cy + 1, "══════════════════════", TITLE_COLOR);
            put_str_centered(buf, area, cy + 3, &format!("Final Score: {}", game.score), SCORE_COLOR);
            put_str_centered(buf, area, cy + 5, "Press R to restart or Esc to quit", TEXT_COLOR);
        }
        GameState::Victory => {
            let cy = area.y + h / 2;
            put_str_centered(buf, area, cy - 1, "════════════════════════════", PLAYER_COLOR);
            put_str_centered(buf, area, cy,     "  ★ ALL WAVES CLEARED! ★   ", PLAYER_COLOR);
            put_str_centered(buf, area, cy + 1, "════════════════════════════", PLAYER_COLOR);
            put_str_centered(buf, area, cy + 3, &format!("Final Score: {}", game.score), SCORE_COLOR);
            put_str_centered(buf, area, cy + 5, "Press R to restart or Esc to quit", TEXT_COLOR);
        }
        _ => {}
    }
}

pub fn draw_doom_statusbar(game: &DoomGame) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            " DOOM ",
            Style::default().fg(Color::Rgb(17, 17, 27)).bg(TITLE_COLOR).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" Score: {} ", game.score), Style::default().fg(SCORE_COLOR)),
        Span::styled(format!(" Wave: {}/10 ", game.wave), Style::default().fg(TEXT_COLOR)),
        Span::styled(format!(" Lives: {} ", "♥".repeat(game.lives as usize)), Style::default().fg(TITLE_COLOR)),
        Span::styled(" ← → ", Style::default().fg(Color::Rgb(137, 180, 250)).add_modifier(Modifier::BOLD)),
        Span::styled("Move ", Style::default().fg(TEXT_COLOR)),
        Span::styled(" Space ", Style::default().fg(Color::Rgb(137, 180, 250)).add_modifier(Modifier::BOLD)),
        Span::styled("Shoot ", Style::default().fg(TEXT_COLOR)),
        Span::styled(" Esc ", Style::default().fg(Color::Rgb(137, 180, 250)).add_modifier(Modifier::BOLD)),
        Span::styled("Quit", Style::default().fg(TEXT_COLOR)),
    ])
}

// ── Helpers de render ─────────────────────────────────────────────────────────

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
