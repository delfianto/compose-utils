//! Logic for managing Docker images, including pulling and inspecting.

use anyhow::Result;
use bollard::Docker;
use bollard::query_parameters::CreateImageOptions;
use colored::*;
use futures_util::StreamExt;
use std::io::{self, Write};

/// Retrieves the unique digest/ID of a local Docker image.
///
/// # Arguments
///
/// * `docker` - A reference to the initialized [`Docker`] client.
/// * `image` - The name or ID of the image to inspect.
///
/// Returns [`Some(String)`] containing the ID if found, otherwise [`None`].
pub async fn get_image_digest(docker: &Docker, image: &str) -> Option<String> {
    docker
        .inspect_image(image)
        .await
        .ok()
        .and_then(|info| info.id)
}

/// Parses a Docker image string into its repository and tag components.
///
/// Handles standard formats (`repo:tag`) and registry ports (`localhost:5000/image`).
///
/// # Arguments
///
/// * `image` - The full image string.
///
/// Returns a tuple of `(repository, tag)`.
fn parse_image_name(image: &str) -> (&str, &str) {
    if let Some(idx) = image.rfind(':') {
        let after_colon = &image[idx + 1..];
        if after_colon.contains('/') {
            // The colon is part of a registry host/port (e.g., localhost:5000/image)
            (image, "latest")
        } else {
            (&image[..idx], after_colon)
        }
    } else {
        (image, "latest")
    }
}

/// Pulls a list of Docker images with real-time progress output to stdout.
///
/// This function iterates through the provided image list, parses their names,
/// and uses the Docker API to pull them. It prints progress updates to the terminal.
///
/// # Arguments
///
/// * `docker` - A reference to the initialized [`Docker`] client.
/// * `images` - A slice of image name strings to pull.
///
/// # Errors
///
/// Returns an error if the image pulling process fails significantly,
/// though individual image failures are often just logged as warnings.
pub async fn pull_images(docker: &Docker, images: &[String]) -> Result<()> {
    for image in images {
        println!("Pulling {}...", image);

        let (repo, tag) = parse_image_name(image);

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
                    if let Some(status) = &info.status {
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
        println!();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_image_name_simple() {
        assert_eq!(parse_image_name("nginx"), ("nginx", "latest"));
        assert_eq!(parse_image_name("nginx:1.25"), ("nginx", "1.25"));
    }

    #[test]
    fn test_parse_image_name_with_registry() {
        assert_eq!(
            parse_image_name("docker.io/library/nginx:latest"),
            ("docker.io/library/nginx", "latest")
        );
    }

    #[test]
    fn test_parse_image_name_with_port() {
        assert_eq!(
            parse_image_name("localhost:5000/my-app"),
            ("localhost:5000/my-app", "latest")
        );
        assert_eq!(
            parse_image_name("localhost:5000/my-app:v1"),
            ("localhost:5000/my-app", "v1")
        );
    }

    #[test]
    fn test_parse_image_name_complex() {
        assert_eq!(
            parse_image_name("my-registry.com:443/org/repo:tag"),
            ("my-registry.com:443/org/repo", "tag")
        );
    }
}
