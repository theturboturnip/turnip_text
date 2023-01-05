use argh::FromArgs;
use turnip_text::{cli::parse_file, python::TurnipTextPython};

#[derive(FromArgs)]
#[argh(description = "")]
struct ParseCmd {
    #[argh(positional)]
    path: std::path::PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args: ParseCmd = argh::from_env();
    let ttpython = TurnipTextPython::new();
    let root = parse_file(&ttpython, &args.path)?;
    ttpython.with_gil(|py, _| todo!("Print document"));
    Ok(())
}
