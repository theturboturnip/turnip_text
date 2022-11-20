use argh::FromArgs;
use turnip_text::cli::parse_file;

#[derive(FromArgs)]
#[argh(description = "")]
struct ParseCmd {
    #[argh(positional)]
    path: std::path::PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args: ParseCmd = argh::from_env();
    let tokens = parse_file(&args.path)?;
    for t in tokens {
        println!("{:?}", t);
    }
    Ok(())
}
