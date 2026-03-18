use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

const REPO: &str = "soureiBR/gui-gate";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Info de uma release do GitHub
struct ReleaseInfo {
    _tag: String,
    version: String,
    assets: Vec<AssetInfo>,
}

struct AssetInfo {
    name: String,
    download_url: String,
}

/// Detecta o nome do asset correto pra este OS/arch
fn asset_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "gate-windows-amd64.exe"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "gate-macos-arm64"
        } else {
            "gate-macos-amd64"
        }
    } else {
        "gate-linux-amd64"
    }
}

/// Compara versões semver simples (ex: "1.0.0" < "1.1.0")
fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };
    let r = parse(remote);
    let l = parse(local);
    r > l
}

/// Busca a release mais recente do GitHub
fn fetch_latest_release() -> Result<ReleaseInfo, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(10))
        .user_agent("gate-updater")
        .build()?;

    let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);
    let resp = client.get(&url).send()?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API: {}", resp.status()).into());
    }

    let json: serde_json::Value = resp.json()?;

    let tag = json["tag_name"]
        .as_str()
        .ok_or("tag_name não encontrado")?
        .to_string();

    let version = tag.trim_start_matches('v').to_string();

    let assets = json["assets"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    Some(AssetInfo {
                        name: a["name"].as_str()?.to_string(),
                        download_url: a["browser_download_url"].as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(ReleaseInfo { _tag: tag, version, assets })
}

/// Checa se tem update disponível (não bloqueia se falhar)
pub fn check_update_quiet() {
    // Roda em background pra não atrasar o startup
    let release = match fetch_latest_release() {
        Ok(r) => r,
        Err(_) => return, // Offline ou erro — ignora silenciosamente
    };

    if is_newer(&release.version, CURRENT_VERSION) {
        eprintln!(
            "╔══════════════════════════════════════════════╗"
        );
        eprintln!(
            "║  Nova versão disponível: v{} → v{}",
            CURRENT_VERSION,
            release.version
        );
        eprintln!(
            "║  Execute: gate --update                      ║"
        );
        eprintln!(
            "╚══════════════════════════════════════════════╝"
        );
        eprintln!();
    }
}

/// Executa o auto-update
pub fn run_update() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Verificando atualizações...\n");

    let release = fetch_latest_release()?;

    if !is_newer(&release.version, CURRENT_VERSION) {
        eprintln!("✓ Você já está na versão mais recente (v{})", CURRENT_VERSION);
        return Ok(());
    }

    eprintln!(
        "Nova versão: v{} → v{}\n",
        CURRENT_VERSION, release.version
    );

    // Encontra o asset correto
    let target_asset = asset_name();
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == target_asset)
        .ok_or_else(|| {
            format!(
                "Asset '{}' não encontrado na release. Assets disponíveis: {}",
                target_asset,
                release
                    .assets
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    eprintln!("Baixando {}...", asset.name);

    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(120))
        .user_agent("gate-updater")
        .build()?;

    let resp = client.get(&asset.download_url).send()?;

    if !resp.status().is_success() {
        return Err(format!("Download falhou: {}", resp.status()).into());
    }

    let bytes = resp.bytes()?;
    eprintln!("✓ Download completo ({:.1} MB)\n", bytes.len() as f64 / 1_048_576.0);

    // Substitui o binário atual
    let current_exe = std::env::current_exe()?;
    replace_binary(&current_exe, &bytes)?;

    eprintln!("✓ Atualizado para v{}!", release.version);
    eprintln!("  Reinicie o gate para usar a nova versão.");

    Ok(())
}

/// Substitui o binário em execução
fn replace_binary(path: &PathBuf, new_bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        // Unix: pode sobrescrever o binário em execução
        let tmp = path.with_extension("new");
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(new_bytes)?;
        f.flush()?;

        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;

        std::fs::rename(&tmp, path)?;
    }

    #[cfg(windows)]
    {
        // Windows: não pode sobrescrever exe em execução
        // Renomeia o antigo, escreve o novo, deleta o antigo no próximo boot
        let old = path.with_extension("old.exe");
        // Remove .old anterior se existir
        std::fs::remove_file(&old).ok();
        // Renomeia atual → .old
        std::fs::rename(path, &old)?;
        // Escreve novo
        let mut f = std::fs::File::create(path)?;
        f.write_all(new_bytes)?;
        f.flush()?;
        eprintln!("  (arquivo antigo em {:?} — pode deletar)", old);
    }

    Ok(())
}

/// Retorna a versão atual
pub fn current_version() -> &'static str {
    CURRENT_VERSION
}
