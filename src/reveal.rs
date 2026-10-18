//! Open the selected image in the desktop file manager.
//!
//! The preferred path is the freedesktop `FileManager1.ShowItems` D-Bus call,
//! which opens the folder *and highlights the file* in whatever file manager
//! is the user's default. Fallbacks cover desktops without that service:
//! Nautilus' `--select`, then `gio open` and `xdg-open` on the parent folder.

use crossbeam_channel::{unbounded, Receiver};
use std::path::{Path, PathBuf};
use std::process::Command;

/// One candidate command. Kept as data so the argument construction is testable.
#[derive(Debug, PartialEq)]
struct CommandSpec {
    program: &'static str,
    args: Vec<String>,
}

/// Try each strategy in order on a worker thread; the receiver yields once.
pub fn spawn(path: PathBuf) -> Receiver<Result<(), String>> {
    let (tx, rx) = unbounded();
    std::thread::Builder::new()
        .name("reveal".to_owned())
        .spawn(move || {
            let _ = tx.send(open(&path));
        })
        .expect("spawn reveal worker");
    rx
}

fn open(path: &Path) -> Result<(), String> {
    let mut errors = Vec::new();
    for spec in candidate_commands(path) {
        let mut command = Command::new(spec.program);
        command.args(&spec.args);
        if spec.program == "nautilus" {
            // May hand off to the running service (or stay alive as a new
            // window); a successful spawn is all we can cheaply verify.
            match command.spawn() {
                Ok(_) => return Ok(()),
                Err(error) => errors.push(format!("{}: {error}", spec.program)),
            }
            continue;
        }
        match command.output() {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stderr = stderr.trim();
                if stderr.is_empty() {
                    errors.push(format!("{} exited with {}", spec.program, output.status));
                } else {
                    errors.push(format!("{}: {stderr}", spec.program));
                }
            }
            Err(error) => errors.push(format!("{}: {error}", spec.program)),
        }
    }
    Err(errors.join("; "))
}

/// The default-file-manager call first, then selection-aware and folder-only
/// fallbacks.
fn candidate_commands(path: &Path) -> Vec<CommandSpec> {
    let dir = path
        .parent()
        .map(|parent| parent.to_path_buf())
        .unwrap_or_else(|| path.to_path_buf());
    let uri = file_uri(path);
    let file = path.display().to_string();
    let dir = dir.display().to_string();
    vec![
        CommandSpec {
            program: "gdbus",
            args: vec![
                "call".to_owned(),
                "--session".to_owned(),
                "--dest".to_owned(),
                "org.freedesktop.FileManager1".to_owned(),
                "--object-path".to_owned(),
                "/org/freedesktop/FileManager1".to_owned(),
                "--method".to_owned(),
                "org.freedesktop.FileManager1.ShowItems".to_owned(),
                format!("['{uri}']"),
                String::new(),
            ],
        },
        CommandSpec {
            program: "nautilus",
            args: vec!["--select".to_owned(), file],
        },
        CommandSpec {
            program: "gio",
            args: vec!["open".to_owned(), dir.clone()],
        },
        CommandSpec {
            program: "xdg-open",
            args: vec![dir],
        },
    ]
}

/// `file://` URI with RFC 3986 path escaping.
fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(byte as char);
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gio_gets_the_open_subcommand() {
        let specs = candidate_commands(Path::new("/photos/a.jpg"));
        let gio = specs.iter().find(|spec| spec.program == "gio").unwrap();
        assert_eq!(gio.args, vec!["open", "/photos"]);
    }

    #[test]
    fn file_manager1_gets_a_show_items_call() {
        let specs = candidate_commands(Path::new("/photos/a b.jpg"));
        let gdbus = specs.iter().find(|spec| spec.program == "gdbus").unwrap();
        assert_eq!(gdbus.args[7], "org.freedesktop.FileManager1.ShowItems");
        assert_eq!(gdbus.args[8], "['file:///photos/a%20b.jpg']");
        assert_eq!(gdbus.args[9], "");
    }

    #[test]
    fn file_uri_escapes_special_bytes() {
        assert_eq!(
            file_uri(Path::new("/a/b c#d.jpg")),
            "file:///a/b%20c%23d.jpg"
        );
    }
}
