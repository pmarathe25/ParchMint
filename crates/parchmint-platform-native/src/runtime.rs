use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::Mutex,
};

use parchmint_platform_api::{
    ApplicationPaths, ClipboardContent, ClipboardFormats, PathDialog, PathDialogKind,
    PlatformError, SemanticMenu, SemanticMenuEntry, SystemAppearance, UntrustedClipboardContent,
    UntrustedPathSelection, ValidatedExternalIntent, WindowCapability,
};

pub(crate) trait NativeBackend: Send + Sync + 'static {
    fn install_menu(
        &self,
        window: WindowCapability,
        menu: SemanticMenu,
    ) -> Result<(), PlatformError>;
    fn remove_menu(&self, window: WindowCapability);
    fn choose_path(
        &self,
        window: WindowCapability,
        request: PathDialog,
    ) -> Result<Option<UntrustedPathSelection>, PlatformError>;
    fn read_clipboard(
        &self,
        window: WindowCapability,
        formats: ClipboardFormats,
    ) -> Result<UntrustedClipboardContent, PlatformError>;
    fn write_clipboard(
        &self,
        window: WindowCapability,
        content: ClipboardContent,
    ) -> Result<(), PlatformError>;
    fn open_external(
        &self,
        window: WindowCapability,
        intent: ValidatedExternalIntent,
    ) -> Result<(), PlatformError>;
    fn application_paths(&self) -> Result<ApplicationPaths, PlatformError>;
    fn appearance(&self) -> Result<SystemAppearance, PlatformError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MenuSnapshot {
    menu: SemanticMenu,
    commands: Vec<String>,
    contains_separator: bool,
}

impl MenuSnapshot {
    pub(crate) fn from_menu(menu: &SemanticMenu) -> Self {
        let mut snapshot = Self {
            menu: menu.clone(),
            commands: Vec::new(),
            contains_separator: false,
        };
        snapshot.collect(menu.entries());
        snapshot
    }

    fn collect(&mut self, entries: &[SemanticMenuEntry]) {
        for entry in entries {
            match entry {
                SemanticMenuEntry::Command(command) => {
                    self.commands.push(command.id().to_owned());
                }
                SemanticMenuEntry::Separator => self.contains_separator = true,
                SemanticMenuEntry::Submenu { entries, .. } => self.collect(entries),
            }
        }
    }

    pub(crate) fn commands(&self) -> &[String] {
        &self.commands
    }

    pub(crate) fn menu(&self) -> &SemanticMenu {
        &self.menu
    }

    pub(crate) const fn contains_separator(&self) -> bool {
        self.contains_separator
    }

    pub(crate) fn accelerator(&self, command: &str) -> Option<&'static str> {
        self.commands
            .iter()
            .any(|installed| installed == command)
            .then(|| accelerator(command))
            .flatten()
    }
}

#[cfg(target_os = "macos")]
const ACCELERATORS: &[(&str, &str)] = &[
    ("file.open", "Cmd+O"),
    ("file.save", "Cmd+S"),
    ("file.new", "Cmd+N"),
    ("file.close", "Cmd+W"),
    ("edit.copy", "Cmd+C"),
    ("edit.cut", "Cmd+X"),
    ("edit.paste", "Cmd+V"),
    ("edit.undo", "Cmd+Z"),
    ("edit.redo", "Cmd+Shift+Z"),
];

#[cfg(not(target_os = "macos"))]
const ACCELERATORS: &[(&str, &str)] = &[
    ("file.open", "Ctrl+O"),
    ("file.save", "Ctrl+S"),
    ("file.new", "Ctrl+N"),
    ("file.close", "Ctrl+W"),
    ("edit.copy", "Ctrl+C"),
    ("edit.cut", "Ctrl+X"),
    ("edit.paste", "Ctrl+V"),
    ("edit.undo", "Ctrl+Z"),
    ("edit.redo", "Ctrl+Y"),
];

pub(crate) fn accelerator(command: &str) -> Option<&'static str> {
    ACCELERATORS
        .iter()
        .find_map(|(id, accelerator)| (*id == command).then_some(*accelerator))
}

#[derive(Default)]
pub(crate) struct SystemBackend {
    menus: Mutex<HashMap<WindowCapability, MenuSnapshot>>,
}

