pub fn write_file(path: &str, content: &str) -> Result<(), std::io::Error> {
    std::fs::write(path, content)
}