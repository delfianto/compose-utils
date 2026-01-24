//! Data types representing Docker Compose file structures.

use serde::Deserialize;
use std::collections::HashMap;

/// Represents a subset of a `docker-compose.yml` file.
///
/// This structure is used primarily for extracting image information from
/// defined services.
#[derive(Deserialize, Debug)]
pub struct DockerCompose {
    /// A map of service names to their respective configurations.
    pub services: Option<HashMap<String, ComposeService>>,
}

/// Represents the configuration of an individual service in a Docker Compose file.
#[derive(Deserialize, Debug)]
pub struct ComposeService {
    /// The Docker image name assigned to this service.
    pub image: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_compose_images() {
        let yaml = r#"
services:
  web:
    image: nginx:${TAG}
  db:
    image: postgres:14
"#;

        let compose: DockerCompose = serde_yaml_ng::from_str(yaml).unwrap();
        let services = compose.services.unwrap();
        assert_eq!(
            services.get("web").unwrap().image,
            Some("nginx:${TAG}".to_string())
        );
        assert_eq!(
            services.get("db").unwrap().image,
            Some("postgres:14".to_string())
        );
    }

    #[test]
    fn test_parse_compose_no_image() {
        // Some services might use build instead of image
        let yaml = r#"
services:
  app:
    build: .
  db:
    image: postgres:14
"#;

        let compose: DockerCompose = serde_yaml_ng::from_str(yaml).unwrap();
        let services = compose.services.unwrap();
        assert_eq!(services.get("app").unwrap().image, None);
        assert_eq!(
            services.get("db").unwrap().image,
            Some("postgres:14".to_string())
        );
    }
}
