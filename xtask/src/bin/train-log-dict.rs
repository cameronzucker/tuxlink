//! Trains a zstd dictionary from a JSONL corpus directory.
//! Output: a .zdict file ready for `include_bytes!` in src-tauri/src/logging/dict.rs.

use anyhow::{bail, Context, Result};
use clap::Parser;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 16)]
    size_kb: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let files: Vec<Vec<u8>> = WalkDir::new(&args.input)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "jsonl"))
        .map(|e| std::fs::read(e.path()).with_context(|| format!("read {:?}", e.path())))
        .collect::<Result<Vec<_>>>()?;

    if files.is_empty() {
        bail!("no .jsonl files found under {:?}", args.input);
    }

    println!("Training dictionary from {} files ({} total bytes)...",
        files.len(),
        files.iter().map(Vec::len).sum::<usize>());

    let dict_size_bytes = args.size_kb * 1024;
    let dict = zstd::dict::from_continuous(
        &files.concat(),
        &files.iter().map(Vec::len).collect::<Vec<_>>(),
        dict_size_bytes,
    )
    .context("zstd::dict::from_continuous failed")?;

    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&args.output, &dict).with_context(|| format!("write {:?}", args.output))?;
    println!("Wrote {} byte dictionary to {:?}", dict.len(), args.output);
    Ok(())
}
