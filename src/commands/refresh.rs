//! Recreate running Compose services after their local image tags move.

use crate::commands::compose_direct::build_compose_command_in_dir;
use crate::core::{Context, Report};
use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::process::{Command, Output};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Container {
    name: String,
    image: String,
    config: ContainerConfig,
    state: ContainerState,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerConfig {
    image: String,
    labels: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerState {
    running: bool,
    paused: bool,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Project {
    name: String,
    dir: PathBuf,
    files: Vec<PathBuf>,
}

#[derive(Debug, Serialize)]
struct RestartResult {
    project: Option<String>,
    service: String,
    status: &'static str,
    reason: Option<String>,
}

fn docker_command(ctx: &Context, args: &[&str]) -> Command {
    let mut cmd = Command::new("docker");
    cmd.args(args);
    if let Some(host) = &ctx.docker_host {
        cmd.env("DOCKER_HOST", host);
    }
    cmd
}

fn checked_output(
    cmd: &mut Command,
    execute: &mut impl FnMut(&mut Command) -> Result<Output>,
) -> Result<Output> {
    let output = execute(cmd).with_context(|| format!("Failed to execute {cmd:?}"))?;
    if !output.status.success() {
        bail!(
            "{cmd:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

fn project_service(container: &Container) -> Option<(Project, String)> {
    let labels = container.config.labels.as_ref()?;
    let label = |key: &str| {
        labels
            .get(&format!("com.docker.compose.{key}"))
            .filter(|value| !value.is_empty())
    };
    if label("oneoff").is_some_and(|value| value.eq_ignore_ascii_case("true")) {
        return None;
    }
    let dir = PathBuf::from(label("project.working_dir")?);
    if !dir.is_absolute() {
        return None;
    }
    let files: Vec<_> = label("project.config_files")?
        .split(',')
        .map(|file| dir.join(file))
        .collect();
    Some((
        Project {
            name: label("project")?.clone(),
            dir,
            files,
        },
        label("service")?.clone(),
    ))
}

fn restart_with(
    ctx: &Context,
    dry_run: bool,
    mut execute: impl FnMut(&mut Command) -> Result<Output>,
) -> Result<Vec<RestartResult>> {
    let output = checked_output(
        &mut docker_command(ctx, &["ps", "--quiet", "--no-trunc"]),
        &mut execute,
    )?;
    let ids = String::from_utf8(output.stdout).context("Invalid docker ps output")?;
    let ids: Vec<_> = ids.split_whitespace().collect();
    let mut projects: BTreeMap<Project, BTreeSet<String>> = BTreeMap::new();
    let mut images: HashMap<String, Option<String>> = HashMap::new();
    let mut results = Vec::new();

    // Finish discovery before changing any containers. Limit command-line size.
    for batch in ids.chunks(100) {
        let output = checked_output(
            docker_command(ctx, &["container", "inspect"]).args(batch),
            &mut execute,
        )?;
        let containers: Vec<Container> =
            serde_json::from_slice(&output.stdout).context("Invalid container inspect output")?;
        for container in containers {
            if !container.state.running || container.state.paused {
                continue;
            }
            let image = &container.config.image;
            if !images.contains_key(image) {
                let output = execute(&mut docker_command(
                    ctx,
                    &["image", "inspect", "--format", "{{.Id}}", image],
                ))?;
                let current = if output.status.success() {
                    let id = String::from_utf8(output.stdout)?.trim().to_owned();
                    if id.is_empty() {
                        bail!("Empty image ID returned for {image}");
                    }
                    Some(id)
                } else if String::from_utf8_lossy(&output.stderr).contains("No such image") {
                    None
                } else {
                    bail!(
                        "Failed to inspect image {image}: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    );
                };
                images.insert(image.clone(), current);
            }
            let Some(current) = &images[image] else {
                results.push(RestartResult {
                    project: None,
                    service: container.name.trim_start_matches('/').to_owned(),
                    status: "skipped",
                    reason: Some(format!("Image {image} is unavailable locally")),
                });
                continue;
            };
            if current == &container.image {
                continue;
            }
            if let Some((project, service)) = project_service(&container) {
                projects.entry(project).or_default().insert(service);
            } else {
                results.push(RestartResult {
                    project: None,
                    service: container.name.trim_start_matches('/').to_owned(),
                    status: "skipped",
                    reason: Some(
                        "No reusable Compose project metadata (or one-off container)".into(),
                    ),
                });
            }
        }
    }

    for (project, services) in projects {
        if !dry_run {
            let mut cmd = build_compose_command_in_dir(ctx, &project.dir, &[]);
            cmd.arg("--project-name").arg(&project.name);
            for file in &project.files {
                cmd.arg("--file").arg(file);
            }
            cmd.args(["up", "-d", "--no-deps", "--no-build", "--pull", "never"]);
            cmd.args(&services);
            let output = checked_output(&mut cmd, &mut execute)?;
            // Keep stdout available for the JSON report.
            eprint!("{}", String::from_utf8_lossy(&output.stdout));
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        for service in services {
            results.push(RestartResult {
                project: Some(project.name.clone()),
                service,
                status: if dry_run {
                    "would_restart"
                } else {
                    "restarted"
                },
                reason: None,
            });
        }
    }
    Ok(results)
}

pub fn run(ctx: &Context, dry_run: bool) -> Result<()> {
    let results = restart_with(ctx, dry_run, |cmd| Ok(cmd.output()?))?;
    if crate::core::is_json() {
        crate::core::print_json(&Report {
            command: "refresh",
            results,
        })?;
    } else if results.is_empty() {
        println!("No running containers have outdated local images.");
    } else {
        for result in results {
            let project = result.project.as_deref().unwrap_or("container");
            println!("{} {}/{}", result.status, project, result.service);
            if let Some(reason) = result.reason {
                println!("  {reason}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    fn ctx() -> Context {
        Context {
            is_root: false,
            systemd_dir: "/unused".into(),
            compose_base: "/unused".into(),
            env_file: "/missing/compose.env".into(),
            docker_host: Some("unix:///test/docker.sock".into()),
        }
    }

    fn output(value: impl Into<Vec<u8>>) -> Output {
        Output {
            status: ExitStatus::from_raw(0),
            stdout: value.into(),
            stderr: Vec::new(),
        }
    }

    fn failure(message: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(256),
            stdout: Vec::new(),
            stderr: message.as_bytes().to_vec(),
        }
    }

    fn container(service: &str, image: &str, running: bool) -> Value {
        json!({
            "Name": format!("/demo-{service}"),
            "Image": image,
            "State": {"Running": running, "Paused": false},
            "Config": {
                "Image": "example/app:latest",
                "Labels": {
                    "com.docker.compose.project": "custom-project",
                    "com.docker.compose.service": service,
                    "com.docker.compose.project.working_dir": "/srv/projects/demo",
                    "com.docker.compose.project.config_files": "/srv/projects/demo/compose.yaml,override.yaml"
                }
            }
        })
    }

    fn args(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn recreates_only_outdated_running_services_and_groups_replicas() {
        let mut calls = Vec::new();
        let mut paused = container("paused", "sha256:old", true);
        paused["State"]["Paused"] = json!(true);
        let containers = vec![
            container("app", "sha256:old", true),
            container("app", "sha256:old", true),
            container("worker", "sha256:old", true),
            container("current", "sha256:new", true),
            container("stopped", "sha256:old", false),
            paused,
        ];
        let results = restart_with(&ctx(), false, |cmd| {
            assert!(cmd.get_envs().any(|(key, value)| {
                key == "DOCKER_HOST" && value == Some("unix:///test/docker.sock".as_ref())
            }));
            let argv = args(cmd);
            calls.push(argv.clone());
            Ok(match argv[0].as_str() {
                "ps" => output("id1\nid2\nid3\nid4\nid5\nid6\n"),
                "container" => output(serde_json::to_vec(&containers)?),
                "image" => output("sha256:new\n"),
                "compose" => {
                    assert_eq!(
                        cmd.get_current_dir(),
                        Some(std::path::Path::new("/srv/projects/demo"))
                    );
                    output("")
                }
                _ => panic!("Unexpected command: {argv:?}"),
            })
        })
        .unwrap();
        assert_eq!(calls.len(), 4, "image inspection should be cached");
        assert_eq!(calls[0], ["ps", "--quiet", "--no-trunc"]);
        assert_eq!(
            calls[3],
            [
                "compose",
                "--project-name",
                "custom-project",
                "--file",
                "/srv/projects/demo/compose.yaml",
                "--file",
                "/srv/projects/demo/override.yaml",
                "up",
                "-d",
                "--no-deps",
                "--no-build",
                "--pull",
                "never",
                "app",
                "worker"
            ]
        );
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| result.status == "restarted"));
    }

    #[test]
    fn dry_run_only_inspects_and_reports_affected_services() {
        let results = restart_with(&ctx(), true, |cmd| {
            Ok(match args(cmd)[0].as_str() {
                "ps" => output("id\n"),
                "container" => output(serde_json::to_vec(&vec![container("app", "old", true)])?),
                "image" => output("new\n"),
                _ => panic!("Dry run must not recreate containers"),
            })
        })
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "would_restart");
        assert_eq!(results[0].project.as_deref(), Some("custom-project"));
    }

    #[test]
    fn empty_daemon_never_inspects_or_recreates() {
        let results = restart_with(&ctx(), false, |cmd| {
            assert_eq!(args(cmd)[0], "ps");
            Ok(output(""))
        })
        .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn current_images_do_not_recreate_services() {
        let results = restart_with(&ctx(), false, |cmd| {
            Ok(match args(cmd)[0].as_str() {
                "ps" => output("id\n"),
                "container" => output(serde_json::to_vec(&vec![container("app", "same", true)])?),
                "image" => output("same\n"),
                _ => panic!("Current containers must be left alone"),
            })
        })
        .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn skips_standalone_oneoff_and_missing_local_images() {
        let mut standalone = container("standalone", "old", true);
        standalone["Config"]["Labels"] = Value::Null;
        let mut oneoff = container("oneoff", "old", true);
        oneoff["Config"]["Labels"]["com.docker.compose.oneoff"] = json!("True");
        let mut missing = container("missing", "old", true);
        missing["Config"]["Image"] = json!("missing:latest");
        let results = restart_with(&ctx(), false, |cmd| {
            let argv = args(cmd);
            Ok(match argv[0].as_str() {
                "ps" => output("a\nb\nc\n"),
                "container" => output(serde_json::to_vec(&vec![
                    standalone.clone(),
                    oneoff.clone(),
                    missing.clone(),
                ])?),
                "image" if argv.last().unwrap() == "missing:latest" => {
                    failure("Error: No such image: missing:latest")
                }
                "image" => output("new\n"),
                _ => panic!("Skipped containers must not be recreated"),
            })
        })
        .unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|result| result.status == "skipped"));
        assert!(
            results[2]
                .reason
                .as_ref()
                .unwrap()
                .contains("unavailable locally")
        );
    }

    #[test]
    fn docker_inspection_failures_abort_before_recreation() {
        for failing_command in ["ps", "container", "image"] {
            let result = restart_with(&ctx(), false, |cmd| {
                let argv = args(cmd);
                if argv[0] == failing_command {
                    return Ok(failure("daemon unavailable"));
                }
                Ok(match argv[0].as_str() {
                    "ps" => output("id\n"),
                    "container" => {
                        output(serde_json::to_vec(&vec![container("app", "old", true)])?)
                    }
                    _ => panic!("Must abort before recreation"),
                })
            });
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("daemon unavailable")
            );
        }
    }

    #[test]
    fn recreation_failure_is_propagated() {
        let result = restart_with(&ctx(), false, |cmd| {
            Ok(match args(cmd)[0].as_str() {
                "ps" => output("id\n"),
                "container" => output(serde_json::to_vec(&vec![container("app", "old", true)])?),
                "image" => output("new\n"),
                "compose" => failure("compose file is missing"),
                _ => unreachable!(),
            })
        });
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("compose file is missing")
        );
    }
}
