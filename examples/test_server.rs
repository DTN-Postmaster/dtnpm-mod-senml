use std::sync::Arc;

use dtnpm_mod_senml::SenmlModule;
use dtnpm_web_core::TestServerBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let srv = TestServerBuilder::new()
        .node_id("dtn://senml-dev/")
        .callsign("SENMLDEV")
        .with_module(Arc::new(SenmlModule::new()))
        .build()
        .await?;

    let (addr, handle) = srv.spawn_http().await?;
    println!("dtnpm-mod-senml test server listening on http://{addr}");
    println!("Capabilities: http://{addr}/api/capabilities");
    println!("SenML API:    http://{addr}/api/senml/v1/metrics");

    tokio::signal::ctrl_c().await?;
    handle.abort();
    Ok(())
}
