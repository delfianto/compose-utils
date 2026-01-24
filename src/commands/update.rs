//! Logic for the `update` command.

use super::service::run_restart;
use crate::core::Context;
use anyhow::Result;
use colored::*;
use std::collections::HashMap;

/// Executes the `update` command to pull images and restart services if changes are detected.
pub async fn run_update(ctx: &Context, services: &[String]) -> Result<()> {
    let docker = crate::docker::connect_docker(ctx)?;

    let services = crate::systemd::discovery::resolve_services(ctx, services)?;
    crate::systemd::discovery::validate_compose_dirs(ctx, &services)?;

    let mut services_to_restart = Vec::new();

    for service in &services {
        let bare = crate::systemd::service::get_bare_name(service);
        let dir = crate::systemd::service::get_compose_dir(ctx, bare);

        println!("{} Checking for updates: '{}'...", ">>".blue(), bare);

        let images = crate::compose::project::get_images_for_project(&dir)?;
        if images.is_empty() {
            println!("No images defined in compose file for '{}'", bare);
            continue;
        }

        let mut pre_pull_hashes: HashMap<String, Option<String>> = HashMap::new();
        for image in &images {
            let hash = crate::docker::images::get_image_digest(&docker, image).await;
            pre_pull_hashes.insert(image.clone(), hash);
        }

        for image in &images {
            crate::docker::images::pull_image_with_progress(&docker, image).await?;
        }

        let mut updated = false;
        for image in &images {
            let old_hash = pre_pull_hashes.get(image).and_then(|h| h.as_ref());
            let new_hash = crate::docker::images::get_image_digest(&docker, image).await;

            match (old_hash, new_hash.as_ref()) {
                (Some(old), Some(new)) if old != new => {
                    println!(
                        "{} Image updated: {} ({} -> {})",
                        "+".green(),
                        image,
                        shorten_hash(old),
                        shorten_hash(new)
                    );
                    updated = true;
                }
                (None, Some(new)) => {
                    println!(
                        "{} New image downloaded: {} ({})",
                        "+".green(),
                        image,
                        shorten_hash(new)
                    );
                    updated = true;
                }
                _ => {}
            }
        }

        if updated {
            services_to_restart.push(service.clone());
        } else {
            println!("{} '{}' is already up to date.", "OK".green(), bare);
        }
    }

    if !services_to_restart.is_empty() {
        println!("\nRestarting updated services...");
        run_restart(ctx, &services_to_restart).await?;
        println!(
            "{} Updated and restarted {} service(s).",
            "OK".green(),
            services_to_restart.len()
        );
    } else {
        println!("\n{} All services are already up to date.", "OK".green());
    }

    Ok(())
}

/// Truncates a long hash string (like a Docker image ID) for more concise display.
fn shorten_hash(hash: &str) -> String {
    let hash = hash.strip_prefix("sha256:").unwrap_or(hash);
    if hash.len() > 12 {
        hash[..12].to_string()
    } else {
        hash.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shorten_hash() {
        assert_eq!(shorten_hash("sha256:abc123def456xyz789"), "abc123def456");
        assert_eq!(shorten_hash("short"), "short");
    }
}
