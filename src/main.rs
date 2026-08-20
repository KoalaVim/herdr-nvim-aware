use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process;

// -- Platform-specific IPC stream --

#[cfg(unix)]
fn connect_herdr(path: &str) -> io::Result<impl Read + Write> {
    std::os::unix::net::UnixStream::connect(path)
}

#[cfg(windows)]
fn connect_herdr(path: &str) -> io::Result<impl Read + Write> {
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL,
    };

    let full_path = format!(r"\\.\pipe\{path}");
    let pipe_name: Vec<u16> = full_path.encode_utf16().chain(std::iter::once(0)).collect();

    let handle = unsafe {
        CreateFileW(
            pipe_name.as_ptr(),
            0x80000000 | 0x40000000, // GENERIC_READ | GENERIC_WRITE
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            0,
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    Ok(unsafe { std::fs::File::from_raw_handle(handle as _) })
}

// -- Process existence check --

#[cfg(unix)]
fn process_exists(pid: i32) -> bool {
    unsafe {
        if libc::kill(pid, 0) == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

#[cfg(windows)]
fn process_exists(pid: i32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32) };
    if handle == 0 {
        false
    } else {
        unsafe { CloseHandle(handle) };
        true
    }
}

// -- Marker detection --

fn build_marker_path(pane: &str) -> Option<PathBuf> {
    if let Ok(cache) = env::var("XDG_CACHE_HOME") {
        if !cache.is_empty() {
            return Some(
                PathBuf::from(cache)
                    .join("herdr")
                    .join("nvim-panes")
                    .join(pane),
            );
        }
    }

    if let Ok(home) = env::var("HOME") {
        if !home.is_empty() {
            return Some(
                PathBuf::from(home)
                    .join(".cache")
                    .join("herdr")
                    .join("nvim-panes")
                    .join(pane),
            );
        }
    }

    #[cfg(windows)]
    if let Ok(profile) = env::var("USERPROFILE") {
        if !profile.is_empty() {
            return Some(
                PathBuf::from(profile)
                    .join(".cache")
                    .join("herdr")
                    .join("nvim-panes")
                    .join(pane),
            );
        }
    }

    None
}

fn marker_says_vim(pane: &str) -> bool {
    let path = match build_marker_path(pane) {
        Some(p) => p,
        None => return false,
    };

    let content = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let pid: i32 = match content.trim().parse() {
        Ok(p) if p > 0 => p,
        _ => return false,
    };

    if process_exists(pid) {
        return true;
    }

    let _ = fs::remove_file(&path);
    false
}

// -- Socket path --

#[cfg(unix)]
fn socket_path() -> Option<String> {
    if let Ok(sock) = env::var("HERDR_SOCKET_PATH") {
        if !sock.is_empty() {
            return Some(sock);
        }
    }
    if let Ok(home) = env::var("HOME") {
        if !home.is_empty() {
            return Some(format!("{home}/.config/herdr/herdr.sock"));
        }
    }
    None
}

#[cfg(windows)]
fn socket_path() -> Option<String> {
    if let Ok(sock) = env::var("HERDR_SOCKET_PATH") {
        if !sock.is_empty() {
            return Some(sock);
        }
    }
    if let Ok(appdata) = env::var("APPDATA") {
        if !appdata.is_empty() {
            return Some(format!("{appdata}\\herdr\\herdr.sock"));
        }
    }
    None
}

// -- IPC request --

fn herdr_request(json: &str) -> Result<(), String> {
    let path = socket_path().ok_or("no socket path")?;

    let mut stream = connect_herdr(&path).map_err(|e| format!("connect: {e}"))?;

    stream
        .write_all(json.as_bytes())
        .map_err(|e| format!("send: {e}"))?;
    stream.flush().map_err(|e| format!("flush: {e}"))?;

    let mut reader = BufReader::new(stream);
    let mut reply = String::new();
    reader.read_line(&mut reply).map_err(|e| format!("recv: {e}"))?;

    if reply.contains("\"error\"") {
        return Err(format!("rejected: {reply}"));
    }

    Ok(())
}

// -- Action dispatch --

struct Action {
    name: &'static str,
    nvim_key: &'static str,
    herdr_method: Option<&'static str>,
    herdr_params_fmt: Option<&'static str>,
    is_shell_cmd: bool,
}

const ACTIONS: &[Action] = &[
    Action { name: "left", nvim_key: "ctrl+h", herdr_method: Some("pane.focus_direction"), herdr_params_fmt: Some(r#""direction":"left","pane_id":"{}""#), is_shell_cmd: false },
    Action { name: "down", nvim_key: "ctrl+j", herdr_method: Some("pane.focus_direction"), herdr_params_fmt: Some(r#""direction":"down","pane_id":"{}""#), is_shell_cmd: false },
    Action { name: "up", nvim_key: "ctrl+k", herdr_method: Some("pane.focus_direction"), herdr_params_fmt: Some(r#""direction":"up","pane_id":"{}""#), is_shell_cmd: false },
    Action { name: "right", nvim_key: "ctrl+l", herdr_method: Some("pane.focus_direction"), herdr_params_fmt: Some(r#""direction":"right","pane_id":"{}""#), is_shell_cmd: false },
    Action { name: "split_v", nvim_key: "alt+e", herdr_method: Some("pane.split"), herdr_params_fmt: Some(r#""direction":"right","pane_id":"{}","focus":true"#), is_shell_cmd: false },
    Action { name: "split_h", nvim_key: "alt+o", herdr_method: Some("pane.split"), herdr_params_fmt: Some(r#""direction":"down","pane_id":"{}","focus":true"#), is_shell_cmd: false },
    Action { name: "close", nvim_key: "alt+w", herdr_method: Some("pane.close"), herdr_params_fmt: Some(r#""pane_id":"{}""#), is_shell_cmd: false },
    Action { name: "quit", nvim_key: "alt+q", herdr_method: Some("pane.close"), herdr_params_fmt: Some(r#""pane_id":"{}""#), is_shell_cmd: false },
    Action { name: "zoom", nvim_key: "alt+z", herdr_method: Some("pane.zoom"), herdr_params_fmt: Some(r#""pane_id":"{}","mode":"toggle""#), is_shell_cmd: false },
    Action { name: "extrakto", nvim_key: "ctrl+space", herdr_method: None, herdr_params_fmt: None, is_shell_cmd: true },
];

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: herdr-nvim-aware <action>");
        process::exit(2);
    }

    let action_name = &args[1];
    let action = match ACTIONS.iter().find(|a| a.name == action_name.as_str()) {
        Some(a) => a,
        None => {
            eprintln!("herdr-nvim-aware: unknown action: {action_name}");
            process::exit(2);
        }
    };

    let pane = env::var("HERDR_PANE_ID").unwrap_or_default();

    if !pane.is_empty() && marker_says_vim(&pane) {
        // Forward key to nvim
        let json = format!(
            "{{\"id\":\"nvim-aware\",\"method\":\"pane.send_keys\",\"params\":{{\"pane_id\":\"{pane}\",\"keys\":[\"{}\"]}}}}\n",
            action.nvim_key
        );
        if let Err(e) = herdr_request(&json) {
            eprintln!("herdr-nvim-aware: {e}");
            process::exit(1);
        }
    } else if action.is_shell_cmd {
        // Shell command (extrakto) — must go through CLI
        let env_arg = format!("EXTRAKTO_TRIGGER_PANE={pane}");
        let status = process::Command::new("herdr")
            .args([
                "plugin", "pane", "open",
                "--plugin", "extrakto-herdr",
                "--entrypoint", "picker",
                "--env", &env_arg,
            ])
            .status();
        match status {
            Ok(s) => process::exit(s.code().unwrap_or(1)),
            Err(e) => {
                eprintln!("herdr-nvim-aware: exec: {e}");
                process::exit(1);
            }
        }
    } else if !pane.is_empty() {
        // Herdr action via IPC
        let params = action.herdr_params_fmt.unwrap().replace("{}", &pane);
        let json = format!(
            "{{\"id\":\"nvim-aware\",\"method\":\"{}\",\"params\":{{{params}}}}}\n",
            action.herdr_method.unwrap()
        );
        if let Err(e) = herdr_request(&json) {
            eprintln!("herdr-nvim-aware: {e}");
            process::exit(1);
        }
    } else if action.herdr_method == Some("pane.focus_direction") {
        // No pane ID — focus_direction without pane_id
        let json = format!(
            "{{\"id\":\"nvim-aware\",\"method\":\"pane.focus_direction\",\"params\":{{\"direction\":\"{action_name}\"}}}}\n"
        );
        if let Err(e) = herdr_request(&json) {
            eprintln!("herdr-nvim-aware: {e}");
            process::exit(1);
        }
    } else {
        eprintln!("herdr-nvim-aware: no HERDR_PANE_ID for action {action_name}");
        process::exit(1);
    }
}