impl NativeBackend for SystemBackend {
    fn install_menu(
        &self,
        window: WindowCapability,
        menu: SemanticMenu,
    ) -> Result<(), PlatformError> {
        // Installation retains the complete semantic tree. On Windows and
        // macOS, the Iced event loop later supplies a validated raw handle to
        // the narrow native-menu adapter; Linux projects the same tree as an
        // accessible in-window menu because winit's X11/Wayland window is not
        // a GTK window and cannot be passed to muda's GTK attachment API.
        let snapshot = MenuSnapshot::from_menu(&menu);
        self.menus
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(window, snapshot);
        Ok(())
    }

    fn remove_menu(&self, window: WindowCapability) {
        self.menus
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&window);
    }

    fn choose_path(
        &self,
        _window: WindowCapability,
        request: PathDialog,
    ) -> Result<Option<UntrustedPathSelection>, PlatformError> {
        platform::choose_path(&request).map(|path| path.map(UntrustedPathSelection::new))
    }

    fn read_clipboard(
        &self,
        _window: WindowCapability,
        formats: ClipboardFormats,
    ) -> Result<UntrustedClipboardContent, PlatformError> {
        platform::read_clipboard(formats)
    }

    fn write_clipboard(
        &self,
        _window: WindowCapability,
        content: ClipboardContent,
    ) -> Result<(), PlatformError> {
        platform::write_clipboard(content.as_plain_text())
    }

    fn open_external(
        &self,
        _window: WindowCapability,
        intent: ValidatedExternalIntent,
    ) -> Result<(), PlatformError> {
        platform::open_external(&intent)
    }

    fn application_paths(&self) -> Result<ApplicationPaths, PlatformError> {
        platform::application_paths()
    }

    fn appearance(&self) -> Result<SystemAppearance, PlatformError> {
        platform::appearance()
    }
}

fn failed(operation: &'static str, reason: impl Into<String>) -> PlatformError {
    PlatformError::Failed {
        operation,
        reason: reason.into(),
    }
}

fn output_text(operation: &'static str, output: Output) -> Result<String, PlatformError> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(failed(
            operation,
            if stderr.is_empty() {
                format!("native command exited with {}", output.status)
            } else {
                stderr
            },
        ));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim_end_matches(['\r', '\n']).to_owned())
        .map_err(|error| failed(operation, error.to_string()))
}

fn command_output(operation: &'static str, command: &mut Command) -> Result<String, PlatformError> {
    let output = command
        .output()
        .map_err(|error| failed(operation, error.to_string()))?;
    output_text(operation, output)
}

#[cfg(target_os = "windows")]
fn optional_path_output(
    operation: &'static str,
    command: &mut Command,
) -> Result<Option<PathBuf>, PlatformError> {
    command_output(operation, command).map(|path| (!path.is_empty()).then(|| PathBuf::from(path)))
}

