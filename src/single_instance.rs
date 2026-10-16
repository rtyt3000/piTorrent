use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const BASE_PORT: u16 = 47613;
const SHOW: &str = "pitorrent:show";
const PING: &str = "pitorrent:ping";
const OPEN: &str = "pitorrent:open:";
const MAX_MESSAGE: u64 = 16 * 1024;

fn port() -> u16 {
    match crate::settings::profile() {
        Some(p) => BASE_PORT + 1 + p.bytes().map(u16::from).sum::<u16>() % 500,
        None => BASE_PORT,
    }
}

static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);
static OPEN_REQUESTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static STARTUP_ARG: Mutex<Option<String>> = Mutex::new(None);

/// Returns false if another instance is already running. That instance is asked to show its
/// window (and to open `arg`, a magnet link or .torrent path) unless `quiet`, e.g. when
/// launched by autostart.
pub fn acquire(quiet: bool, arg: Option<String>) -> bool {
    match TcpListener::bind((Ipv4Addr::LOCALHOST, port())) {
        Ok(listener) => {
            *STARTUP_ARG.lock().unwrap() = arg;
            std::thread::spawn(move || {
                for mut stream in listener.incoming().flatten() {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut msg = String::new();
                    let _ = (&mut stream).take(MAX_MESSAGE).read_to_string(&mut msg);
                    handle(&msg);
                }
            });
            true
        }
        Err(_) => !notify_existing(quiet, arg),
    }
}

fn handle(msg: &str) {
    if msg == SHOW {
        SHOW_REQUESTED.store(true, Ordering::Relaxed);
    } else if let Some(arg) = msg.strip_prefix(OPEN) {
        OPEN_REQUESTS.lock().unwrap().push(arg.to_string());
        SHOW_REQUESTED.store(true, Ordering::Relaxed);
    }
}

fn notify_existing(quiet: bool, arg: Option<String>) -> bool {
    let Ok(mut s) = TcpStream::connect_timeout(
        &(Ipv4Addr::LOCALHOST, port()).into(),
        Duration::from_millis(500),
    ) else {
        return false;
    };
    let msg = match (arg, quiet) {
        (Some(a), _) => format!("{OPEN}{a}"),
        (None, true) => PING.to_string(),
        (None, false) => SHOW.to_string(),
    };
    s.write_all(msg.as_bytes()).is_ok()
}

pub fn take_show_request() -> bool {
    SHOW_REQUESTED.swap(false, Ordering::Relaxed)
}

pub fn take_open_requests() -> Vec<String> {
    std::mem::take(&mut *OPEN_REQUESTS.lock().unwrap())
}

/// What this instance was launched with, if it is the first one.
pub fn take_startup_arg() -> Option<String> {
    STARTUP_ARG.lock().unwrap().take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_routed() {
        handle("pitorrent:open:magnet:?xt=urn:btih:abc");
        assert_eq!(take_open_requests(), ["magnet:?xt=urn:btih:abc"]);
        assert!(take_show_request());
        handle(PING);
        assert!(!take_show_request());
        handle(SHOW);
        assert!(take_show_request());
    }
}
