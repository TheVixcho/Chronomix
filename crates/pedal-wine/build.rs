use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo::rustc-check-cfg=cfg(has_wine_host)");
    println!("cargo::rerun-if-changed=cpp/wine_host.cpp");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing"));
    let src = PathBuf::from("cpp/wine_host.cpp");
    let out_base = out_dir.join("chronomix-wine-host");

    let status = Command::new("wineg++")
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
                "cargo::warning=wineg++ exited with {s}; Windows VST3 bridge will be disabled"
            );
        }
        Err(e) => {
            println!(
                "cargo::warning=wineg++ not found ({e}); Windows VST3 bridge will be disabled"
            );
        }
    }
}
