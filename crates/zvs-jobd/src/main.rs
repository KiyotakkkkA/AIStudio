use std::process::ExitCode;

use tokio::io::{stdin, stdout};

#[tokio::main]
async fn main() -> ExitCode {
    match zvs_jobd::server::serve(stdin(), stdout()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("zvs-jobd: {error}");
            ExitCode::FAILURE
        }
    }
}
