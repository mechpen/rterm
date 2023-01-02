use rterm::app::App;

use std::process::exit;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[clap(version, about)]
struct AppArg {
    #[clap(short, long)]
    geometry: Option<String>,
    #[clap(short, long)]
    font: Option<String>,
    #[clap(short = 'o', long)]
    log: Option<String>,
}

fn _main() -> Result<()> {
    let arg: AppArg = AppArg::parse();
    let mut app = App::new(
        arg.geometry.as_deref(),
        arg.font.as_deref(),
        arg.log.as_deref(),
    )?;
    app.run()?;

    Ok(())
}

fn main() {
    let ret = match _main() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{}", e);
            -1
        }
    };
    exit(ret);
}
