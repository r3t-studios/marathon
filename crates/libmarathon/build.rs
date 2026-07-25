use std::{
    env,
    path::PathBuf,
    process::Command,
};

fn main() {
    // Only compile Swift code when building for iOS
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os != "ios" {
        println!("cargo:warning=Not building for iOS, skipping Swift compilation");
        return;
    }

    println!("cargo:warning=Building Swift bridge for iOS");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let swift_src = manifest_dir.join("src/platform/ios/PencilCapture.swift");
    let header = manifest_dir.join("src/platform/ios/PencilBridge.h");
    let object_file = out_dir.join("PencilCapture.o");

    // Get the iOS SDK path
    let sdk_output = Command::new("xcrun")
        .args(&["--sdk", "iphoneos", "--show-sdk-path"])
        .output()
        .expect("Failed to get iOS SDK path");

    let sdk_path = String::from_utf8_lossy(&sdk_output.stdout)
        .trim()
        .to_string();

    println!("cargo:warning=Using iOS SDK: {}", sdk_path);

    // Compile Swift to object file
    let status = Command::new("swiftc")
        .args(&[
            "-sdk",
            &sdk_path,
            "-target",
            "arm64-apple-ios14.0", // Minimum iOS 14
            "-import-objc-header",
            header.to_str().unwrap(),
            "-parse-as-library",
            "-c",
            swift_src.to_str().unwrap(),
            "-o",
            object_file.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to compile Swift");

    if !status.success() {
        panic!("Swift compilation failed");
    }

    println!("cargo:warning=Swift compilation succeeded");

    // Create static library from object file
    let lib_file = out_dir.join("libPencilCapture.a");
    let ar_status = Command::new("ar")
        .args(&[
            "rcs",
            lib_file.to_str().unwrap(),
            object_file.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to create static library");

    if !ar_status.success() {
        panic!("Failed to create static library");
    }

    println!(
        "cargo:warning=Created static library: {}",
        lib_file.display()
    );

    // Tell Cargo to link the static library
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=PencilCapture");

    // Link Swift standard library
    println!("cargo:rustc-link-lib=dylib=swiftCore");
    println!("cargo:rustc-link-lib=dylib=swiftFoundation");
    println!("cargo:rustc-link-lib=dylib=swiftUIKit");

    // Link iOS frameworks
    println!("cargo:rustc-link-lib=framework=UIKit");
    println!("cargo:rustc-link-lib=framework=Foundation");

    // Rerun if Swift files change
    println!("cargo:rerun-if-changed={}", swift_src.display());
    println!("cargo:rerun-if-changed={}", header.display());
}
