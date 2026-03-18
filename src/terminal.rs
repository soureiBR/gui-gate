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
}
