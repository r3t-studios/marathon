use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command;
use std::fs;
use tracing::{info, warn, error};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Build automation for Aspen", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build for iOS Simulator
    IosBuild {
        /// Build in debug mode instead of release
        #[arg(long)]
        debug: bool,
    },
    /// Deploy to iOS Simulator
    IosDeploy {
        /// Device name (defaults to "iPad Pro 12.9-inch M2")
        #[arg(long, default_value = "iPad Pro 12.9-inch M2")]
        device: String,
        /// Use debug build instead of release
        #[arg(long)]
        debug: bool,
    },
    /// Build, deploy, and stream logs from iOS Simulator
    IosRun {
        /// Device name (defaults to "iPad Pro 12.9-inch M2")
        #[arg(long, default_value = "iPad Pro 12.9-inch M2")]
        device: String,
        /// Build in debug mode instead of release
        #[arg(long)]
        debug: bool,
    },
    /// Build, deploy, and stream logs from physical iOS device
    IosDevice {
        /// Build in debug mode instead of release
        #[arg(long)]
        debug: bool,
    },
}

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("xtask=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::IosBuild { debug } => ios_build(debug),
        Commands::IosDeploy { device, debug } => ios_deploy(&device, debug),
        Commands::IosRun { device, debug } => ios_run(&device, debug),
        Commands::IosDevice { debug } => ios_device(debug),
    }
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("Failed to find project root")
        .to_path_buf()
}

fn package_app_bundle(profile: &str, target: &str) -> Result<()> {
    let root = project_root();
    let app_name = "Aspen";

    let binary_path = root.join(format!("target/{}/{}/app", target, profile));
    let bundle_path = root.join(format!("target/{}/{}/{}.app", target, profile, app_name));
    let info_plist_src = root.join("scripts/ios/Info.plist");

    info!("Packaging app bundle");
    info!("  Binary: {}", binary_path.display());
    info!("  Bundle: {}", bundle_path.display());

    // Verify binary exists
    if !binary_path.exists() {
        anyhow::bail!("Binary not found at {}", binary_path.display());
    }

    // Remove old bundle if it exists
    if bundle_path.exists() {
        fs::remove_dir_all(&bundle_path)
            .context("Failed to remove old app bundle")?;
    }

    // Create bundle directory
    fs::create_dir_all(&bundle_path)
        .context("Failed to create app bundle directory")?;

    // Copy binary
    let bundle_binary = bundle_path.join("app");
    fs::copy(&binary_path, &bundle_binary)
        .context("Failed to copy binary to bundle")?;

    // Set executable permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&bundle_binary)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bundle_binary, perms)
            .context("Failed to set executable permissions")?;
    }

    // Copy Info.plist
    fs::copy(&info_plist_src, bundle_path.join("Info.plist"))
        .context("Failed to copy Info.plist")?;

    // Create PkgInfo
    fs::write(bundle_path.join("PkgInfo"), b"APPL????")
        .context("Failed to create PkgInfo")?;

    info!("App bundle packaged successfully");
    Ok(())
}

fn ios_build(debug: bool) -> Result<()> {
    let root = project_root();
    let profile = if debug { "debug" } else { "release" };

    info!("Building for iOS Simulator");
    info!("Project root: {}", root.display());

    let mut cmd = Command::new("cargo");
    cmd.current_dir(&root)
        .arg("build")
        .arg("--package")
        .arg("app")
        .arg("--target")
        .arg("aarch64-apple-ios-sim");

    if !debug {
        cmd.arg("--release");
    }

    let status = cmd.status().context("Failed to run cargo build")?;

    if !status.success() {
        anyhow::bail!("Build failed");
    }

    // Package the app bundle
    package_app_bundle(profile, "aarch64-apple-ios-sim")?;

    info!("Build complete: target/aarch64-apple-ios-sim/{}/Aspen.app", profile);
    Ok(())
}

