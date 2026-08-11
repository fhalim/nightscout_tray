use std::io;

use crate::config::AppConfig;

#[cfg(windows)]
mod platform {
    use std::env;
    use std::io;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    use winreg::RegKey;
    use crate::config::AppConfig;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APP_NAME: &str = "NightscoutTray";

    pub fn sync_autostart(config: &AppConfig) -> io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run = hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)?;

        if config.launch_on_startup {
            let exe = env::current_exe()?;
            run.set_value(APP_NAME, &exe.to_string_lossy().as_ref())?;
        } else {
            match run.delete_value(APP_NAME) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                other => other?,
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod platform {
    use std::env;
    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use directories::ProjectDirs;
    use crate::config::AppConfig;

    pub fn sync_autostart(config: &AppConfig) -> io::Result<()> {
        let path = autostart_path()?;

        if config.launch_on_startup {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, desktop_entry()?)
        } else if path.exists() {
            fs::remove_file(path)
        } else {
            Ok(())
        }
    }

    fn autostart_path() -> io::Result<PathBuf> {
        let project_dirs = ProjectDirs::from("", "", "nightscout_tray").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not determine the XDG config directory",
            )
        })?;

        let config_home = project_dirs
            .config_dir()
            .parent()
            .ok_or_else(|| io::Error::other("could not determine the XDG config home"))?;

        Ok(config_home
            .join("autostart")
            .join("nightscout_tray.desktop"))
    }

    fn desktop_entry() -> io::Result<String> {
        let executable = env::current_exe()?;

        Ok(format!(
            concat!(
                "[Desktop Entry]\n",
                "Type=Application\n",
                "Version=1.0\n",
                "Name=NightScout Tray\n",
                "Comment=Show the latest NightScout CGM value in the KDE tray\n",
                "Exec={}\n",
                "Terminal=false\n",
                "Categories=Utility;\n",
                "X-KDE-autostart-after=panel\n"
            ),
            executable.display()
        ))
    }
}

pub fn sync_autostart(config: &AppConfig) -> io::Result<()> {
    platform::sync_autostart(config)
}
