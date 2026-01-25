//! Logic for interacting with Docker images, including pulling with progress reporting.

use crate::verbose;
use anyhow::{Context as _, Result};
use bollard::query_parameters::CreateImageOptions;
use bollard::Docker;
use futures_util::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;

/// Pulls a Docker image with progress output to stdout.
///
/// Parses the image reference to extract repository and tag,
/// then streams pull progress to the terminal using multiple progress bars.
///
/// # Arguments
///
/// * `docker` - The Docker client.
/// * `image` - The full image reference (e.g., "nginx:latest" or "repo/app@sha256:...").
///
/// # Errors
///
/// Returns an error if the pull fails or if terminal output fails.
pub async fn pull_image_with_progress(docker: &Docker, image: &str) -> Result<()> {
    let (repo, tag) = parse_image_reference(image);

    println!("Pulling {}:{}...", repo, tag);
    verbose!("Initiating pull for image: {}:{}", repo, tag);

    let options = CreateImageOptions {
        from_image: Some(repo.to_string()),
        tag: Some(tag.to_string()),
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(options), None, None);
    let mut bars = HashMap::new();
    let multi = MultiProgress::new();

    let layer_style = ProgressStyle::with_template(
        "  {prefix:<12} {msg:<20} {bar:30.cyan/blue} {bytes:>10}/{total_bytes:>10}",
    )
    .unwrap_or_else(|_| ProgressStyle::default_bar())
    .progress_chars("#>-");

    while let Some(result) = stream.next().await {
        match result {
            Ok(info) => {
                let id = info.id.as_deref().unwrap_or("system");
                let status = info.status.as_deref().unwrap_or("");

                if id != "system" {
                    let pb = bars.entry(id.to_string()).or_insert_with(|| {
                        let pb = multi.add(ProgressBar::new(0));
                        pb.set_style(layer_style.clone());
                        pb.set_prefix(id.to_string());
                        pb
                    });

                    pb.set_message(status.to_string());

                    if let Some(progress) = &info.progress_detail {
                        if let (Some(current), Some(total)) = (progress.current, progress.total) {
                            pb.set_length(total as u64);
                            pb.set_position(current as u64);
                        }
                    }

                    if status.contains("Download complete")
                        || status.contains("Pull complete")
                        || status.contains("Already exists")
                    {
                        pb.finish_with_message(status.to_string());
                    }
                } else {
                    // System-wide status message
                    if !status.is_empty() {
                        multi.println(format!("  {}", status))?;
                    }
                }
            }
            Err(e) => {
                verbose!("Failed to pull image: {:?}", e);
                return Err(e).context(format!("Failed to pull {}", image));
            }
        }
    }
    verbose!("Successfully pulled image: {}:{}", repo, tag);

    Ok(())
}

/// Parses an image reference into (repository, tag).
/// Defaults to "latest" if no tag specified.
///
/// # Arguments
///
/// * `image` - The image reference string.
pub fn parse_image_reference(image: &str) -> (&str, &str) {
    if image.contains('@') {
        return (image, "");
    }

    match image.rsplit_once(':') {
        Some((repo, tag)) if !tag.contains('/') => (repo, tag),
        _ => (image, "latest"),
    }
}

/// Retrieves the digest for a local Docker image.
///
/// # Arguments
///
/// * `docker` - The Docker client.
/// * `image` - The image reference to inspect.
pub async fn get_image_digest(docker: &Docker, image: &str) -> Option<String> {
    verbose!("Inspecting image for digest: {}", image);
    match docker.inspect_image(image).await {
        Ok(info) => {
            verbose!("Found image digest: {:?}", info.id);
            info.id
        }
        Err(e) => {
            verbose!("Failed to inspect image {}: {:?}", image, e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_image_name_simple() {
        let (repo, tag) = parse_image_reference("nginx");
        assert_eq!(repo, "nginx");
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_parse_image_name_complex() {
        let (repo, tag) = parse_image_reference("nginx:1.25.3");
        assert_eq!(repo, "nginx");
        assert_eq!(tag, "1.25.3");
    }

    #[test]
    fn test_parse_image_name_with_registry() {
        let (repo, tag) = parse_image_reference("ghcr.io/username/image:latest");
        assert_eq!(repo, "ghcr.io/username/image");
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_parse_image_name_with_port() {
        let (repo, tag) = parse_image_reference("localhost:5000/image:v1");
        assert_eq!(repo, "localhost:5000/image");
        assert_eq!(tag, "v1");
    }

    #[test]
    fn test_parse_image_name_digest() {
        let (repo, tag) = parse_image_reference("nginx@sha256:abc123def456");
        assert_eq!(repo, "nginx@sha256:abc123def456");
        assert_eq!(tag, "");
    }

    #[test]
    fn test_parse_image_name_with_port_no_tag() {
        let (repo, tag) = parse_image_reference("localhost:5000/image");
        assert_eq!(repo, "localhost:5000/image");
        assert_eq!(tag, "latest");
    }
}
