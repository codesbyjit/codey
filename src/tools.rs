use std::fs;
use std::process::Command;

pub fn read_file(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("Failed to read `{path}`: {e}"))
}

pub fn write_file(path: &str, content: &str) -> Result<String, String> {
    fs::write(path, content).map_err(|e| format!("Failed to write `{path}`: {e}"))?;

    Ok(format!("Successfully wrote `{path}`"))
}

pub fn list_files(path: &str) -> Result<String, String> {
    let entries = fs::read_dir(path).map_err(|e| format!("Failed to list `{path}`: {e}"))?;

    let mut result = String::new();

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;

        let file_type = entry.file_type().map_err(|e| e.to_string())?;

        let name = entry.file_name().to_string_lossy().to_string();

        if file_type.is_dir() {
            result.push_str(&format!("{name}/\n"));
        } else {
            result.push_str(&format!("{name}\n"));
        }
    }

    if result.is_empty() {
        result.push_str("(empty directory)");
    }

    Ok(result)
}

pub fn search(pattern: &str, path: &str) -> Result<String, String> {
    let output = Command::new("rg")
        .arg("--line-number")
        .arg("--hidden")
        .arg(pattern)
        .arg(path)
        .output()
        .map_err(|e| format!("Failed to run rg: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        return Ok(stdout.to_string());
    }

    if !stderr.is_empty() {
        return Ok(stderr.to_string());
    }

    Ok("No matches found.".to_string())
}

pub fn shell(command: &str) -> Result<String, String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .map_err(|e| format!("Failed to execute command: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut result = String::new();

    if !stdout.is_empty() {
        result.push_str(&stdout);
    }

    if !stderr.is_empty() {
        result.push_str("\nSTDERR:\n");
        result.push_str(&stderr);
    }

    if result.is_empty() {
        result.push_str("Command completed with no output.");
    }

    result.push_str(&format!(
        "\nExit code: {}",
        output.status.code().unwrap_or(-1)
    ));

    Ok(result)
}
