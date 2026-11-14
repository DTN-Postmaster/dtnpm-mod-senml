use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use dtnpm_core::cla_manager::ClaManager;
use dtnpm_core::config::{HamConfig, WebConfig};
use dtnpm_core::internal_storage::InternalStorage;
use dtnpm_core::log_buffer::LogBuffer;
use dtnpm_core::routing::RouteManager;
use dtnpm_core::security::DirectionalKeyStore;
use dtnpm_core::{BpaClient, GrpcBpaClient, ServiceTracker};
use dtnpm_mod_senml::SenmlModule;
use dtnpm_web_core::{
    ModuleContext, WebBundleInjector, WebModule, register_modules, start_web_server,
};
use hardy_bpa::bpa::BpaRegistration;
use hardy_bpa::cla::ClaInit;

#[derive(Debug, Parser)]
#[command(
    name = "dtnpm-mod-senml",
    about = "Standalone DTN SenML Telemetry WebModule daemon"
)]
struct Args {
    /// Hardy BPA gRPC endpoint
    #[arg(long, default_value = "http://127.0.0.1:50051")]
    bpa_grpc: String,

    /// Web HTTP listen address
    #[arg(long, default_value = "127.0.0.1:8083")]
    listen: SocketAddr,

    /// Local DTN node EID
    #[arg(long, default_value = "dtn://local/")]
    node_id: String,

    /// Optional radio callsign
    #[arg(long)]
    callsign: Option<String>,

    /// Optional SQLite data directory (uses in-memory DB if omitted)
    #[arg(long)]
    data_dir: Option<PathBuf>,

    /// Optional Bearer authentication token for protected gRPC BPA endpoints
    #[arg(long, env = "DTN_HDY_TOKEN")]
    token: Option<String>,

    /// Optional Unix Domain Socket path for connecting to a protected local BPA
    #[arg(long, env = "DTN_HDY_SOCKET")]
    socket: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    let mut config = HamConfig::default();
    config.node_id = args.node_id;
    config.callsign = args.callsign;
    config.web = Some(WebConfig {
        address: args.listen,
        static_dir: dtnpm_core::config::default_web_static_dir(),
    });
    if let Some(dir) = args.data_dir {
        config.storage.in_memory = false;
        config.storage.data_dir = Some(dir);
    } else {
        config.storage.in_memory = true;
    }
    config.normalize();

    let internal_storage = if config.storage.in_memory {
        InternalStorage::new_in_memory_with_quota(config.application.quota.max_storage)?
    } else {
        InternalStorage::new_disk_with_quota(
            config
                .storage
                .data_dir
                .as_deref()
                .unwrap_or_else(|| std::path::Path::new(".")),
            config.application.quota.max_storage,
        )?
    };

    let grpc_bpa = Arc::new(GrpcBpaClient::with_auth(
        args.bpa_grpc,
        args.token.as_deref(),
        args.socket.as_deref(),
    ));
    let bpa_reg: Arc<dyn BpaRegistration> = grpc_bpa.clone().as_registration();
    let bpa: Arc<dyn BpaClient> = grpc_bpa;
    let service_tracker = ServiceTracker::new(bpa_reg.clone());
    let injector = WebBundleInjector::new();
    let _ = bpa_reg
        .register_cla(
            "web-injector-senml".to_string(),
            Arc::new(injector.clone()),
            None,
            ClaInit::default(),
        )
        .await;

    let modules: Vec<Arc<dyn WebModule>> = vec![Arc::new(SenmlModule::new())];
    let ctx = ModuleContext::new(
        Arc::new(config),
        bpa,
        injector,
        service_tracker,
        internal_storage,
        ClaManager::new(),
        Arc::new(RouteManager::new(Vec::new(), None)),
        LogBuffer::new(250),
        Arc::new(DirectionalKeyStore::empty()),
    )
    .with_capabilities(&modules);

    register_modules(&ctx, &modules).await?;
    let _web_handle = start_web_server(ctx, modules, args.listen).await?;

    tokio::signal::ctrl_c().await?;
    Ok(())
}
