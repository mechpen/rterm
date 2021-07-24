use crate::Result;
use nix::sys::signal::{signal, SigHandler, Signal};
use nix::unistd::execvp;
use nix::unistd::{Uid, User};
use std::env;
use std::ffi::CString;
use std::os::unix::ffi::OsStringExt;
use std::process::exit;

fn _exec_shell() -> Result<()> {
    let user = User::from_uid(Uid::current())?.unwrap();
    let shell_default = &user.shell;
    let shell = env::var_os("SHELL").unwrap_or_else(|| shell_default.into());

    env::remove_var("COLUMNS");
    env::remove_var("LINES");
    env::remove_var("TERMCAP");

    env::set_var("LOGNAME", &user.name);
    env::set_var("USER", &user.name);
    env::set_var("HOME", &user.dir);
    env::set_var("SHELL", &shell);
    env::set_var("TERM", "xterm");

    unsafe {
        signal(Signal::SIGCHLD, SigHandler::SigDfl)?;
        signal(Signal::SIGCHLD, SigHandler::SigDfl)?;
        signal(Signal::SIGHUP, SigHandler::SigDfl)?;
        signal(Signal::SIGINT, SigHandler::SigDfl)?;
        signal(Signal::SIGQUIT, SigHandler::SigDfl)?;
        signal(Signal::SIGTERM, SigHandler::SigDfl)?;
        signal(Signal::SIGALRM, SigHandler::SigDfl)?;
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
