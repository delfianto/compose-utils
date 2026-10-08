//! Internal commands called by systemd unit templates.

use crate::commands::compose_direct::build_compose_command;
use crate::core::{Context, context::read_env_file};
use anyhow::Result;
use std::os::unix::process::CommandExt;
use std::process::Command;

fn service_command(ctx: &Context, name: &str, args: &[&str]) -> Result<Command> {
    let mut cmd = build_compose_command(ctx, name, args);
    // The unit's EnvironmentFile exports global defaults as shell variables.
    // Read them through --env-file instead, so project overrides can win.
    for key in read_env_file(&ctx.env_file)?.keys() {
        if key != "DOCKER_HOST" {
            cmd.env_remove(key);
        }
    }
    Ok(cmd)
}

pub fn run_service(ctx: &Context, name: &str) -> Result<()> {
    let mut cmd = service_command(ctx, name, &["up"])?;
    let err = cmd.exec();
    Err(anyhow::anyhow!("Failed to exec compose command: {}", err))
}

pub fn stop_service(ctx: &Context, service: &str) -> Result<()> {
    let mut cmd = service_command(ctx, service, &["down", "--remove-orphans"])?;
    let err = cmd.exec();
    Err(anyhow::anyhow!("Failed to exec compose command: {}", err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_globals_do_not_override_project_files_or_daemon_selection() {
        let temp = tempfile::tempdir().unwrap();
        let env_file = temp.path().join("compose.env");
        std::fs::write(
            &env_file,
            "DATA_DIR=/global\nDOCKER_HOST=unix:///run/user/1000/docker.sock\n",
        )
        .unwrap();
        let ctx = Context {
            is_root: false,
            compose_base: temp.path().to_owned(),
            systemd_dir: temp.path().to_owned(),
            env_file,
            docker_host: Some("unix:///run/user/1000/docker.sock".to_owned()),
        };
        let cmd = service_command(&ctx, "app", &["up"]).unwrap();
        let envs = cmd.get_envs().collect::<std::collections::HashMap<_, _>>();
        assert_eq!(envs[std::ffi::OsStr::new("DATA_DIR")], None);
        assert_eq!(
            envs[std::ffi::OsStr::new("DOCKER_HOST")],
            Some(std::ffi::OsStr::new("unix:///run/user/1000/docker.sock"))
        );
    }
}