fn spawn_detached(command: &mut Command) -> std::io::Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn command_output_raw(
    operation: &'static str,
    command: &mut Command,
) -> Result<String, PlatformError> {
    let output = command
        .output()
        .map_err(|error| failed(operation, error.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(failed(
            operation,
            if stderr.is_empty() {
                format!("native command exited with {}", output.status)
            } else {
                stderr
            },
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| failed(operation, error.to_string()))
}

fn clipboard_content(
    plain: Result<String, PlatformError>,
    html: Option<Result<String, PlatformError>>,
) -> Result<UntrustedClipboardContent, PlatformError> {
    if let (Err(error), None | Some(Err(_))) = (&plain, &html) {
        return Err(error.clone());
    }

    let mut content = UntrustedClipboardContent::empty();
    if let Ok(plain) = plain {
        content = content.with_plain_text(plain);
    }
    if let Some(Ok(html)) = html
        && !html.is_empty()
    {
        content = content.with_html(html);
    }
    Ok(content)
}

fn write_command(
    operation: &'static str,
    command: &mut Command,
    value: &str,
) -> Result<(), PlatformError> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| failed(operation, error.to_string()))?;
    use std::io::Write;
    child
        .stdin
        .take()
        .ok_or_else(|| failed(operation, "native command did not expose stdin"))?
        .write_all(value.as_bytes())
        .map_err(|error| failed(operation, error.to_string()))?;
    let output = child
        .wait_with_output()
        .map_err(|error| failed(operation, error.to_string()))?;
    output_text(operation, output).map(|_| ())
}

fn home_directory(operation: &'static str) -> Result<PathBuf, PlatformError> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| failed(operation, "HOME is not available"))
}

fn environment_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn child_path(base: impl AsRef<Path>, child: &str) -> PathBuf {
    base.as_ref().join(child)
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    pub(super) fn choose_path(request: &PathDialog) -> Result<Option<PathBuf>, PlatformError> {
        let mut zenity = Command::new("zenity");
        zenity.arg("--file-selection");
        match request.kind {
            PathDialogKind::OpenFile => {}
            PathDialogKind::OpenDirectory => {
                zenity.arg("--directory");
            }
            PathDialogKind::SaveFile => {
                zenity.arg("--save").arg("--confirm-overwrite");
            }
        }
        if let Some(title) = &request.title {
            zenity.arg(format!("--title={title}"));
        }
        match zenity.output() {
            Ok(output) if output.status.success() => output_text("choose path", output)
                .map(|path| (!path.is_empty()).then(|| PathBuf::from(path))),
            Ok(output) if output.status.code() == Some(1) => Ok(None),
            Ok(output) => output_text("choose path", output).map(|_| None),
            Err(zenity_error) => choose_path_with_kdialog(request).map_err(|kdialog_error| {
                failed(
                    "choose path",
                    format!("zenity: {zenity_error}; kdialog: {kdialog_error}"),
                )
            }),
        }
    }

    fn choose_path_with_kdialog(request: &PathDialog) -> Result<Option<PathBuf>, PlatformError> {
        let mut command = Command::new("kdialog");
        match request.kind {
            PathDialogKind::OpenFile => command.arg("--getopenfilename"),
            PathDialogKind::OpenDirectory => command.arg("--getexistingdirectory"),
            PathDialogKind::SaveFile => command.arg("--getsavefilename"),
        };
        if let Some(title) = &request.title {
            command.arg("--title").arg(title);
        }
        let output = command
            .output()
            .map_err(|error| failed("choose path", error.to_string()))?;
        if output.status.code() == Some(1) {
            return Ok(None);
        }
        output_text("choose path", output)
            .map(|path| (!path.is_empty()).then(|| PathBuf::from(path)))
    }

    pub(super) fn read_clipboard(
        formats: ClipboardFormats,
    ) -> Result<UntrustedClipboardContent, PlatformError> {
        let plain = read_clipboard_format("text/plain;charset=utf-8")
            .or_else(|_| read_clipboard_format("text/plain"));
        let html = formats
            .accepts_html()
            .then(|| read_clipboard_format("text/html"));
        clipboard_content(plain, html)
    }

    fn read_clipboard_format(mime: &str) -> Result<String, PlatformError> {
        command_output_raw(
            "read clipboard",
            Command::new("wl-paste").arg("--type").arg(mime),
        )
        .or_else(|_| {
            command_output_raw(
                "read clipboard",
                Command::new("xclip")
                    .arg("-selection")
                    .arg("clipboard")
                    .arg("-t")
                    .arg(mime)
                    .arg("-o"),
            )
        })
    }

    pub(super) fn write_clipboard(value: &str) -> Result<(), PlatformError> {
        write_command(
            "write clipboard",
            Command::new("wl-copy")
                .arg("--type")
                .arg("text/plain;charset=utf-8"),
            value,
        )
        .or_else(|_| {
            write_command(
                "write clipboard",
                Command::new("xclip")
                    .arg("-selection")
                    .arg("clipboard")
                    .arg("-in"),
                value,
            )
        })
    }

    pub(super) fn open_external(intent: &ValidatedExternalIntent) -> Result<(), PlatformError> {
        spawn_detached(Command::new("xdg-open").arg(intent.as_url()))
            .or_else(|_| spawn_detached(Command::new("gio").arg("open").arg(intent.as_url())))
            .map_err(|error| failed("open external URL", error.to_string()))
    }

    pub(super) fn application_paths() -> Result<ApplicationPaths, PlatformError> {
        let home = home_directory("application paths")?;
        let configuration =
            environment_path("XDG_CONFIG_HOME").unwrap_or_else(|| child_path(&home, ".config"));
        let data =
            environment_path("XDG_DATA_HOME").unwrap_or_else(|| child_path(&home, ".local/share"));
        let cache =
            environment_path("XDG_CACHE_HOME").unwrap_or_else(|| child_path(&home, ".cache"));
        Ok(ApplicationPaths::new(
            child_path(configuration, "parchmint"),
            child_path(data, "parchmint"),
            child_path(cache, "parchmint"),
        ))
    }

    pub(super) fn appearance() -> Result<SystemAppearance, PlatformError> {
        let color_scheme = command_output(
            "system appearance",
            Command::new("gsettings")
                .arg("get")
                .arg("org.gnome.desktop.interface")
                .arg("color-scheme"),
        )
        .or_else(|_| {
            command_output(
                "system appearance",
                Command::new("gsettings")
                    .arg("get")
                    .arg("org.gnome.desktop.interface")
                    .arg("gtk-theme"),
            )
        })?;
        if color_scheme.to_ascii_lowercase().contains("dark") {
            Ok(SystemAppearance::Dark)
        } else {
            Ok(SystemAppearance::Light)
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub(super) fn choose_path(request: &PathDialog) -> Result<Option<PathBuf>, PlatformError> {
        let script = match request.kind {
            PathDialogKind::OpenFile => {
                "on run argv\nchoose file with prompt (item 1 of argv)\nPOSIX path of result\nend run"
            }
            PathDialogKind::OpenDirectory => {
                "on run argv\nchoose folder with prompt (item 1 of argv)\nPOSIX path of result\nend run"
            }
            PathDialogKind::SaveFile => {
                "on run argv\nchoose file name with prompt (item 1 of argv)\nPOSIX path of result\nend run"
            }
        };
        let title = request.title.as_deref().unwrap_or("Choose a path");
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .arg(title)
            .output()
            .map_err(|error| failed("choose path", error.to_string()))?;
        if output.status.code() == Some(1)
            && String::from_utf8_lossy(&output.stderr).contains("User canceled")
        {
            return Ok(None);
        }
        output_text("choose path", output).map(|path| Some(PathBuf::from(path)))
    }

    pub(super) fn read_clipboard(
        formats: ClipboardFormats,
    ) -> Result<UntrustedClipboardContent, PlatformError> {
        let plain = command_output_raw("read clipboard", Command::new("pbpaste"));
        let html = formats.accepts_html().then(|| {
            command_output_raw(
                "read clipboard HTML",
                Command::new("osascript")
                    .arg("-l")
                    .arg("JavaScript")
                    .arg("-e")
                    .arg("ObjC.import('AppKit'); const value=$.NSPasteboard.generalPasteboard.stringForType('public.html'); if (value) console.log(ObjC.unwrap(value));"),
            )
        });
        clipboard_content(plain, html)
    }

    pub(super) fn write_clipboard(value: &str) -> Result<(), PlatformError> {
        write_command("write clipboard", Command::new("pbcopy"), value)
    }

    pub(super) fn open_external(intent: &ValidatedExternalIntent) -> Result<(), PlatformError> {
        spawn_detached(Command::new("open").arg(intent.as_url()))
            .map_err(|error| failed("open external URL", error.to_string()))
    }

    pub(super) fn application_paths() -> Result<ApplicationPaths, PlatformError> {
        let home = home_directory("application paths")?;
        let support = child_path(&home, "Library/Application Support/ParchMint");
        Ok(ApplicationPaths::new(
            &support,
            &support,
            child_path(&home, "Library/Caches/ParchMint"),
        ))
    }

    pub(super) fn appearance() -> Result<SystemAppearance, PlatformError> {
        let output = Command::new("defaults")
            .arg("read")
            .arg("-g")
            .arg("AppleInterfaceStyle")
            .output()
            .map_err(|error| failed("system appearance", error.to_string()))?;
        if output.status.success() {
            Ok(SystemAppearance::Dark)
        } else {
            Ok(SystemAppearance::Light)
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;

    pub(super) fn choose_path(request: &PathDialog) -> Result<Option<PathBuf>, PlatformError> {
        let script = match request.kind {
            PathDialogKind::OpenFile => {
                "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Title=$env:PARCHMINT_NATIVE_DIALOG_TITLE; if($d.ShowDialog() -eq 'OK'){[Console]::Out.Write($d.FileName)}"
            }
            PathDialogKind::OpenDirectory => {
                "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description=$env:PARCHMINT_NATIVE_DIALOG_TITLE; if($d.ShowDialog() -eq 'OK'){[Console]::Out.Write($d.SelectedPath)}"
            }
            PathDialogKind::SaveFile => {
                "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.SaveFileDialog; $d.Title=$env:PARCHMINT_NATIVE_DIALOG_TITLE; if($d.ShowDialog() -eq 'OK'){[Console]::Out.Write($d.FileName)}"
            }
        };
        optional_path_output(
            "choose path",
            Command::new("powershell.exe")
                .arg("-NoProfile")
                .arg("-NonInteractive")
                .arg("-Command")
                .arg(script)
                .env(
                    "PARCHMINT_NATIVE_DIALOG_TITLE",
                    request.title.as_deref().unwrap_or("Choose a path"),
                ),
        )
    }

    pub(super) fn read_clipboard(
        formats: ClipboardFormats,
    ) -> Result<UntrustedClipboardContent, PlatformError> {
        let plain = command_output_raw(
            "read clipboard",
            Command::new("powershell.exe")
                .arg("-NoProfile")
                .arg("-NonInteractive")
                .arg("-Command")
                .arg("[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); Get-Clipboard -Raw"),
        );
        let html = formats.accepts_html().then(|| {
            command_output_raw(
                "read clipboard HTML",
                Command::new("powershell.exe")
                    .arg("-NoProfile")
                    .arg("-NonInteractive")
                    .arg("-Command")
                    .arg("[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); Get-Clipboard -TextFormatType Html -Raw"),
            )
        });
        clipboard_content(plain, html)
    }

    pub(super) fn write_clipboard(value: &str) -> Result<(), PlatformError> {
        write_command(
            "write clipboard",
            Command::new("powershell.exe")
                .arg("-NoProfile")
                .arg("-NonInteractive")
                .arg("-Command")
                .arg("[Console]::InputEncoding=[Text.UTF8Encoding]::new($false); Set-Clipboard -Value ([Console]::In.ReadToEnd())"),
            value,
        )
    }

    pub(super) fn open_external(intent: &ValidatedExternalIntent) -> Result<(), PlatformError> {
        spawn_detached(
            Command::new("rundll32.exe")
                .arg("url.dll,FileProtocolHandler")
                .arg(intent.as_url()),
        )
        .map_err(|error| failed("open external URL", error.to_string()))
    }

    pub(super) fn application_paths() -> Result<ApplicationPaths, PlatformError> {
        let roaming = environment_path("APPDATA")
            .ok_or_else(|| failed("application paths", "APPDATA is not available"))?;
        let local = environment_path("LOCALAPPDATA")
            .ok_or_else(|| failed("application paths", "LOCALAPPDATA is not available"))?;
        Ok(ApplicationPaths::new(
            child_path(&roaming, "ParchMint"),
            child_path(&local, "ParchMint/Data"),
            child_path(&local, "ParchMint/Cache"),
        ))
    }

    pub(super) fn appearance() -> Result<SystemAppearance, PlatformError> {
        let value = command_output(
            "system appearance",
            Command::new("reg.exe")
                .arg("query")
                .arg(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
                .arg("/v")
                .arg("AppsUseLightTheme"),
        )?;
        if value.split_whitespace().last() == Some("0x0") {
            Ok(SystemAppearance::Dark)
        } else {
            Ok(SystemAppearance::Light)
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod platform {
    use super::*;

    pub(super) fn choose_path(_request: &PathDialog) -> Result<Option<PathBuf>, PlatformError> {
        Err(PlatformError::Unavailable {
            operation: "choose path",
        })
    }

    pub(super) fn read_clipboard(
        _formats: ClipboardFormats,
    ) -> Result<UntrustedClipboardContent, PlatformError> {
        Err(PlatformError::Unavailable {
            operation: "read clipboard",
        })
    }

    pub(super) fn write_clipboard(_value: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unavailable {
            operation: "write clipboard",
        })
    }

    pub(super) fn open_external(_intent: &ValidatedExternalIntent) -> Result<(), PlatformError> {
        Err(PlatformError::Unavailable {
            operation: "open external URL",
        })
    }

    pub(super) fn application_paths() -> Result<ApplicationPaths, PlatformError> {
        Err(PlatformError::Unavailable {
            operation: "application paths",
        })
    }

    pub(super) fn appearance() -> Result<SystemAppearance, PlatformError> {
        Err(PlatformError::Unavailable {
            operation: "system appearance",
        })
    }
}
