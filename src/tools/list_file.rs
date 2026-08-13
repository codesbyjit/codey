pub fn list_file(path: &std) -> Result<String, std::io::Error> {
    std::fs::read_dir(".")
}