use super::detect::detect_system_info;
use super::install::{run_install, InstallOptions};
use anyhow::{bail, Result};

/// Orchestrates the reinstallation process.
///
/// This behaves like `run_install` but enforces that a valid configuration
/// environment already exists. It ensures that the binary and service templates
/// are updated to the current version while preserving user configuration.
pub fn run_reinstall(opts: InstallOptions) -> Result<()> {
    let info = detect_system_info();

    if !info.env_file.exists() {
        bail!(
            "Environment file not found at {}. Cannot reinstall without existing configuration.",
            info.env_file.display()
        );
    }

    println!("Reinstalling for {} mode...", info.mode);

    // Delegate to install logic which handles updates and preserves config if present.
    run_install(opts)
}

#[cfg(test)]
mod tests {
    // Integration tests are covered by install tests since logic is delegated.
}
