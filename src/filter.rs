use crate::config::{Category, Server};

/// Filtra servidores dentro de uma categoria
pub fn filter_servers(servers: &[Server], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return (0..servers.len()).collect();
    }
    let q = query.to_lowercase();
    servers
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            s.name.to_lowercase().contains(&q)
                || s.host.contains(&q)
                || s.user.to_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

/// Resultado de busca global
pub struct GlobalMatch {
    pub category_index: usize,
    pub server_index: usize,
    pub category_name: String,
    pub server: Server,
}

/// Busca em todas as categorias
pub fn global_search(categories: &[Category], query: &str) -> Vec<GlobalMatch> {
    if query.is_empty() {
        return vec![];
    }
    let q = query.to_lowercase();
    let mut results = Vec::new();

    for (ci, cat) in categories.iter().enumerate() {
        for (si, server) in cat.servers.iter().enumerate() {
            if server.name.to_lowercase().contains(&q)
                || server.host.contains(&q)
                || server.user.to_lowercase().contains(&q)
            {
                results.push(GlobalMatch {
                    category_index: ci,
                    server_index: si,
                    category_name: cat.name.clone(),
                    server: server.clone(),
                });
            }
        }
    }

    results
}
