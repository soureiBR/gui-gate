use std::borrow::Cow;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use alacritty_terminal::event::{Event, EventListener, Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::tty::{Options as TtyOptions, Shell};

use crate::config::Server;

/// EventProxy que detecta Exit/ChildExit e seta o flag is_dead
#[derive(Clone)]
pub struct TermEventProxy {
    is_dead: Arc<AtomicBool>,
}

impl TermEventProxy {
    fn new(is_dead: Arc<AtomicBool>) -> Self {
        Self { is_dead }
    }
}

impl EventListener for TermEventProxy {
    fn send_event(&self, event: Event) {
        match event {
            Event::Exit | Event::ChildExit(_) => {
                self.is_dead.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

/// Dimensões simples para Term::new
pub struct TermSize {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize { self.rows }
    fn screen_lines(&self) -> usize { self.rows }
    fn columns(&self) -> usize { self.cols }
}

/// Uma sessão SSH rodando dentro do TUI
pub struct TerminalSession {
    pub name: String,
    pub term: Arc<FairMutex<Term<TermEventProxy>>>,
    pub notifier: Notifier,
    pub is_dead: Arc<AtomicBool>,
    pub connected_at: std::time::Instant,
    pub server_host: String,
    pub server_port: u16,
    pub server_user: String,
}

impl TerminalSession {
    pub fn new(server: &Server, key_path: &str, cols: u16, rows: u16) -> io::Result<Self> {
        let is_dead = Arc::new(AtomicBool::new(false));

        let window_size = WindowSize {
            num_cols: cols,
            num_lines: rows,
            cell_width: 1,
            cell_height: 1,
        };

        let mut ssh_args = vec![
            "-p".to_string(),
            server.port.to_string(),
        ];

        // Só adiciona -i se a chave existir
        if !key_path.is_empty() && std::path::Path::new(key_path).exists() {
            ssh_args.push("-i".to_string());
            ssh_args.push(key_path.to_string());
        }

        ssh_args.push(format!("{}@{}", server.user, server.host));

        let tty_options = TtyOptions {
            shell: Some(Shell::new("ssh".to_string(), ssh_args)),
            working_directory: None,
            drain_on_exit: true,
            env: Default::default(),
            #[cfg(windows)]
            escape_args: false,
        };

        let proxy = TermEventProxy::new(is_dead.clone());
        let config = TermConfig::default();
        let size = TermSize { cols: cols as usize, rows: rows as usize };
        let term = Term::new(config, &size, proxy.clone());
        let term = Arc::new(FairMutex::new(term));

        let pty = alacritty_terminal::tty::new(&tty_options, window_size, 0)?;
        let event_loop = EventLoop::new(term.clone(), proxy, pty, false, false)?;
        let notifier = Notifier(event_loop.channel());

        // Roda o event loop — quando o SSH encerra, dispara Event::ChildExit
        std::thread::spawn(move || {
            event_loop.spawn();
        });

        Ok(TerminalSession {
            name: server.name.clone(),
            term,
            notifier,
            is_dead,
            connected_at: std::time::Instant::now(),
            server_host: server.host.clone(),
            server_port: server.port,
            server_user: server.user.clone(),
        })
    }

    pub fn write_input(&mut self, bytes: impl Into<Cow<'static, [u8]>>) {
        self.notifier.notify(bytes);
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.notifier.on_resize(WindowSize {
            num_cols: cols,
            num_lines: rows,
            cell_width: 1,
            cell_height: 1,
        });
        let size = TermSize { cols: cols as usize, rows: rows as usize };
        let mut term = self.term.lock();
        term.resize(size);
    }

    pub fn is_dead(&self) -> bool {
        self.is_dead.load(Ordering::Relaxed)
    }

    pub fn scroll_up(&mut self, lines: usize) {
        use alacritty_terminal::grid::Scroll;
        let mut term = self.term.lock();
        term.scroll_display(Scroll::Delta(lines as i32));
    }

    pub fn scroll_down(&mut self, lines: usize) {
        use alacritty_terminal::grid::Scroll;
        let mut term = self.term.lock();
        term.scroll_display(Scroll::Delta(-(lines as i32)));
    }

    pub fn scroll_reset(&mut self) {
        use alacritty_terminal::grid::Scroll;
        let mut term = self.term.lock();
        term.scroll_display(Scroll::Bottom);
    }

    /// Extrai texto por range de coordenadas (col, row) relativas ao viewport
    pub fn copy_selection(&self, start: (u16, u16), end: (u16, u16)) -> String {
        use alacritty_terminal::index::{Column, Line};
        use alacritty_terminal::grid::Dimensions;

        let term = self.term.lock();
        let grid = term.grid();
        let columns = term.columns();
        let display_offset = grid.display_offset() as i32;

        // Normaliza start/end pra que start venha antes
        let (start, end) = if start.1 < end.1 || (start.1 == end.1 && start.0 <= end.0) {
            (start, end)
        } else {
            (end, start)
        };

        let mut result = String::new();

        for row in start.1..=end.1 {
            let line_idx = Line(row as i32 - display_offset);
            let col_start = if row == start.1 { start.0 as usize } else { 0 };
            let col_end = if row == end.1 { end.0 as usize } else { columns.saturating_sub(1) };

            let mut line = String::new();
            for col in col_start..=col_end.min(columns.saturating_sub(1)) {
                let cell = &grid[line_idx][Column(col)];
                if cell.c != '\0' {
                    line.push(cell.c);
                }
            }
            result.push_str(line.trim_end());
            if row < end.1 {
                result.push('\n');
            }
        }

        result
    }

    /// Extrai as últimas N linhas de texto visível do terminal
    pub fn copy_lines(&self, count: usize) -> String {
        use alacritty_terminal::index::{Column, Line};
        use alacritty_terminal::grid::Dimensions;

        let term = self.term.lock();
        let grid = term.grid();
        let screen_lines = term.screen_lines();
        let columns = term.columns();

        let start = if count >= screen_lines { 0 } else { screen_lines - count };
        let mut result = String::new();

        for row in start..screen_lines {
            let mut line = String::new();
            for col in 0..columns {
                let cell = &grid[Line(row as i32)][Column(col)];
                let c = cell.c;
                if c != '\0' {
                    line.push(c);
                }
            }
            // Remove trailing whitespace
            let trimmed = line.trim_end();
            if !trimmed.is_empty() || row < screen_lines - 1 {
                result.push_str(trimmed);
                result.push('\n');
            }
        }

        // Remove trailing empty lines
        while result.ends_with("\n\n") {
            result.pop();
        }

        result
    }

}
