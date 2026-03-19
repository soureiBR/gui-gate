use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::{Category, Server};

/// Base64 decode simples (sem crate extra)
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in input.as_bytes() {
        if b == b'=' { break; }
        let val = TABLE.iter().position(|&c| c == b)? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Dados da sessão persistidos em JSON
#[derive(Serialize, Deserialize)]
struct SessionData {
    access_token: String,
    refresh_token: String,
}

/// Client da Gate API — JWT na RAM, cache em ~/.config/soureigate/.session
pub struct ApiClient {
    base_url: String,
    jwt: String,
    refresh_token: String,
    http: reqwest::blocking::Client,
}

// ── Session cache ─────────────────────────────────────────────────────────────

fn session_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("soureigate").join(".session"))
}

fn save_session(access: &str, refresh: &str) {
    if let Some(path) = session_path() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let data = SessionData {
            access_token: access.to_string(),
            refresh_token: refresh.to_string(),
        };
        if let Ok(json) = serde_json::to_string(&data) {
            if std::fs::write(&path, json).is_ok() {
                // chmod 600 — so o dono le/escreve
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).ok();
                }
            }
        }
    }
}

/// Carrega sessao salva. Compativel com formato antigo (JWT puro) e novo (JSON).
fn load_session() -> Option<SessionData> {
    let path = session_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    let content = content.trim().to_string();
    if content.is_empty() { return None; }

    // Tenta parsear como JSON (formato novo)
    if let Ok(data) = serde_json::from_str::<SessionData>(&content) {
        return Some(data);
    }

    // Fallback: formato antigo (JWT puro, sem refresh token)
    Some(SessionData {
        access_token: content,
        refresh_token: String::new(),
    })
}

fn clear_session() {
    if let Some(path) = session_path() {
        std::fs::remove_file(&path).ok();
    }
}

// ── Respostas da API ──────────────────────────────────────────────────────────

/// Mapeamento exato da API (API_DOCS.md)
///
/// GET /api/hosts retorna:
///   id, hostname, alias, host_type (pve|bm|pbs|monitor),
///   ip_mesh, ip_public, port_ssh, ssh_user, status, ...
///
/// GET /api/vms retorna:
///   id, name, alias, ip, port_ssh, ssh_user, host_id, host_name, ...
#[derive(Deserialize)]
struct ApiHost {
    #[serde(default, deserialize_with = "de_to_string")]
    _id: String,
    #[serde(default, deserialize_with = "de_to_string")]
    alias: String,
    #[serde(default, deserialize_with = "de_to_string")]
    hostname: String,
    #[serde(default, deserialize_with = "de_to_string")]
    host_type: String,
    #[serde(default, deserialize_with = "de_to_string")]
    ip_mesh: String,
    #[serde(default, deserialize_with = "de_to_string")]
    ip_public: String,
    #[serde(default)]
    port_ssh: Option<u16>,
    #[serde(default, deserialize_with = "de_to_string")]
    ssh_user: String,
    #[serde(default, deserialize_with = "de_to_string")]
    status: String,
    #[serde(default, deserialize_with = "de_to_string")]
    wg_status: String,
    #[serde(default, deserialize_with = "de_to_string")]
    zabbix_status: String,
    #[serde(default, deserialize_with = "de_to_string")]
    fluentbit_status: String,
    #[serde(default, deserialize_with = "de_to_string")]
    subnet: String,
    #[serde(flatten)]
    _extra: serde_json::Value,
}

#[derive(Deserialize)]
struct ApiVm {
    #[serde(default, deserialize_with = "de_to_string")]
    _id: String,
    #[serde(default, deserialize_with = "de_to_string")]
    name: String,
    #[serde(default, deserialize_with = "de_to_string")]
    alias: String,
    #[serde(default, deserialize_with = "de_to_string")]
    ip: String,
    #[serde(default)]
    port_ssh: Option<u16>,
    #[serde(default, deserialize_with = "de_to_string")]
    ssh_user: String,
    #[serde(default, deserialize_with = "de_to_string")]
    host_name: String,
    #[serde(flatten)]
    _extra: serde_json::Value,
}

