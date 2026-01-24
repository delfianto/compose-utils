use serde::Deserialize;
use std::collections::HashMap;

/// Struct representing a subset of docker-compose.yml structure for image parsing
#[derive(Deserialize, Debug)]
pub struct DockerCompose {
    pub services: Option<HashMap<String, ComposeService>>,
}

#[derive(Deserialize, Debug)]
pub struct ComposeService {
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
