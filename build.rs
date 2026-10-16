use std::{env, path::PathBuf, process::Command};

fn main() {
    for file in ["branding/esperust.ico", "branding/esperust.rc"] {
        println!("cargo:rerun-if-changed={file}");
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let resource = output.join(if msvc { "esperust.res" } else { "esperust.o" });
    let status = if msvc {
        Command::new("rc.exe")
            .current_dir("branding")
            .arg("/nologo").arg("/fo").arg(&resource).arg("esperust.rc")
            .status()
    } else {
        let compiler = env::var("WINDRES").unwrap_or_else(|_| "x86_64-w64-mingw32-windres".into());
        Command::new(compiler)
            .current_dir("branding")
            .args(["-i", "esperust.rc", "-O", "coff", "-o"]).arg(&resource)
            .status()
    }.expect("Cannot run the Windows SDK resource compiler (rc.exe) or MinGW windres");
    assert!(status.success(), "Cannot compile the ESPERUST icon");
    println!("cargo:rustc-link-arg-bin=esperust={}", resource.display());
}
