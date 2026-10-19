use std::{io::Read, path::Path, process::ExitCode};

use zvs_core::{
    CancellationToken,
    index::{VectorIndex, VectorRow},
};

async fn seed(path: &str, rows: &[VectorRow]) -> Result<usize, String> {
    let index = VectorIndex::open(Path::new(path))
        .await
        .map_err(|error| error.to_string())?;
    index
        .upsert(rows, &CancellationToken::new())
        .await
        .map_err(|error| error.to_string())
}

#[tokio::main]
async fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: seed_vectors <store directory> < rows.json");
        return ExitCode::FAILURE;
    };
    let mut input = String::new();
    if let Err(error) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("seed_vectors: {error}");
        return ExitCode::FAILURE;
    }
    let rows: Vec<VectorRow> = match serde_json::from_str(&input) {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("seed_vectors: {error}");
            return ExitCode::FAILURE;
        }
    };
    match seed(&path, &rows).await {
        Ok(count) => {
            println!("{count}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("seed_vectors: {error}");
            ExitCode::FAILURE
        }
    }
}
