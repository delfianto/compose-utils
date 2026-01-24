use anyhow::Result;
use bollard::Docker;
use bollard::query_parameters::CreateImageOptions;
use colored::*;
use futures_util::StreamExt;
use std::io::{self, Write};

/// Get the digest/ID of a local docker image using Bollard
pub async fn get_image_digest(docker: &Docker, image: &str) -> Option<String> {
    docker
        .inspect_image(image)
        .await
        .ok()
        .and_then(|info| info.id)
}

/// Pull docker images using Bollard with progress output
pub async fn pull_images(docker: &Docker, images: &[String]) -> Result<()> {
    for image in images {
        println!("Pulling {}...", image);

        // Parse image name and tag
        let (repo, tag) = if let Some(idx) = image.rfind(':') {
            // Check if the colon is part of a port (e.g., localhost:5000/image)
            let after_colon = &image[idx + 1..];
            if after_colon.contains('/') {
                // This is a port, not a tag
                (image.as_str(), "latest")
            } else {
                (&image[..idx], after_colon)
            }
        } else {
            (image.as_str(), "latest")
        };

        let options = CreateImageOptions {
            from_image: Some(repo.to_string()),
            tag: Some(tag.to_string()),
            ..Default::default()
        };

        let mut stream = docker.create_image(Some(options), None, None);
        let mut last_status = String::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    // Show progress info
                    if let Some(status) = &info.status {
                        // Build progress string from progress_detail if available
                        let progress_str = info
                            .progress_detail
                            .as_ref()
                            .and_then(|pd| match (pd.current, pd.total) {
                                (Some(current), Some(total)) if total > 0 => {
                                    Some(format!(" [{}/{}]", current, total))
                                }
                                _ => None,
                            })
                            .unwrap_or_default();
                        let id_str = info.id.as_deref().unwrap_or("");

                        let current = format!("{}: {}{}", id_str, status, progress_str);
                        if current != last_status {
                            // Clear line and print new status
                            print!("\r\x1b[K{}", current);
                            let _ = io::stdout().flush();
                            last_status = current;
                        }
                    }
                }
                Err(e) => {
                    println!();
                    eprintln!("{} Failed to pull {}: {}", "Warning:".yellow(), image, e);
                    break;
                }
            }
        }
        println!(); // New line after progress
    }
    Ok(())
}
