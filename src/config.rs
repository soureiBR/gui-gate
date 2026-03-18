use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub api: Option<ApiSettings>,
    #[serde(default)]
    pub categories: Vec<Category>,
    /// Flag interno: dados vieram da API (não exige SSH key local)
    #[serde(skip)]
    pub loaded_from_api: bool,
}

#[derive(Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub ssh_key: String,
}

#[derive(Deserialize, Clone)]
pub struct ApiSettings {
    pub url: String,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct Category {
    pub name: String,
    #[serde(default)]
    pub icon: String,
    pub servers: Vec<Server>,
}

#[derive(Deserialize, Clone)]
pub struct Server {
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_user")]
    pub user: String,
}

fn default_port() -> u16 {
    22
}

fn default_user() -> String {
    "root".into()
}

impl Server {
    pub fn display_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

impl Config {
    /// Carrega config do TOML (pode ter só [api] sem categories)
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = Self::find_config();

        match path {
            Ok(p) => {
                let content = std::fs::read_to_string(&p)?;
                let mut config: Config = toml::from_str(&content)?;
                if !config.settings.ssh_key.is_empty() {
                    config.settings.ssh_key = expand_home(&config.settings.ssh_key);
                }
                Ok(config)
            }
            Err(_) => {
                // Sem config — precisa de setup
                Ok(Config::default())
            }
        }
    }

    /// Cria config a partir dos dados da API
    pub fn from_api(categories: Vec<Category>, ssh_key: String) -> Self {
        Config {
            settings: Settings { ssh_key },
            api: None,
            categories,
            loaded_from_api: true,
        }
    }

    /// Verifica se deve usar modo API
    pub fn is_api_mode(&self) -> bool {
        self.api.is_some()
    }

    /// Precisa de setup (sem config e sem API)
    pub fn needs_setup(&self) -> bool {
        self.api.is_none() && self.categories.is_empty()
    }

    /// Configura a URL da API
    pub fn setup_api(&mut self, url: &str) {
        // Remove trailing slash
        let url = url.trim_end_matches('/').to_string();
        self.api = Some(ApiSettings { url });
    }

    /// Salva config em ~/.config/soureigate/servers.toml
    pub fn save_to_config_dir(&self) -> Result<(), Box<dyn std::error::Error>> {
        let dir = dirs::config_dir()
            .ok_or("Não foi possível encontrar config dir")?
            .join("soureigate");
        std::fs::create_dir_all(&dir)?;

        let path = dir.join("servers.toml");
        let content = if let Some(ref api) = self.api {
            format!(
                "# SoureiGate — Configuração\n\
                 # Gerado automaticamente\n\n\
                 [api]\n\
                 url = \"{}\"\n\n\
                 [settings]\n\
                 # ssh_key = \"~/.ssh/id_sourei_IKI\"\n",
                api.url
            )
        } else {
            String::new()
        };
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// URL da API (se configurada)
    pub fn api_url(&self) -> Option<&str> {
        self.api.as_ref().map(|a| a.url.as_str())
    }

    fn find_config() -> Result<PathBuf, Box<dyn std::error::Error>> {
        // 1. CLI arg
        if let Some(arg) = std::env::args().nth(1) {
            let p = PathBuf::from(&arg);
            if p.exists() {
                return Ok(p);
            }
        }

        // 2. Next to the binary
        if let Ok(exe) = std::env::current_exe() {
            let p = exe.parent().unwrap_or(Path::new(".")).join("servers.toml");
            if p.exists() {
                return Ok(p);
            }
        }

        // 3. Current directory
        let p = PathBuf::from("servers.toml");
        if p.exists() {
            return Ok(p);
        }

        // 4. ~/.config/soureigate/servers.toml
        if let Some(config_dir) = dirs::config_dir() {
            let p = config_dir.join("soureigate").join("servers.toml");
            if p.exists() {
                return Ok(p);
            }
        }

        Err("servers.toml not found".into())
    }
}

fn expand_home(path: &str) -> String {
    if path.starts_with("$HOME") || path.starts_with("~") {
        if let Some(home) = dirs::home_dir() {
            let home_str = home.to_string_lossy();
            return path
                .replace("$HOME", &home_str)
                .replace("~", &home_str);
        }
    }
    path.to_string()
}
