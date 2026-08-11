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
        sync_autostart_in(config, RUN_KEY, APP_NAME)
    }

    fn sync_autostart_in(config: &AppConfig, key_path: &str, value_name: &str) -> io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run = hkcu.open_subkey_with_flags(key_path, KEY_SET_VALUE)?;

        if config.launch_on_startup {
            let exe = env::current_exe()?;
            run.set_value(value_name, &exe.to_string_lossy().as_ref())?;
        } else {
            match run.delete_value(value_name) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                other => other?,
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
        use winreg::RegKey;
        use crate::config::AppConfig;

        // Each test gets its own unique key path to avoid parallel-test interference.
        const KEY_ENABLE: &str = r"Software\NightscoutTrayTestEnable";
        const KEY_DISABLE: &str = r"Software\NightscoutTrayTestDisable";
        const KEY_ABSENT: &str = r"Software\NightscoutTrayTestAbsent";
        const VALUE: &str = "Autostart";

        fn config_with(launch: bool) -> AppConfig {
            AppConfig { launch_on_startup: launch, ..AppConfig::default() }
        }

        #[test]
        fn enable_writes_exe_path_to_registry() {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            hkcu.create_subkey(KEY_ENABLE).expect("create test registry key");

            sync_autostart_in(&config_with(true), KEY_ENABLE, VALUE)
                .expect("should write registry value");

            let key = hkcu.open_subkey_with_flags(KEY_ENABLE, KEY_READ).unwrap();
            let value: String = key.get_value(VALUE).expect("value should exist");
            assert!(!value.is_empty(), "exe path should not be empty");

            let _ = hkcu.delete_subkey_all(KEY_ENABLE);
        }

        #[test]
        fn disable_removes_registry_value() {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let (key, _) = hkcu.create_subkey(KEY_DISABLE).expect("create test registry key");
            let exe = std::env::current_exe().unwrap();
            key.set_value(VALUE, &exe.to_string_lossy().as_ref()).unwrap();
            drop(key);

            sync_autostart_in(&config_with(false), KEY_DISABLE, VALUE)
                .expect("should delete registry value");

            let key = hkcu.open_subkey_with_flags(KEY_DISABLE, KEY_READ).unwrap();
            let result: io::Result<String> = key.get_value(VALUE);
            assert!(result.is_err(), "value should be absent after disable");

            let _ = hkcu.delete_subkey_all(KEY_DISABLE);
        }

        #[test]
        fn disable_when_value_absent_succeeds() {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            hkcu.create_subkey(KEY_ABSENT).expect("create test registry key");

            sync_autostart_in(&config_with(false), KEY_ABSENT, VALUE)
                .expect("should succeed even when value does not exist");

            let _ = hkcu.delete_subkey_all(KEY_ABSENT);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::env;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use directories::ProjectDirs;
    use crate::config::AppConfig;

    pub fn sync_autostart(config: &AppConfig) -> io::Result<()> {
        sync_autostart_to(config, &autostart_path()?)
    }

    fn sync_autostart_to(config: &AppConfig, path: &Path) -> io::Result<()> {
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::config::AppConfig;
        use std::path::PathBuf;

        fn config_with(launch: bool) -> AppConfig {
            AppConfig { launch_on_startup: launch, ..AppConfig::default() }
        }

        fn temp_path(tag: &str) -> PathBuf {
            std::env::temp_dir().join(format!("nightscout_autostart_test_{tag}.desktop"))
        }

        #[test]
        fn enable_creates_desktop_file() {
            let path = temp_path("enable");
            let _ = std::fs::remove_file(&path);

            sync_autostart_to(&config_with(true), &path).expect("should create .desktop file");

            assert!(path.exists(), ".desktop file should exist after enable");
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(content.contains("[Desktop Entry]"));
            assert!(content.contains("X-KDE-autostart-after=panel"));

            let _ = std::fs::remove_file(&path);
        }

        #[test]
        fn enable_creates_parent_dirs() {
            let path = std::env::temp_dir()
                .join("nightscout_autostart_test_parentdir")
                .join("nightscout_tray.desktop");
            let _ = std::fs::remove_dir_all(path.parent().unwrap());

            sync_autostart_to(&config_with(true), &path).expect("should create parent dirs");

            assert!(path.exists());
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        }

        #[test]
        fn disable_removes_existing_desktop_file() {
            let path = temp_path("disable");
            std::fs::write(&path, "[Desktop Entry]\n").unwrap();

            sync_autostart_to(&config_with(false), &path).expect("should remove .desktop file");

            assert!(!path.exists(), ".desktop file should be absent after disable");
        }

        #[test]
        fn disable_when_absent_succeeds() {
            let path = temp_path("absent");
            let _ = std::fs::remove_file(&path);

            sync_autostart_to(&config_with(false), &path).expect("should succeed when file absent");
        }
    }
}

pub fn sync_autostart(config: &AppConfig) -> io::Result<()> {
    platform::sync_autostart(config)
}
