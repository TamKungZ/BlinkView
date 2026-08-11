use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let exe = env::current_exe().map_err(|e| format!("cannot locate BlinkView executable: {e}"))?;
    platform_set_enabled(&exe, enabled).map_err(|e| format!("startup configuration failed: {e}"))
}

#[cfg(target_os = "linux")]
fn platform_set_enabled(exe: &Path, enabled: bool) -> io::Result<()> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    let dir = home.join(".config/autostart");
    let file = dir.join("blinkview-background.desktop");

    if !enabled {
        match fs::remove_file(file) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        }
    }

    fs::create_dir_all(dir)?;
    let exec = desktop_exec_escape(exe);
    fs::write(
        file,
        format!(
            "[Desktop Entry]\nType=Application\nName=BlinkView Background\nExec={exec} --background\nTerminal=false\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n"
        ),
    )
}

#[cfg(target_os = "macos")]
fn platform_set_enabled(exe: &Path, enabled: bool) -> io::Result<()> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    let dir = home.join("Library/LaunchAgents");
    let file = dir.join("me.tamkungz.blinkview.background.plist");

    if !enabled {
        match fs::remove_file(file) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        }
    }

    fs::create_dir_all(dir)?;
    let exe = xml_escape(&exe.to_string_lossy());
    fs::write(
        file,
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key><string>me.tamkungz.blinkview.background</string>\n  <key>ProgramArguments</key>\n  <array><string>{exe}</string><string>--background</string></array>\n  <key>RunAtLoad</key><true/>\n  <key>ProcessType</key><string>Background</string>\n</dict>\n</plist>\n"
        ),
    )
}

#[cfg(target_os = "windows")]
fn platform_set_enabled(exe: &Path, enabled: bool) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut cmd = Command::new("reg.exe");
    if enabled {
        let value = format!("\"{}\" --background", exe.display());
        cmd.args([
            "ADD",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "BlinkView",
            "/t",
            "REG_SZ",
            "/d",
        ])
        .arg(value)
        .arg("/f");
    } else {
        cmd.args([
            "DELETE",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "BlinkView",
            "/f",
        ]);
    }
    cmd.creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let status = cmd.status()?;
    if status.success() || !enabled {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "reg.exe returned failure"))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_set_enabled(_: &Path, _: bool) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "startup integration is not implemented for this OS",
    ))
}

#[cfg(target_os = "linux")]
fn desktop_exec_escape(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
