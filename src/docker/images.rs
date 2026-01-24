//! Logic for interacting with Docker images, including pulling with progress reporting.

use anyhow::{Context as _, Result};
use bollard::query_parameters::CreateImageOptions;
use bollard::Docker;
use futures_util::StreamExt;
use std::io::{self, Write};

/// Pulls a Docker image with progress output to stdout.
///
/// Parses the image reference to extract repository and tag,
/// then streams pull progress to the terminal.
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

    let options = CreateImageOptions {
        from_image: Some(repo.to_string()),
        tag: Some(tag.to_string()),
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(options), None, None);

    while let Some(result) = stream.next().await {
        match result {
            Ok(info) => {
                if let Some(status) = &info.status {
                    // Print progress on same line
                    print!("\r  {} ", status);
                    if let Some(progress) = &info.progress_detail {
                        if let (Some(current), Some(total)) = (progress.current, progress.total) {
                            print!("[{}/{}]", current, total);
                        }
                    }
                    io::stdout().flush()?;
                }
            }
            Err(e) => {
                println!();
                return Err(e).context(format!("Failed to pull {}", image));
            }
        }
    }

    println!("\r  Pulled {}:{}", repo, tag);
    Ok(())
}

/// Parses an image reference into (repository, tag).
/// Defaults to "latest" if no tag specified.
///
/// # Arguments
///
/// * `image` - The image reference string.
pub fn parse_image_reference(image: &str) -> (&str, &str) {
    // Handle digests (repo@sha256:...)
    if image.contains('@') {
        return (image, "");
    }

    // Handle tags (repo:tag)
    match image.rsplit_once(':') {
        Some((repo, tag)) if !repo.contains('/') || !tag.contains('/') => (repo, tag),
        _ => (image, "latest"),
    }
}

/// Retrieves the digest for a local Docker image.
pub async fn get_image_digest(docker: &Docker, image: &str) -> Option<String> {
    match docker.inspect_image(image).await {
        Ok(info) => info.id,
        Err(_) => None,
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
}
