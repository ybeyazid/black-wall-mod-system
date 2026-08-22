//! Scan dos mods BWMS instalados: lê staging (<jogo>/BWMS/mods/) + manifesto aplicado.
//! Toggle = copia/remove do content em background thread; restart necessário p/ .archive.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[derive(Clone)]
pub struct BwmsMod {
    pub name: String,
    pub theme: String,
    pub active: bool,
    pub pending: bool, // toggle pendente (toggle clicado, apply ainda rodando)
    pub problem: Option<String>,
}

static CACHE: Mutex<Vec<BwmsMod>> = Mutex::new(Vec::new());
pub static NEEDS_RESTART: AtomicBool = AtomicBool::new(false);
static REFRESHING: AtomicBool = AtomicBool::new(false);

// Mesmo prefixo do apply.rs do bwms-core
const PREFIX: &str = "basegame_zzbwms_";

fn game_root() -> Option<PathBuf> {
    crate::dylib_dir()
        .map(PathBuf::from)
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_us = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_us = false;
        } else if !last_us {
            out.push('_');
            last_us = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn archives_in(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    stack.push(p);
                } else if p.extension().and_then(|s| s.to_str())
                    .map(|e| e.eq_ignore_ascii_case("archive"))
                    .unwrap_or(false)
                {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

fn read_applied(path: &Path) -> HashSet<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Dispara um refresh em background; no-op se já rodando.
pub fn refresh_async() {
    if REFRESHING.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed).is_err() {
        return;
    }
    std::thread::spawn(|| {
        refresh_sync();
        REFRESHING.store(false, Ordering::Relaxed);
    });
}

fn refresh_sync() {
    let Some(root) = game_root() else { return };
    let staging = root.join("BWMS/mods");
    let applied_path = root.join(".cp77-mods/bwms-applied.txt");
    let content_dir = root.join("archive/Mac/content");

    let applied = read_applied(&applied_path);

    let mut result: Vec<BwmsMod> = Vec::new();

    if let Ok(themes) = std::fs::read_dir(&staging) {
        for te in themes.flatten() {
            if !te.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
            let theme = te.file_name().to_string_lossy().into_owned();
            if let Ok(mods) = std::fs::read_dir(te.path()) {
                for me in mods.flatten() {
                    if !me.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
                    let name = me.file_name().to_string_lossy().into_owned();
                    let slug = slugify(&name);
                    let prefix_str = format!("{PREFIX}{slug}");

                    // Ativo = aparece no manifesto OU o arquivo já está no content
                    let in_manifest = applied.iter().any(|f| f.starts_with(&prefix_str));
                    let in_content = content_dir.is_dir() &&
                        std::fs::read_dir(&content_dir)
                            .ok()
                            .map(|rd| rd.flatten().any(|e| {
                                e.file_name().to_string_lossy().starts_with(&prefix_str)
                            }))
                            .unwrap_or(false);
                    let active = in_manifest || in_content;

                    // Problema: no manifesto mas arquivo não existe no content
                    let problem = if in_manifest && !in_content {
                        Some("arquivo ausente no content".to_string())
                    } else {
                        None
                    };

                    result.push(BwmsMod {
                        name,
                        theme: theme.clone(),
                        active,
                        pending: false,
                        problem,
                    });
                }
            }
        }
    }

    result.sort_by(|a, b| a.theme.cmp(&b.theme).then(a.name.cmp(&b.name)));

    if let Ok(mut cache) = CACHE.lock() {
        // preserva flags `pending` dos itens que já estavam no cache
        for m in &mut result {
            if let Some(old) = cache.iter().find(|c| c.name == m.name) {
                m.pending = old.pending;
            }
        }
        *cache = result;
    }
}

pub fn get() -> Vec<BwmsMod> {
    CACHE.lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn summary() -> (usize, usize, usize) {
    let list = get();
    let active = list.iter().filter(|m| m.active).count();
    let inactive = list.iter().filter(|m| !m.active).count();
    let problems = list.iter().filter(|m| m.problem.is_some()).count();
    (active, inactive, problems)
}

/// Toggle em background. `currently_active` = estado ATUAL (antes do toggle).
pub fn toggle(mod_name: String, currently_active: bool) {
    // Marca pending na cache imediatamente (feedback visual instantâneo)
    if let Ok(mut cache) = CACHE.lock() {
        if let Some(m) = cache.iter_mut().find(|m| m.name == mod_name) {
            m.pending = true;
        }
    }
    std::thread::spawn(move || {
        apply_toggle(&mod_name, currently_active);
        refresh_sync();
        NEEDS_RESTART.store(true, Ordering::Relaxed);
    });
}

fn apply_toggle(mod_name: &str, currently_active: bool) {
    let Some(root) = game_root() else { return };
    let staging = root.join("BWMS/mods");
    let applied_path = root.join(".cp77-mods/bwms-applied.txt");
    let content_dir = root.join("archive/Mac/content");
    let slug = slugify(mod_name);
    let prefix_str = format!("{PREFIX}{slug}");

    if currently_active {
        // Desativar: remove arquivos do content + retira do manifesto
        if let Ok(rd) = std::fs::read_dir(&content_dir) {
            for e in rd.flatten() {
                let fname = e.file_name().to_string_lossy().into_owned();
                if fname.starts_with(&prefix_str) {
                    let _ = std::fs::remove_file(e.path());
                    crate::log(&format!("[mod-scan] removido: {fname}"));
                }
            }
        }
        let manifest = std::fs::read_to_string(&applied_path).unwrap_or_default();
        let new_manifest: String = manifest.lines()
            .filter(|l| !l.contains(&prefix_str))
            .map(|l| format!("{l}\n"))
            .collect();
        let _ = std::fs::write(&applied_path, new_manifest);
    } else {
        // Ativar: encontra archives no staging, copia pro content
        let mut found_archives: Vec<PathBuf> = Vec::new();
        if let Ok(themes) = std::fs::read_dir(&staging) {
            for te in themes.flatten() {
                let mod_path = te.path().join(mod_name);
                if mod_path.is_dir() {
                    found_archives = archives_in(&mod_path);
                    break;
                }
            }
        }
        if found_archives.is_empty() {
            crate::log(&format!("[mod-scan] sem .archive em staging para '{mod_name}'"));
            return;
        }
        let _ = std::fs::create_dir_all(&content_dir);
        let mut new_files: Vec<String> = Vec::new();
        for (i, src) in found_archives.iter().enumerate() {
            let dst_name = if found_archives.len() == 1 {
                format!("{prefix_str}.archive")
            } else {
                let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("a");
                format!("{prefix_str}__{}.archive", slugify(stem))
            };
            let dst = content_dir.join(&dst_name);
            match std::fs::copy(src, &dst) {
                Ok(_) => {
                    crate::log(&format!("[mod-scan] copiado: {dst_name}"));
                    new_files.push(dst_name);
                }
                Err(e) => {
                    crate::log(&format!("[mod-scan] erro ao copiar '{dst_name}': {e}"));
                }
            }
            let _ = i; // evita warning
        }
        // Atualiza manifesto
        let mut manifest = std::fs::read_to_string(&applied_path).unwrap_or_default();
        for f in &new_files {
            if !manifest.lines().any(|l| l.trim() == f) {
                manifest.push_str(f);
                manifest.push('\n');
            }
        }
        let _ = std::fs::write(&applied_path, manifest);
    }
}
