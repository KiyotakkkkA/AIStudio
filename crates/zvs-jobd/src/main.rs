use std::process::ExitCode;

use tokio::io::{stdin, stdout};
use zvs_jobd::server::{ServeOptions, serve_with};

#[cfg(debug_assertions)]
fn deaf() -> bool {
    std::env::args()
        .skip(1)
        .any(|argument| argument == zvs_jobd::server::DEAF_FLAG)
}

#[cfg(not(debug_assertions))]
fn deaf() -> bool {
    false
}

#[tokio::main]
async fn main() -> ExitCode {
    let options = ServeOptions {
        answer_ping: !deaf(),
    };
    match serve_with(stdin(), stdout(), options).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("zvs-jobd: {error}");
            ExitCode::FAILURE
        }
    }
}
