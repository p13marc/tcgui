//! Integration tests for the new configuration management system.
//!
//! These tests verify that the configuration system works correctly with
//! CLI argument parsing, validation, and configuration file handling.

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

/// Get the path to the backend binary, building it if necessary
fn get_backend_binary() -> Result<PathBuf> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../target/debug/tcgui-backend");

    // Build the binary if it doesn't exist or is out of date
    if !path.exists() {
        let output = Command::new("cargo")
            .args(["build", "-p", "tcgui-backend"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to build backend binary");
        }
    }

    Ok(path)
}

#[test]
fn test_backend_cli_help_works() -> Result<()> {
    // Test that the CLI help command works
    let binary_path = get_backend_binary()?;
    let output = Command::new(binary_path).args(["--help"]).output()?;

    // Help should succeed
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tcgui-backend"));
    assert!(stdout.contains("--verbose"));
    assert!(stdout.contains("--exclude-loopback"));
    assert!(stdout.contains("--name"));
    assert!(stdout.contains("--zenoh-mode"));
    assert!(stdout.contains("--zenoh-connect"));
    assert!(stdout.contains("--zenoh-listen"));

    Ok(())
}

#[test]
fn test_backend_cli_version_works() -> Result<()> {
    // Test that the CLI version command works
    let binary_path = get_backend_binary()?;
    let output = Command::new(binary_path).args(["--version"]).output()?;

    // Version should succeed
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain version info
    assert!(!stdout.is_empty());

    Ok(())
}

#[test]
fn test_backend_cli_invalid_args() -> Result<()> {
    // Test that invalid arguments are caught
    let binary_path = get_backend_binary()?;
    let output = Command::new(binary_path)
        .args(["--invalid-option"])
        .output()?;

    // Invalid arguments should fail
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should contain error message about unknown argument
    assert!(
        stderr.contains("unexpected argument")
            || stderr.contains("invalid")
            || stderr.contains("error")
    );

    Ok(())
}

#[test]
fn test_backend_cli_zenoh_mode_validation() -> Result<()> {
    // Test that invalid Zenoh modes are caught
    let binary_path = get_backend_binary()?;
    let output = Command::new(binary_path)
        .args(["--zenoh-mode", "invalid-mode"])
        .output()?;

    // Invalid mode should fail
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should contain error about invalid mode
    assert!(stderr.contains("invalid value") || stderr.contains("possible values"));

    Ok(())
}
