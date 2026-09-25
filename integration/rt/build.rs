use std::{path::PathBuf, process::Command};
fn run(command: &mut Command) {
    assert!(
        command
            .status()
            .expect("starting native build tool")
            .success(),
        "native build failed: {command:?}"
    );
}
fn main() {
    deno_runtime::deno_napi::print_linker_flags("denort");
    deno_runtime::deno_webgpu::print_linker_flags("denort");
    deno_runtime::deno_napi::print_linker_flags("dnr");
    deno_runtime::deno_webgpu::print_linker_flags("dnr");
    let root =
        PathBuf::from(std::env::var("DNR_ROOT").expect("build via cargo run -p xtask -- build"));
    use std::hash::{Hash, Hasher};
    let mut cache_id = std::collections::hash_map::DefaultHasher::new();
    for file in [
        "integration/deno.patch",
        "integration/deno.Cargo.lock",
        "integration/rt/dnr_cache.rs",
        "integration/rt/dnr.rs",
    ] {
        let path = root.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        std::fs::read(path)
            .expect("cache identity input")
            .hash(&mut cache_id);
    }
    println!(
        "cargo:rustc-env=DNR_CACHE_BUILD_ID={:016x}",
        cache_id.finish()
    );
    let backend = std::env::var("DNR_BACKEND").unwrap_or_else(|_| "webview".into());
    println!("cargo:rustc-env=DNR_BACKEND={backend}");
    println!("cargo:rerun-if-env-changed=DNR_BACKEND");
    println!("cargo:rerun-if-env-changed=DNR_ROOT");
    println!(
        "cargo:rerun-if-changed={}",
        root.join("integration/native").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join(".upstream/laufey").display()
    );
    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("native");
    let bundled_cmake = root.join(".upstream/build-tools/cmake/data/bin/cmake");
    let cmake = if bundled_cmake.exists() {
        bundled_cmake
    } else {
        "cmake".into()
    };
    let mut configure = Command::new(&cmake);
    configure
        .arg("-S")
        .arg(root.join("integration/native"))
        .arg("-B")
        .arg(&output)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg(format!("-DDNR_BACKEND={backend}"))
        .arg(format!(
            "-DLAUFEY_SOURCE_DIR={}",
            root.join(".upstream/laufey").display()
        ))
        .arg(format!(
            "-DCMAKE_INSTALL_PREFIX={}",
            output.join("install").display()
        ));
    run(&mut configure);
    run(Command::new(&cmake)
        .arg("--build")
        .arg(&output)
        .args(["--parallel", "4"]));
    run(Command::new(&cmake).arg("--install").arg(&output));
    println!(
        "cargo:rustc-link-search=native={}",
        output.join("install/lib").display()
    );
    println!("cargo:rustc-link-lib=static=dnr_laufey");
    println!("cargo:rustc-link-lib=static=laufey_backend_common");
    if cfg!(target_os = "macos") {
        for framework in [
            "Cocoa",
            "WebKit",
            "UserNotifications",
            "UniformTypeIdentifiers",
            "QuartzCore",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
        println!("cargo:rustc-link-lib=c++");
    } else {
        let packages: &[&str] = if backend == "system-cef" {
            &["gtk+-3.0", "xi", "x11"]
        } else {
            &["webkit2gtk-4.1", "gtk+-3.0"]
        };
        let out = Command::new("pkg-config")
            .arg("--libs")
            .args(packages)
            .output()
            .unwrap();
        assert!(out.status.success(), "pkg-config failed");
        for flag in String::from_utf8(out.stdout).unwrap().split_whitespace() {
            if let Some(lib) = flag.strip_prefix("-l") {
                println!("cargo:rustc-link-lib={lib}");
            } else if let Some(path) = flag.strip_prefix("-L") {
                println!("cargo:rustc-link-search=native={path}");
            } else {
                println!("cargo:rustc-link-arg-bin=dnr={flag}");
            }
        }
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-arg-bin=dnr=-Wl,--exclude-libs,ALL");
        if backend == "system-cef" {
            println!("cargo:rustc-link-search=native=/usr/lib/cef");
            println!("cargo:rustc-link-lib=cef");
            println!("cargo:rustc-link-arg-bin=dnr=-Wl,-rpath,/usr/lib/cef");
            // CEF's wrapper is built by the system CMake module.
            let wrapper = PathBuf::from(
                std::fs::read_to_string(output.join("cef-wrapper-path"))
                    .expect("CEF wrapper target path")
                    .trim(),
            );
            assert!(
                wrapper.exists(),
                "CEF wrapper archive missing: {}",
                wrapper.display()
            );
            println!("cargo:rustc-link-arg-bin=dnr={}", wrapper.display());
        }
    }
}
