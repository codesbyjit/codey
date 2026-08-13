pub fn read_file(path: &std) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}