fn ios_deploy(device_name: &str, debug: bool) -> Result<()> {
    let root = project_root();
    let profile = if debug { "debug" } else { "release" };
    let bundle_path = root.join(format!("target/aarch64-apple-ios-sim/{}/Aspen.app", profile));

    info!("Deploying to iOS Simulator");
    info!("Device: {}", device_name);
    info!("Bundle: {}", bundle_path.display());

    // Find device UUID
    info!("Finding device UUID");
    let output = Command::new("xcrun")
        .args(["simctl", "list", "devices", "available"])
        .output()
        .context("Failed to list devices")?;

    let devices_list = String::from_utf8_lossy(&output.stdout);
    let device_uuid = devices_list
        .lines()
        .find(|line| line.contains(device_name))
        .and_then(|line| {
            line.split('(')
                .nth(1)?
                .split(')')
                .next()
        })
        .context("Device not found")?;

    info!("Found device: {}", device_uuid);

    // Boot simulator
    info!("Booting simulator");
    let boot_result = Command::new("xcrun")
        .args(["simctl", "boot", device_uuid])
        .status();

    if let Ok(status) = boot_result {
        if !status.success() {
            warn!("Simulator boot returned non-zero (device may already be booted)");
        }
    }

    // Open Simulator.app
    info!("Opening Simulator.app");
    Command::new("open")
        .args(["-a", "Simulator"])
        .status()
        .context("Failed to open Simulator")?;

    // Uninstall old version
    info!("Uninstalling old version");
    let uninstall_result = Command::new("xcrun")
        .args(["simctl", "uninstall", device_uuid, "G872CZV7WG.aspen"])
        .status();

    if let Ok(status) = uninstall_result {
        if !status.success() {
            warn!("Uninstall returned non-zero (app may not have been installed)");
        }
    }

    // Install app
    info!("Installing app");
    let status = Command::new("xcrun")
        .args(["simctl", "install", device_uuid])
        .arg(&bundle_path)
        .status()
        .context("Failed to install app")?;

    if !status.success() {
        anyhow::bail!("Installation failed");
    }

    // Launch app
    info!("Launching app");
    let output = Command::new("xcrun")
        .args(["simctl", "launch", device_uuid, "G872CZV7WG.aspen"])
        .output()
        .context("Failed to launch app")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("Launch failed: {}", stderr);
        anyhow::bail!("Launch failed");
    }

    info!("App launched successfully");
    Ok(())
}

fn ios_run(device_name: &str, debug: bool) -> Result<()> {
    // Build
    ios_build(debug)?;

    // Deploy
    ios_deploy(device_name, debug)?;

    // Stream logs
    info!("Streaming logs (Ctrl+C to stop)");

    // Find device UUID again
    let output = Command::new("xcrun")
        .args(["simctl", "list", "devices", "available"])
        .output()
        .context("Failed to list devices")?;

    let devices_list = String::from_utf8_lossy(&output.stdout);
    let device_uuid = devices_list
        .lines()
        .find(|line| line.contains(device_name))
        .and_then(|line| {
            line.split('(')
                .nth(1)?
                .split(')')
                .next()
        })
        .context("Device not found")?;

    // Stream logs using simctl spawn
    // Note: Process name is "app" (CFBundleExecutable), not "Aspen" (display name)
    let mut log_cmd = Command::new("xcrun");
    log_cmd.args([
        "simctl",
        "spawn",
        device_uuid,
        "log",
        "stream",
        "--predicate",
        "process == \"app\"",
        "--style",
        "compact",
    ]);

    // When debug flag is set, show all log levels (info, debug, default, error, fault)
    if debug {
        log_cmd.args(["--info", "--debug"]);
        info!("Streaming all log levels (info, debug, default, error, fault)");
    } else {
        info!("Streaming default log levels (default, error, fault)");
    }

    let status = log_cmd.status().context("Failed to start log stream")?;

    // Log streaming can exit for legitimate reasons:
    // - User quit the simulator
    // - App was killed
    // - User hit Ctrl+C
    // Don't treat these as errors
    match status.code() {
        Some(0) => info!("Log streaming ended normally"),
        Some(code) => info!("Log streaming ended with code {}", code),
        None => info!("Log streaming ended (killed by signal)"),
    }

    Ok(())
}

