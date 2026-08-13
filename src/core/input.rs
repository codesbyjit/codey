use std::env;

pub fn get_user_prompt() -> Result<String, String> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        return Err("Usage: cargo run -- \"Your question here\"".to_string());
    }
    
    Ok(args[1].clone())
}