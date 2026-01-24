use crate::core::Context;
use anyhow::Result;

pub async fn run_ps(ctx: &Context, _services: &[String]) -> Result<()> {
    let docker = crate::docker::connect_docker(ctx)?;
    let containers = crate::docker::containers::list_containers(&docker).await?;
    crate::display::table::render_containers_table(containers);
    Ok(())
}