fn ios_device(debug: bool) -> Result<()> {
    let root = project_root();
    let profile = if debug { "debug" } else { "release" };
    let target = "aarch64-apple-ios";

    info!("Building for physical iOS device");
    info!("Project root: {}", root.display());

    // Build
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&root)
        .arg("build")
        .arg("--package")
        .arg("app")
        .arg("--target")
        .arg(target);

    if !debug {
        cmd.arg("--release");
    }

    let status = cmd.status().context("Failed to run cargo build")?;

    if !status.success() {
        anyhow::bail!("Build failed");
    }

    // Package the app bundle
    package_app_bundle(profile, target)?;

    let bundle_path = root.join(format!("target/{}/{}/Aspen.app", target, profile));
    info!("Build complete: {}", bundle_path.display());

    // Code sign with Apple Development certificate
    info!("Signing app with Apple Development certificate");
    let status = Command::new("codesign")
        .args(["--force", "--sign", "Apple Development: sienna@r3t.io (6A6PF29R8A)"])
        .arg(&bundle_path)
        .status()
        .context("Failed to code sign app")?;

    if !status.success() {
        anyhow::bail!("Code signing failed. Make sure the certificate is trusted in Keychain Access.");
    }

    info!("App signed successfully");

    // Get connected device
    info!("Finding connected device");
    let output = Command::new("xcrun")
        .args(["devicectl", "list", "devices"])
        .output()
        .context("Failed to list devices")?;

    let devices_list = String::from_utf8_lossy(&output.stdout);
    let device_id = devices_list
        .lines()
        .find(|line| (line.contains("connected") || line.contains("available")) && line.contains("iPad"))
        .and_then(|line| {
            // Extract device ID from the line
            // Format: "Name    domain.coredevice.local    ID    connected/available    Model"
            line.split_whitespace().nth(2)
        })
        .context("No connected iPad found. Make sure your iPad is connected and trusted.")?;

    info!("Found device: {}", device_id);

    // Install app
    info!("Installing app to device");
    let status = Command::new("xcrun")
        .args(["devicectl", "device", "install", "app"])
        .arg("--device")
        .arg(device_id)
        .arg(&bundle_path)
        .status()
        .context("Failed to install app")?;

    if !status.success() {
        anyhow::bail!("Installation failed. Make sure Developer Mode is enabled on your iPad.");
    }

    info!("App installed successfully");

    // Launch app
    info!("Launching app");
    let status = Command::new("xcrun")
        .args(["devicectl", "device", "process", "launch"])
        .arg("--device")
        .arg(device_id)
        .arg("G872CZV7WG.aspen")
        .status()
        .context("Failed to launch app")?;

    if !status.success() {
        anyhow::bail!("Launch failed");
    }

    info!("App launched successfully");

    // Stream logs
    info!("Streaming device logs (Ctrl+C to stop)");
    info!("Note: Device logs may be delayed. Use Console.app for real-time logs.");

    let mut log_cmd = Command::new("xcrun");
    log_cmd.args([
        "devicectl",
        "device",
        "stream",
        "log",
        "--device",
        device_id,
        "--predicate",
        "process == \"app\"",
    ]);

    if debug {
        info!("Streaming all log levels");
    }

    let status = log_cmd.status().context("Failed to start log stream")?;

    match status.code() {
        Some(0) => info!("Log streaming ended normally"),
        Some(code) => info!("Log streaming ended with code {}", code),
        None => info!("Log streaming ended (killed by signal)"),
    }

    Ok(())
}