/// Desserializa int/string/null/bool → String
fn de_to_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v: serde_json::Value = serde::Deserialize::deserialize(d)?;
    Ok(val_to_string(&v))
}


fn val_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

// ── Refresh token ────────────────────────────────────────────────────────────

fn try_refresh(http: &reqwest::blocking::Client, base_url: &str, refresh_token: &str) -> Result<SessionData, Box<dyn std::error::Error>> {
    let url = format!("{}/api/auth/refresh", base_url);
    let body = serde_json::json!({ "refresh_token": refresh_token });
    let resp = http.post(&url)
        .json(&body)
        .send()?;

    if !resp.status().is_success() {
        return Err("Refresh failed".into());
    }

    let json: serde_json::Value = resp.json()?;
    let access = json.get("access_token")
        .or_else(|| json.get("token"))
        .and_then(|v| v.as_str())
        .ok_or("No access_token in refresh response")?
        .to_string();
    let refresh = json.get("refresh_token")
        .and_then(|v| v.as_str())
        .ok_or("No refresh_token in refresh response")?
        .to_string();

    Ok(SessionData { access_token: access, refresh_token: refresh })
}

impl ApiClient {
    fn build_http() -> Result<reqwest::blocking::Client, reqwest::Error> {
        reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(30))
            .build()
    }

    /// Tenta retomar sessao salva. Se expirada, tenta refresh. Se falhar, faz passkey login.
    pub fn login(base_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let http = Self::build_http()?;

        // Testa se a API ta acessivel
        let status_url = format!("{}/api/status", base_url);
        http.get(&status_url).send().map_err(|e| {
            format!("API inacessivel em {}: {}", base_url, e)
        })?;

        // Tenta reusar sessao salva
        if let Some(session) = load_session() {
            // Tenta access token
            let test_url = format!("{}/api/hosts", base_url);
            let resp = http.get(&test_url)
                .header("Authorization", format!("Bearer {}", session.access_token))
                .send();

            if let Ok(r) = resp {
                if r.status().is_success() {
                    eprintln!("\u{2713} Sessao valida\n");
                    return Ok(Self {
                        base_url: base_url.to_string(),
                        jwt: session.access_token,
                        refresh_token: session.refresh_token,
                        http,
                    });
                }
            }

            // Access token expirado -> tenta refresh
            if !session.refresh_token.is_empty() {
                eprintln!("Token expirado, renovando...");
                if let Ok(new_session) = try_refresh(&http, base_url, &session.refresh_token) {
                    save_session(&new_session.access_token, &new_session.refresh_token);
                    eprintln!("\u{2713} Token renovado\n");
                    return Ok(Self {
                        base_url: base_url.to_string(),
                        jwt: new_session.access_token,
                        refresh_token: new_session.refresh_token,
                        http,
                    });
                }
            }

            // Refresh tambem falhou
            clear_session();
            eprintln!("Sessao expirada, autenticando novamente...\n");
        }

        // Passkey login via browser
        let (jwt, refresh) = passkey_auth(base_url)?;
        save_session(&jwt, &refresh);

        Ok(Self {
            base_url: base_url.to_string(),
            jwt,
            refresh_token: refresh,
            http,
        })
    }

    /// Busca hosts e VMs da API e monta as categorias
    pub fn fetch_categories(&self) -> Result<Vec<Category>, Box<dyn std::error::Error>> {
        let mut categories: Vec<Category> = Vec::new();

        // Busca hosts
        let hosts_url = format!("{}/api/hosts", self.base_url);
        let resp = self.http.get(&hosts_url)
            .header("Authorization", format!("Bearer {}", self.jwt))
            .send()?;

        if resp.status().is_success() {
            let hosts: Vec<ApiHost> = resp.json()?;
            categorize_hosts(&hosts, &mut categories);
        }

        // Busca VMs
        let vms_url = format!("{}/api/vms", self.base_url);
        let resp = self.http.get(&vms_url)
            .header("Authorization", format!("Bearer {}", self.jwt))
            .send()?;

        if resp.status().is_success() {
            let vms: Vec<ApiVm> = resp.json()?;
            if !vms.is_empty() {
                categorize_vms(&vms, &mut categories);
            }
        }

        if categories.is_empty() {
            return Err("Nenhum host ou VM encontrado na API".into());
        }

        Ok(categories)
    }

    /// Extrai admin_id do JWT (decodifica payload base64)
    pub fn admin_id(&self) -> Option<String> {
        let parts: Vec<&str> = self.jwt.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        // Decode base64url payload
        let payload = parts[1];
        let padded = match payload.len() % 4 {
            2 => format!("{}==", payload),
            3 => format!("{}=", payload),
            _ => payload.to_string(),
        };
        let decoded = padded.replace('-', "+").replace('_', "/");
        let bytes = base64_decode(&decoded)?;
        let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;

        // Procura id em vários campos comuns de JWT
        json.get("sub")
            .or_else(|| json.get("admin_id"))
            .or_else(|| json.get("id"))
            .or_else(|| json.get("user_id"))
            .map(|v| match v {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
    }

    /// Baixa a chave SSH do admin logado e salva em arquivo temporário
    pub fn fetch_and_save_ssh_key(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let admin_id = self.admin_id()
            .ok_or("Não foi possível extrair admin_id do JWT")?;

        let url = format!("{}/api/admins/{}/ssh-key", self.base_url, admin_id);
        let resp = self.http.get(&url)
            .header("Authorization", format!("Bearer {}", self.jwt))
            .send()?;

        if !resp.status().is_success() {
            return Err(format!("Erro ao buscar SSH key: HTTP {}", resp.status()).into());
        }

        let key_content = resp.text()?;
        if key_content.is_empty() || !key_content.contains("PRIVATE KEY") {
            return Err("SSH key inválida ou vazia".into());
        }

        // Salva em ~/.config/soureigate/.ssh_key (temporário, chmod 600)
        let key_dir = dirs::config_dir()
            .ok_or("Config dir não encontrado")?
            .join("soureigate");
        std::fs::create_dir_all(&key_dir)?;

        let key_path = key_dir.join(".ssh_key");
        std::fs::write(&key_path, &key_content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
        }

        Ok(key_path)
    }

    /// Logout — invalida JWT no servidor + limpa tudo localmente
    pub fn logout(&self) {
        let url = format!("{}/api/auth/logout", self.base_url);
        let _ = self.http.post(&url)
            .header("Authorization", format!("Bearer {}", self.jwt))
            .send();
        clear_session();
        // Remove SSH key temporária
        if let Some(dir) = dirs::config_dir() {
            let key_path = dir.join("soureigate").join(".ssh_key");
            std::fs::remove_file(&key_path).ok();
        }
    }

    /// Refresh — recarrega hosts/VMs da API
    pub fn refresh_categories(&self) -> Result<Vec<Category>, Box<dyn std::error::Error>> {
        self.fetch_categories()
    }

    /// Tenta renovar o access token usando o refresh token.
    /// Retorna true se conseguiu, false se falhou.
    pub fn auto_refresh(&mut self) -> bool {
        if let Ok(new_session) = try_refresh(&self.http, &self.base_url, &self.refresh_token) {
            save_session(&new_session.access_token, &new_session.refresh_token);
            self.jwt = new_session.access_token;
            self.refresh_token = new_session.refresh_token;
            true
        } else {
            false
        }
    }

    /// Verifica se a API esta online (ping /api/status)
    pub fn check_api_status(&self) -> bool {
        let url = format!("{}/api/status", self.base_url);
        self.http.get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

}

// ── Passkey auth via browser ──────────────────────────────────────────────────

fn passkey_auth(base_url: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    // Abre um mini HTTP server em porta aleatoria
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();

    // Timeout de 2 minutos pro user fazer o passkey
    listener.set_nonblocking(false)?;

    // Abre o browser
    let auth_url = format!("{}/cli-auth.html?port={}", base_url, port);
    eprintln!("Abrindo browser para autenticacao...");
    eprintln!("   URL: {}", auth_url);
    eprintln!("   Aguardando passkey...\n");

    open::that(&auth_url).map_err(|e| {
        format!("Nao foi possivel abrir o browser: {}. Acesse manualmente: {}", e, auth_url)
    })?;

    // Espera o callback com timeout
    listener.set_nonblocking(false)?;
    let (jwt, refresh) = wait_for_callback(listener)?;

    eprintln!("\u{2713} Autenticado com sucesso!\n");
    Ok((jwt, refresh))
}

fn wait_for_callback(listener: TcpListener) -> Result<(String, String), Box<dyn std::error::Error>> {
    // Timeout de 120s
    listener
        .set_nonblocking(false)
        .ok();

    // Aceita conexoes ate receber o JWT
    loop {
        let (mut stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;

        let mut buf = [0u8; 8192];
        let n = match stream.read(&mut buf) {
            Ok(n) => n,
            Err(_) => continue,
        };

        let request = String::from_utf8_lossy(&buf[..n]);
        let first_line = request.lines().next().unwrap_or("");

        // Responde OPTIONS (CORS preflight)
        if first_line.starts_with("OPTIONS") {
            let cors_response = "HTTP/1.1 204 No Content\r\n\
                Access-Control-Allow-Origin: *\r\n\
                Access-Control-Allow-Methods: GET, OPTIONS\r\n\
                Access-Control-Allow-Headers: *\r\n\
                Content-Length: 0\r\n\r\n";
            stream.write_all(cors_response.as_bytes()).ok();
            continue;
        }

        // Procura jwt= e refresh_token= no query string do GET
        if let Some((jwt, refresh)) = extract_tokens_from_request(first_line) {
            let ok_response = "HTTP/1.1 200 OK\r\n\
                Access-Control-Allow-Origin: *\r\n\
                Content-Type: text/plain\r\n\
                Content-Length: 2\r\n\r\nOK";
            stream.write_all(ok_response.as_bytes()).ok();
            return Ok((jwt, refresh));
        }

        // Request sem JWT — responde 400
        let bad_response = "HTTP/1.1 400 Bad Request\r\n\
            Access-Control-Allow-Origin: *\r\n\
            Content-Type: text/plain\r\n\
            Content-Length: 6\r\n\r\nno jwt";
        stream.write_all(bad_response.as_bytes()).ok();
    }
}

/// Extrai jwt e refresh_token do query string do callback.
/// Retorna (access_token, refresh_token). refresh_token pode ser vazio se nao enviado.
fn extract_tokens_from_request(request_line: &str) -> Option<(String, String)> {
    // GET /callback?jwt=xxx&refresh_token=yyy HTTP/1.1
    let path = request_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;

    let mut jwt = None;
    let mut refresh = String::new();

    for param in query.split('&') {
        let mut kv = param.splitn(2, '=');
        let key = kv.next()?;
        let val = kv.next().unwrap_or("");
        match key {
            "jwt" if !val.is_empty() => jwt = Some(val.to_string()),
            "refresh_token" if !val.is_empty() => refresh = val.to_string(),
            _ => {}
        }
    }

    jwt.map(|t| (t, refresh))
}

// ── Categorização de hosts ────────────────────────────────────────────────────

fn categorize_hosts(hosts: &[ApiHost], categories: &mut Vec<Category>) {
    let mut pve: Vec<Server> = Vec::new();
    let mut bm: Vec<Server> = Vec::new();
    let mut pbs: Vec<Server> = Vec::new();
    let mut monitor: Vec<Server> = Vec::new();
    let mut other: Vec<Server> = Vec::new();

    for host in hosts {
        let server = Server {
            name: if !host.alias.is_empty() {
                host.alias.clone()
            } else if !host.hostname.is_empty() {
                host.hostname.clone()
            } else {
                host.ip_mesh.clone()
            },
            host: host.ip_mesh.clone(),
            port: host.port_ssh.unwrap_or(22),
            user: if host.ssh_user.is_empty() { "root".into() } else { host.ssh_user.clone() },
            ip_public: host.ip_public.clone(),
            hostname: host.hostname.clone(),
            host_type: host.host_type.clone(),
            status: host.status.clone(),
            wg_status: host.wg_status.clone(),
            zabbix_status: host.zabbix_status.clone(),
            fluentbit_status: host.fluentbit_status.clone(),
            subnet: host.subnet.clone(),
            host_name: String::new(),
        };

        // Categoriza pelo host_type (API retorna: pve, bm, pbs, monitor)
        let type_str = host.host_type.as_str();
        let alias_lower = host.alias.to_lowercase();

        match type_str.to_lowercase().as_str() {
            "pve" | "proxmox" => pve.push(server),
            "bm" | "baremetal" | "bare_metal" => bm.push(server),
            "pbs" | "backup" => pbs.push(server),
            "monitor" | "monitoring" | "zabbix" => monitor.push(server),
            _ => {
                // Fallback: tenta adivinhar pelo alias
                if alias_lower.contains("pve") || alias_lower.contains("proxmox") {
                    pve.push(server);
                } else if alias_lower.contains("bm-") || alias_lower.contains("baremetal") {
                    bm.push(server);
                } else if alias_lower.contains("pbs") || alias_lower.contains("backup") {
                    pbs.push(server);
                } else if alias_lower.contains("mon") || alias_lower.contains("zbx") {
                    monitor.push(server);
                } else {
                    other.push(server);
                }
            }
        }
    }

    if !pve.is_empty() {
        categories.push(Category { name: "PVE Nodes".into(), icon: String::new(), servers: pve });
    }
    if !bm.is_empty() {
        categories.push(Category { name: "Baremetals".into(), icon: String::new(), servers: bm });
    }
    if !pbs.is_empty() {
        categories.push(Category { name: "PBS".into(), icon: String::new(), servers: pbs });
    }
    if !monitor.is_empty() {
        categories.push(Category { name: "Monitor".into(), icon: String::new(), servers: monitor });
    }
    if !other.is_empty() {
        categories.push(Category { name: "Hosts".into(), icon: String::new(), servers: other });
    }
}

// ── Categorização de VMs por host ─────────────────────────────────────────

fn categorize_vms(vms: &[ApiVm], categories: &mut Vec<Category>) {
    use std::collections::BTreeMap;

    // Agrupa VMs por host_name (BTreeMap pra ordenar alfabeticamente)
    let mut by_host: BTreeMap<String, Vec<Server>> = BTreeMap::new();

    for vm in vms {
        let server = Server {
            name: if !vm.alias.is_empty() {
                vm.alias.clone()
            } else if !vm.name.is_empty() {
                vm.name.clone()
            } else {
                vm.ip.clone()
            },
            host: vm.ip.clone(),
            port: vm.port_ssh.unwrap_or(22),
            user: if vm.ssh_user.is_empty() { "root".into() } else { vm.ssh_user.clone() },
            ip_public: String::new(),
            hostname: vm.name.clone(),
            host_type: "vm".into(),
            status: String::new(),
            wg_status: String::new(),
            zabbix_status: String::new(),
            fluentbit_status: String::new(),
            subnet: String::new(),
            host_name: vm.host_name.clone(),
        };

        let group = if vm.host_name.is_empty() {
            "Outros".to_string()
        } else {
            vm.host_name.clone()
        };

        by_host.entry(group).or_default().push(server);
    }

    // Cria uma categoria pra cada host, com "Outros" por último
    let mut others = None;
    for (host_name, servers) in &by_host {
        if host_name == "Outros" {
            others = Some(servers.clone());
            continue;
        }
        categories.push(Category {
            name: format!("VMs > {}", host_name),
            icon: String::new(),
            servers: servers.clone(),
        });
    }

    // "Outros" no final
    if let Some(servers) = others {
        categories.push(Category {
            name: "VMs > Outros".into(),
            icon: String::new(),
            servers,
        });
    }
}
