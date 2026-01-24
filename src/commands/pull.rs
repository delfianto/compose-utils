use crate::core::Context;
use anyhow::Result;
use colored::*;

/// Pull images for services without restarting
pub async fn run_pull(ctx: &Context, services: &[String]) -> Result<()> {
    let docker = crate::docker::connect_docker(ctx)?;

    let services = crate::systemd::discovery::resolve_services(ctx, services)?;
    crate::systemd::discovery::validate_compose_dirs(ctx, &services)?;

    for service in &services {
        let bare = crate::systemd::service::get_bare_name(service);
        let dir = crate::systemd::service::get_compose_dir(ctx, bare);

        println!("{} Pulling images for '{}'...", ">>".blue(), bare);

        let images = crate::compose::project::get_images_for_project(&dir)?;
        if images.is_empty() {
            println!("No images defined in compose file for '{}'", bare);
            continue;
        }

        crate::docker::images::pull_images(&docker, &images).await?;
    }

    println!("{} All images pulled successfully.", "OK".green());
    Ok(())
}
