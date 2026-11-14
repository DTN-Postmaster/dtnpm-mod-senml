//! SenML IoT telemetry gateway, normalization, storage, Open MCT integration,
//! and [`dtnpm_web_core::WebModule`] implementation.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use axum::Router;
use axum::routing::{get, put};
use hardy_async::async_trait;
use hardy_bpv7::eid::Service;
use rusqlite::Connection;

use dtnpm_web_core::{ModuleContext, WebModule};

pub mod manager;
pub mod parser;
pub mod routes;
pub mod service;
pub mod storage;

pub use manager::SenmlManager;
pub use parser::{NormalizedSenmlRecord, parse_senml_payload};
pub use service::SenmlService;
pub use storage::{HistoryPoint, LatestMetricEntry, MetricEntry, SenmlStorage, TelemetryPoint};

/// SQL schema applied on the shared SQLite connection when registering [`SenmlModule`].
pub const SENML_SCHEMA_SQL: &str = include_str!("../migrations/0001_senml_schema.sql");

/// Handler state shared across SenML HTTP/SSE routes.
#[derive(Clone)]
pub struct SenmlState {
    pub ctx: ModuleContext,
    pub senml_manager: SenmlManager,
}

/// Modular DTN SenML telemetry plugin exposing `/api/senml/v1/...` routes
/// and registering `dtn://<node>/senml`.
#[derive(Clone, Default)]
pub struct SenmlModule {
    manager: Arc<OnceLock<SenmlManager>>,
    config_path: Option<PathBuf>,
}

impl SenmlModule {
    /// Creates a new [`SenmlModule`] with no external YAML config sync path.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new [`SenmlModule`] configured to persist dashboard metric layout back
    /// into the given YAML configuration file when modified from the WebUI.
    #[must_use]
    pub fn with_config_path(config_path: Option<PathBuf>) -> Self {
        Self {
            manager: Arc::new(OnceLock::new()),
            config_path,
        }
    }

    /// Returns the initialized [`SenmlManager`] if registration or route setup has run.
    #[must_use]
    pub fn manager(&self) -> Option<SenmlManager> {
        self.manager.get().cloned()
    }

    fn get_or_init_manager(&self, ctx: &ModuleContext) -> anyhow::Result<SenmlManager> {
        if let Some(existing) = self.manager.get() {
            return Ok(existing.clone());
        }
        let storage = SenmlStorage::from_connection(
            ctx.internal_storage.connection(),
            self.config_path.clone(),
        )?;
        if let Some(ref senml_cfg) = ctx.config.senml {
            storage.sync_initial_config(&senml_cfg.metrics)?;
        }
        let manager = SenmlManager::new(storage);
        let _ = self.manager.set(manager.clone());
        Ok(self.manager.get().cloned().unwrap_or(manager))
    }
}

#[async_trait]
impl WebModule for SenmlModule {
    fn id(&self) -> &'static str {
        "senml"
    }

    fn name(&self) -> &'static str {
        "SenML Telemetry"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn services(&self) -> Vec<String> {
        vec!["senml".to_string()]
    }

    fn migrate(&self, conn: &Connection) -> anyhow::Result<()> {
        conn.execute_batch(SENML_SCHEMA_SQL)?;
        Ok(())
    }

    async fn register_bpa(&self, ctx: &ModuleContext) -> anyhow::Result<()> {
        let manager = self.get_or_init_manager(ctx)?;
        let senml_service = Arc::new(SenmlService::new(
            manager,
            ctx.service_tracker.clone(),
            ctx.keystore.clone(),
        ));
        ctx.service_tracker
            .register_service_with_type(
                Service::Dtn("senml".into()),
                senml_service,
                "Built-in SenML",
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to register SenML service: {e}"))?;
        Ok(())
    }

    fn api_routes(&self, ctx: &ModuleContext) -> Router<()> {
        let senml_manager = self.get_or_init_manager(ctx).unwrap_or_else(|e| {
            tracing::warn!("SenmlModule: falling back to in-memory storage after init error: {e}");
            SenmlManager::new(
                SenmlStorage::new(None, None).expect("in-memory SenmlStorage must succeed"),
            )
        });
        let state = SenmlState {
            ctx: ctx.clone(),
            senml_manager,
        };
        Router::new()
            .route("/metrics", get(routes::get_metrics))
            .route("/metrics/latest", get(routes::get_latest_metrics))
            .route("/history", get(routes::get_history))
            .route("/stream", get(routes::get_stream))
            .route(
                "/dashboard/cards",
                get(routes::get_dashboard_cards)
                    .post(routes::add_dashboard_card)
                    .delete(routes::clear_dashboard_cards),
            )
            .route(
                "/dashboard/cards/{id}",
                put(routes::update_dashboard_card).delete(routes::remove_dashboard_card),
            )
            .with_state(state)
    }
}
