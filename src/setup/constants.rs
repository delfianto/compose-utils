// Embed templates directly into the binary.
// These files are located in src/setup/templates/ relative to this file.

pub const SERVICE_TEMPLATE: &str = include_str!("templates/compose@.service");
pub const ENV_TEMPLATE: &str = include_str!("templates/compose.env");
