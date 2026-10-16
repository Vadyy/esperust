use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) fn runtime_root() -> PathBuf {
    if let Some(path) = std::env::var_os("ESPER_DATA_DIR") {
        return PathBuf::from(path);
    }
    let executable_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    for root in executable_dir
        .into_iter()
        .chain(std::env::current_dir().ok())
    {
        if root.join("tools/import_assets.py").is_file()
            || root.join("assets/player.json").is_file()
        {
            return root;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub(crate) fn ensure(root: &Path, args: &[String], original_cwd: &Path) -> Result<bool, String> {
    let explicit = args
        .iter()
        .any(|s| matches!(s.as_str(), "--resources" | "--index" | "--import-only"));
    let required = [
        "circle2.json",
        "station2.json",
        "neden1.json",
        "player.json",
        "player-animations.bin",
        "emotes/emotes.json",
        "emotes/poses.bin",
        "ui/loading/selection.json",
        "ui/verdana.ttf",
        "ui/hud/hud-source.json",
        "melee/melee.json",
        "melee/melee-animations.bin",
        "melee/reactions.json",
        "practice/practice.json",
        "practice/poses.bin",
        "ui/weapon-switch.json",
        "ui/minimap/minimap.json",
        "movement-audio.json",
        "movement-effects.json",
    ];
    if explicit
        || !required
            .iter()
            .all(|name| root.join("assets").join(name).is_file())
    {
        let distribution = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| root.to_path_buf());
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let script = [root, distribution.as_path(), script.as_path()]
            .into_iter()
            .map(|p| p.join("tools/import_assets.py"))
            .find(|p| p.is_file())
            .ok_or("Missing tools/import_assets.py. Extract the complete ESPER distribution.")?;
        let python = std::env::var_os("ESPER_PYTHON")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                for installation in [distribution.as_path(), root] {
                    let bundled = installation.join(if cfg!(windows) {
                        "runtime/python/python.exe"
                    } else {
                        "runtime/python/bin/python3"
                    });
                    if bundled.is_file() {
                        return bundled;
                    }
                    let configuration =
                        std::fs::read_to_string(installation.join("config/importer.json"))
                            .ok()
                            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
                    if let Some(value) = configuration.as_ref().and_then(|v| v["python"].as_str()) {
                        let path = Path::new(value);
                        let path = if path.is_absolute() {
                            path.to_path_buf()
                        } else {
                            installation.join(path)
                        };
                        if path.is_file() {
                            return path;
                        }
                    }
                    let relative = if cfg!(windows) {
                        ".venv/Scripts/python.exe"
                    } else {
                        ".venv/bin/python"
                    };
                    let local = installation.join(relative);
                    if local.is_file() {
                        return local;
                    }
                }
                PathBuf::from(if cfg!(windows) { "python" } else { "python3" })
            });
        let python = if cfg!(windows) && python.with_file_name("pythonw.exe").is_file() {
            python.with_file_name("pythonw.exe")
        } else { python };
        let bundled = python.starts_with(distribution.join("runtime")) || python.starts_with(root.join("runtime"));
        let mut command = Command::new(&python);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            std::fs::create_dir_all(root.join("logs")).map_err(|e| e.to_string())?;
            let log = std::fs::OpenOptions::new().create(true).append(true)
                .open(root.join("logs/importer-console.log")).map_err(|e| e.to_string())?;
            command.creation_flags(0x08000000)
                .stdin(std::process::Stdio::null())
                .stdout(log.try_clone().map_err(|e| e.to_string())?)
                .stderr(log);
        }
        if bundled {
            command.env_remove("PYTHONHOME").env_remove("PYTHONPATH").env("PYTHONNOUSERSITE", "1");
            command.arg("-s").arg("-E");
        }
        command
            .arg(script)
            .arg("--destination")
            .arg(root)
            .arg("--ensure");
        for (i, arg) in args.iter().enumerate() {
            if matches!(arg.as_str(), "--resources" | "--index") {
                let value = args
                    .get(i + 1)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| format!("{arg} needs a path"))?;
                let value = Path::new(value);
                command.arg(arg).arg(if value.is_absolute() {
                    value.to_path_buf()
                } else {
                    original_cwd.join(value)
                });
            }
        }
        let status = command.status().map_err(|e| format!("Cannot start the asset importer: {e}. Install Python and the dependencies listed in README, or set ESPER_PYTHON."))?;
        if !status.success() {
            return Err("Asset import did not complete; see the explanation above. Your previous assets were preserved.".into());
        }
    }
    std::fs::create_dir_all(root.join("config")).map_err(|e| e.to_string())?;
    let controller = root.join("config/controller.json");
    if !controller.exists() {
        std::fs::write(controller, include_str!("../config/controller.json"))
            .map_err(|e| e.to_string())?;
    }
    Ok(args.iter().any(|s| s == "--import-only"))
}
