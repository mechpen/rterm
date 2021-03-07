use nix::unistd::{
    Uid,
    User,
};
use nix::sys::signal::{
    signal,
    Signal,
    SigHandler,
};
use nix::unistd::execvp;

use std::env;
use std::process::exit;
use std::ffi::CString;
use std::os::unix::ffi::OsStringExt;

use crate::Result;

fn _exec_shell() -> Result<()> {
    let user = User::from_uid(Uid::current())?.unwrap();
    let shell = env::var_os("SHELL").unwrap_or(user.shell.into());

    env::remove_var("COLUMNS");
    env::remove_var("LINES");
    env::remove_var("TERMCAP");

    env::set_var("LOGNAME", &user.name);
    env::set_var("USER",    &user.name);
    env::set_var("HOME",    &user.dir);
    env::set_var("SHELL",   &shell);
    // FIXME: check TERM support
    env::set_var("TERM",    "st-256color");

    unsafe {
        signal(Signal::SIGCHLD, SigHandler::SigDfl).unwrap();
        signal(Signal::SIGCHLD, SigHandler::SigDfl).unwrap();
        signal(Signal::SIGHUP,  SigHandler::SigDfl).unwrap();
        signal(Signal::SIGINT,  SigHandler::SigDfl).unwrap();
        signal(Signal::SIGQUIT, SigHandler::SigDfl).unwrap();
        signal(Signal::SIGTERM, SigHandler::SigDfl).unwrap();
        signal(Signal::SIGALRM, SigHandler::SigDfl).unwrap();
    }

    let shell = CString::new(shell.into_vec())?;
    let args = [shell.as_c_str()];
    execvp(&shell, &args)?;

    Ok(())
}

pub fn exec_shell() {
    _exec_shell().unwrap();
    exit(1);
}
