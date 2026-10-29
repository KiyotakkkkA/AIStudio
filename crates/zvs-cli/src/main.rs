#[tokio::main]
async fn main() {
    std::process::exit(zvs_cli::runtime::run().await);
}
