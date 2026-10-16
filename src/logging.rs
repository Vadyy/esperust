use std::{fs::File, path::Path};

#[cfg(windows)]
mod windows {
    use std::{ffi::c_void, fs::{File, OpenOptions}, io::Write, os::windows::io::AsRawHandle, path::Path};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetStdHandle(kind: u32, handle: *mut c_void) -> i32;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(window: *mut c_void, text: *const u16, title: *const u16, flags: u32) -> i32;
    }

    pub(super) fn show_error(message: &str) {
        let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "ESPERUST".encode_utf16().chain(Some(0)).collect();
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10); }
    }

    pub(super) fn initialize(root: &Path) -> std::io::Result<File> {
        std::fs::create_dir_all(root.join("logs"))?;
        let path = root.join("logs/esperust.log");
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "\n--- ESPERUST startup (process {}) ---", std::process::id())?;
        for kind in [(-11_i32) as u32, (-12_i32) as u32] {
            if unsafe { SetStdHandle(kind, file.as_raw_handle()) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            previous(info);
            show_error(&format!("ESPERUST stopped unexpectedly.\n\n{info}\n\nLog: {}", path.display()));
        }));
        Ok(file)
    }
}

pub(crate) fn initialize(root: &Path) -> std::io::Result<Option<File>> {
    #[cfg(windows)]
    { windows::initialize(root).map(Some) }
    #[cfg(not(windows))]
    { let _ = root; Ok(None) }
}

pub(crate) fn report_error(message: &str) {
    eprintln!("{message}");
    #[cfg(windows)]
    windows::show_error(message);
}

pub(crate) fn fmt_layer(_app: &mut bevy::prelude::App) -> Option<bevy::log::BoxedFmtLayer> {
    #[cfg(windows)]
    { Some(Box::new(bevy::log::tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_ansi(false))) }
    #[cfg(not(windows))]
    { None }
}
