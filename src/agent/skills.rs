use std::path::{Path, PathBuf};

use crate::config::{config_dir, user_skills_dir};

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub path: PathBuf,
}

fn skills_in(dir: &Path) -> Vec<Skill> {
    let mut skills = Vec::new();
    let dir = dir.join(".codey").join("skills");
    if !dir.is_dir() {
        return skills;
    }
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if skill_md.is_file() {
            if let Some(skill) = parse_skill(&skill_md) {
                skills.push(skill);
            }
        }
    }
    skills
}

fn user_level_skills() -> Vec<Skill> {
    let mut skills = Vec::new();
    if let Some(dir) = user_skills_dir().or_else(config_dir) {
        if dir.is_dir() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let skill_md = path.join("SKILL.md");
                if skill_md.is_file() {
                    if let Some(skill) = parse_skill(&skill_md) {
                        skills.push(skill);
                    }
                }
            }
        }
    }
    skills
}

fn parse_skill(path: &Path) -> Option<Skill> {
    let text = std::fs::read_to_string(path).ok()?;
    let (name, description, front_instructions) = parse_frontmatter(&text);
    let body: String = if text.starts_with("---\n") {
        text.splitn(3, "---\n").nth(2).unwrap_or("").to_string()
    } else {
        text.clone()
    };

    let instructions = if !front_instructions.is_empty() {
        front_instructions
    } else {
        body.trim().to_string()
    };

    let name = name
        .or_else(|| {
            path.parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "unnamed".to_string());

    if instructions.is_empty() {
        return None;
    }

    Some(Skill {
        name,
        description: description.unwrap_or_else(|| "No description.".to_string()),
        instructions,
        path: path.to_path_buf(),
    })
}

fn parse_frontmatter(text: &str) -> (Option<String>, Option<String>, String) {
    if !text.starts_with("---\n") {
        return (None, None, String::new());
    }
    let rest = &text["---\n".len()..];
    let end = match rest.find("\n---\n").or_else(|| rest.find("\n---")) {
        Some(e) => e,
        None => return (None, None, String::new()),
    };
    let fm = &rest[..end];

    let mut name = None;
    let mut description = None;
    let mut instructions = String::new();
    for line in fm.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            let v = v.trim().to_string();
            match k {
                "name" => name = Some(v),
                "description" => description = Some(v),
                "instructions" => instructions.push_str(&v),
                _ => {}
            }
        }
    }
    (name, description, instructions)
}

pub fn discover_skills(workspace: &Path) -> Vec<Skill> {
    let mut skills = skills_in(workspace);
    for skill in user_level_skills() {
        if !skills.iter().any(|s| s.name == skill.name) {
            skills.push(skill);
        }
    }
    skills
}

pub fn summaries(skills: &[Skill]) -> Vec<String> {
    skills
        .iter()
        .map(|s| format!("{}: {}", s.name, s.description))
        .collect()
}

pub fn select_for_task(skills: &[Skill], task: &str) -> Vec<Skill> {
    let task_lower = task.to_lowercase();
    skills
        .iter()
        .filter(|s| {
            let name = s.name.to_lowercase();
            let desc = s.description.to_lowercase();
            task_lower.contains(&name)
                || desc
                    .split_whitespace()
                    .any(|w| w.len() > 4 && task_lower.contains(w))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frontmatter() {
        let text = "---\nname: Rust\n\
description: Rust dev help\n\
instructions: Use cargo.\n---\nBody text here.\n";
        let (name, desc, instr) = parse_frontmatter(text);
        assert_eq!(name.as_deref(), Some("Rust"));
        assert_eq!(desc.as_deref(), Some("Rust dev help"));
        assert_eq!(instr, "Use cargo.");
    }

    #[test]
    fn selects_by_keyword() {
        let skills = vec![
            Skill {
                name: "rust".into(),
                description: "Rust development".into(),
                instructions: "x".into(),
                path: PathBuf::from("/tmp"),
            },
            Skill {
                name: "docker".into(),
                description: "Containers".into(),
                instructions: "y".into(),
                path: PathBuf::from("/tmp"),
            },
        ];
        let selected = select_for_task(&skills, "help me with rust code");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "rust");
    }

    #[test]
    fn discovers_skills_in_workspace() {
        let tmp = std::env::temp_dir().join("codey_skill_discover");
        let skill_dir = tmp.join(".codey").join("skills").join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: does things\n---\nDo the thing.\n",
        )
        .unwrap();

        let found = discover_skills(&tmp);
        assert!(found.iter().any(|s| s.name == "my-skill"));

        std::fs::remove_dir_all(&tmp).ok();
    }
}
