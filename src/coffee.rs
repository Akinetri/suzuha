use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use amane::Service;

pub const ICON: &str = "\u{f0176}";

// the inhibitor while it is held, killed as a group so its sleep goes with it
static HELD: Mutex<Option<Child>> = Mutex::new(None);

/*
 * keeps the screen from blanking and the machine from sleeping while on,
 * through a logind inhibitor; the inhibitor watches the shell, so it is
 * let go even if the shell dies without turning it off
 */
#[derive(Default)]
pub struct Coffee {
    pub on: bool,
}

impl Service for Coffee {
    fn new() -> Self {
        Self::default()
    }

    // it only changes through input
    fn listen() {}
}

pub fn toggle() {
    let mut held = HELD.lock().expect("failed to lock the coffee inhibitor");

    if let Some(mut child) = held.take() {
        // the negative id is the whole group: systemd-inhibit and the loop it runs
        let _ = Command::new("kill").arg("--").arg(format!("-{}", child.id())).status();

        let _ = child.wait();

        Coffee::write().on = false;

        return;
    }

    let shell = std::process::id();

    let watch = format!("while kill -0 {shell} 2>/dev/null; do sleep 5; done");

    let started = Command::new("systemd-inhibit")
        .args(["--what=idle:sleep", "--who=amane", "--why=Coffee is on", "--mode=block"])
        .args(["sh", "-c", &watch])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();

    match started {
        Ok(child) => {
            *held = Some(child);

            Coffee::write().on = true;
        }

        Err(error) => eprintln!("amane: failed to start systemd-inhibit: {error}"),
    }
}

pub fn on() -> bool {
    Coffee::read().on
}

// "toggle", "on" or "off"
pub fn ipc(arguments: &[String]) -> String {
    let wanted = match arguments.first().map(String::as_str) {
        Some("on") => true,
        Some("off") => false,
        _ => !on(),
    };

    if wanted != on() {
        toggle();
    }

    String::from(if on() { "on" } else { "off" })
}
