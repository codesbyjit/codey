use std::path::Path;

const MAX_INSTRUCTIONS_BYTES: usize = 8_000;

pub fn discover_instructions(workspace: &Path) -> String {
    let mut files = Vec::new();

    let mut current = Some(workspace.to_path_buf());
    while let Some(dir) = current {
        let candidate = dir.join("AGENTS.md");
        if candidate.is_file() {
            files.push(candidate);
        }
        current = dir.parent().map(|p| p.to_path_buf());
    }

    files.reverse();

    let mut out = String::new();
    for file in files {
        if let Ok(content) = std::fs::read_to_string(&file) {
            let content = content.trim();
            if content.is_empty() {
                continue;
            }
            out.push_str(&format!("[{}]\n", file.display()));
            out.push_str(content);
            out.push_str("\n\n");
            if out.len() > MAX_INSTRUCTIONS_BYTES {
                out.truncate(MAX_INSTRUCTIONS_BYTES);
                out.push_str("\n[truncated]\n");
                break;
            }
        }
    }

    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_instructions_in_parent_dirs() {
        let tmp = std::env::temp_dir().join("codey_agents_test");
        let sub = tmp.join("backend").join("src");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(tmp.join("AGENTS.md"), "Root rules.").unwrap();
        std::fs::write(sub.parent().unwrap().join("AGENTS.md"), "Backend rules.").unwrap();

        let result = discover_instructions(&sub);
        assert!(result.contains("Root rules."));
        assert!(result.contains("Backend rules."));

        assert!(result.find("Root rules.").unwrap() < result.find("Backend rules.").unwrap());

        std::fs::remove_dir_all(&tmp).ok();
    }
}
