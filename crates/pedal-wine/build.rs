use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

fn find_winegpp() -> Option<PathBuf> {
    if let Ok(explicit) = env::var("CHRONOMIX_WINEGPP") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Some(p);
        }
    }
    for candidate in [
        "wineg++",
        "/usr/bin/wineg++",
        "/usr/local/bin/wineg++",
        "/opt/wine-staging/bin/wineg++",
        "/opt/wine-devel/bin/wineg++",
        "/opt/wine-stable/bin/wineg++",
    ] {
        let p = Path::new(candidate);
        if p.is_absolute() {
            if p.is_file() {
                return Some(p.to_path_buf());
            }
        } else if Command::new(candidate).arg("--version").output().is_ok() {
            return Some(PathBuf::from(candidate));
        }
    }
    None
}

fn main() {
    println!("cargo::rustc-check-cfg=cfg(has_wine_host)");
    println!("cargo::rerun-if-changed=cpp/wine_host.cpp");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing"));
    let src = PathBuf::from("cpp/wine_host.cpp");
    let out_base = out_dir.join("chronomix-wine-host");

    let Some(winegpp) = find_winegpp() else {
        println!(
            "cargo::warning=wineg++ not found at build time; Wine host can still be compiled at runtime if wineg++ is installed"
        );
        return;
    };

    let status = Command::new(&winegpp)
        .arg("-O2")
        .arg("-std=c++17")
        .arg(&src)
        .arg("-o")
        .arg(&out_base)
        .arg("-lole32")
        .arg("-lpthread")
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo::rustc-cfg=has_wine_host");
        }
        Ok(s) => {
            println!(
                "cargo::warning=wineg++ exited with {s}; prebuilt Windows VST bridge will be built at runtime if available"
            );
        }
        Err(e) => {
            println!(
                "cargo::warning=wineg++ failed ({e}); prebuilt Windows VST bridge will be built at runtime if available"
            );
        }
    }
}
