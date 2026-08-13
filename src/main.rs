mod core;
mod agent;

#[tokio::main]
async fn main() {
    let user_prompt = match core::input::get_user_prompt() {
        Ok(prompt) => prompt,
        Err(err_msg) => {
            eprintln!("{}", err_msg);
            std::process::exit(1);
        }
    };

    match agent::model::ask_model(&user_prompt).await {
        Ok(response) => println!("\n{}", response),
        Err(e) => eprintln!("Error calling model: {}", e),
    }
    
}
