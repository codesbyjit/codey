use std::path::PathBuf;

use anyhow::{Context as _, Result};

use crate::config::{data_dir, ensure_dir};

pub fn sessions_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("sessions"))
}

pub fn save_session(id: &str, json: &str) -> Result<()> {
    let dir = sessions_dir().context("could not resolve sessions dir")?;
    ensure_dir(&dir)?;
    let path = dir.join(format!("{id}.json"));
    std::fs::write(&path, json).with_context(|| format!("writing session {}", path.display()))?;
    Ok(())
}

pub fn load_session(id: &str) -> Result<String> {
    let dir = sessions_dir().context("could not resolve sessions dir")?;
    let path = dir.join(format!("{id}.json"));
    std::fs::read_to_string(&path).with_context(|| format!("reading session {}", path.display()))
}

pub fn list_sessions() -> Vec<String> {
    let Some(dir) = sessions_dir() else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(id) = name.strip_suffix(".json") {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

pub fn delete_session(id: &str) -> Result<()> {
    let dir = sessions_dir().context("could not resolve sessions dir")?;
    let path = dir.join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path).ok();
    }
    Ok(())
}
