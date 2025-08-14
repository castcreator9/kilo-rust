use nix::{
    libc::{VMIN, VTIME},
    sys::termios::{
        ControlFlags, InputFlags, LocalFlags, OutputFlags, SetArg, Termios, tcgetattr, tcsetattr,
    },
};
use std::{io, os::fd::AsFd};

pub fn enable_raw_mode() -> Termios {
    let mut raw = tcgetattr(io::stdin().as_fd()).expect("Error while getting terminal attr.");
    let orig = raw.clone();

    raw.input_flags &= !(InputFlags::IXON
        | InputFlags::ICRNL
        | InputFlags::BRKINT
        | InputFlags::INPCK
        | InputFlags::ISTRIP);
    raw.output_flags &= !(OutputFlags::OPOST);
    raw.control_flags &= !(ControlFlags::CS8);
    raw.local_flags &=
        !(LocalFlags::ECHO | LocalFlags::ICANON | LocalFlags::ISIG | LocalFlags::IEXTEN);
    raw.control_chars[VMIN] = 0;
    raw.control_chars[VTIME] = 1;

    // TCSAFLUSH -> Changes will occur after all output has been writen
    tcsetattr(io::stdin().as_fd(), SetArg::TCSAFLUSH, &raw).expect("Error while setting new attr");

    orig
}
