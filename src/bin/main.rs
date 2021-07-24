extern crate rterm;

use rterm::{app::App, Result};
use std::env;

fn usage() {
    println!("usage: rterm [-v] [-g geometry] [-f font] [-o file]");
}

fn _main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    let mut geometry: Option<&str> = None;
    let mut font: Option<&str> = None;
    let mut log: Option<&str> = None;

    let mut i = 1;
    while i < args.len() {
        let arg = args[i];
        i += 1;

        if arg == "-v" {
            println!("rterm-{}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        if arg == "-g" {
            geometry = Some(args[i]);
            i += 1;
            continue;
        }
        if arg == "-f" {
            font = Some(args[i]);
            i += 1;
            continue;
        }
        if arg == "-o" {
            log = Some(args[i]);
            i += 1;
            continue;
        }
        if arg.starts_with("-") {
            usage();
            return Err("invalid option".into());
        }
    }

    let mut app = App::new(geometry, font, log)?;
    app.run()?;

    return Ok(());
}

fn main() {
    let ret = match _main() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{}", e.msg);
            -1
        }
    };
    std::process::exit(ret);
}
