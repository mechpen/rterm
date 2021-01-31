extern crate rt;

use std::env;
use rt::{
    Result,
    win,
};

fn usage() {
    println!("usage: rt [-v] [-f font] [-g geometry] [-c class] [-n name]");
    println!("          [-o file] [-t title] [[-e] command [args ...]]");
}

fn _main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    let mut font: Option<&str> = None;

    let mut i = 1;
    while i < args.len() {
        let arg = args[i];
        i += 1;

        if arg == "-v" {
            println!("rt-{}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        if arg == "-f" {
            font = Some(args[i]);
            i += 1;
            continue;
        }
        if arg == "-e" {
            break
        }
        if arg.starts_with("-") {
            usage();
            return Err("invalid option".into());
        }
    }

    let mut win = win::XWindow::new(80, 24, 7, 0, font)?;
    win.run()?;

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
