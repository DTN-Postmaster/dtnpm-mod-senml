use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use dtnpm_core::config::{DashboardMetricConfig, HamConfig, SenmlConfig};
use dtnpm_mod_senml::SenmlModule;
use dtnpm_web_core::TestServerBuilder;
use hardy_bpv7::builder::Builder;
use hardy_bpv7::creation_timestamp::CreationTimestamp;
use hardy_bpv7::eid::Eid;
use serde_json::Value;

#[tokio::test]
#[allow(clippy::field_reassign_with_default)]
async fn test_senml_service_and_api_endpoints() {
    let mut config = HamConfig::default();
    config.callsign = Some("F5TEST".to_string());
    config.node_id = "dtn://f5test/".to_string();
    config.storage.in_memory = true;
    config.senml = Some(SenmlConfig {
        metrics: vec![
            DashboardMetricConfig {
                name: "urn:dtn:sensor:bus_v".to_string(),
                label: Some("Tension Bus".to_string()),
            },
            DashboardMetricConfig {
                name: "urn:dtn:sensor:temp_cpu".to_string(),
                label: Some("Température CPU".to_string()),
            },
        ],
    });

    let srv = TestServerBuilder::new()
        .with_config(config)
        .with_module(Arc::new(SenmlModule::new()))
        .build()
        .await
        .unwrap();

    // 0. Capabilities should report senml module
    let caps_res = srv.get("/api/capabilities").await;
    assert_eq!(caps_res.status(), StatusCode::OK);
    let caps_body = axum::body::to_bytes(caps_res.into_body(), usize::MAX)
        .await
        .unwrap();
    let caps: Value = serde_json::from_slice(&caps_body).unwrap();
    assert_eq!(caps["modules"][0]["id"], "senml");

    // 1. GET /api/senml/v1/metrics - verify initial 2 metrics from YAML
    let res = srv.get("/api/senml/v1/metrics").await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let metrics: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(metrics.as_array().unwrap().len(), 2);
    assert_eq!(metrics[0]["name"], "urn:dtn:sensor:bus_v");
    assert_eq!(metrics[0]["custom_label"], "Tension Bus");
    assert_eq!(metrics[0]["is_visible"], true);

    // 2. Inject a SenML JSON bundle destined to dtn://f5test/senml
    let senml_payload = r#"[
        {"bn": "urn:dtn:sensor:", "bt": 1731000100, "bu": "V", "n": "bus_v", "v": 28.1},
        {"n": "temp_cpu", "u": "Cel", "v": 45.2},
        {"n": "new_unconfigured_metric", "u": "mA", "v": 320.0}
    ]"#;

    let source_eid: Eid = "dtn://remote_station/telemetry".parse().unwrap();
    let dest_eid: Eid = "dtn://f5test/senml".parse().unwrap();
    let (_bundle, binbundle) = Builder::new(source_eid, dest_eid)
        .with_payload(senml_payload.as_bytes().into())
        .with_lifetime(Duration::from_secs(3600))
        .build(CreationTimestamp::now())
        .unwrap();

    srv.ctx
        .injector
        .inject(bytes::Bytes::from(binbundle.into_vec()))
        .await
        .unwrap();

    // Allow async dispatch into SenmlService
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. GET /api/senml/v1/metrics/latest?dashboard_only=true - should only return the 2 configured cards
    let res = srv
        .get("/api/senml/v1/metrics/latest?dashboard_only=true")
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let latest_dash: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(latest_dash.as_array().unwrap().len(), 2);
    assert_eq!(latest_dash[0]["name"], "urn:dtn:sensor:bus_v");
    assert_eq!(latest_dash[0]["value"], 28.1);
    assert_eq!(latest_dash[0]["unit"], "V");
    assert_eq!(latest_dash[0]["utc"], 1731000100000i64);

    // 4. GET /api/senml/v1/metrics/latest?id=urn:dtn:sensor:temp_cpu - filter test
    let res = srv
        .get("/api/senml/v1/metrics/latest?id=urn:dtn:sensor:temp_cpu")
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let filtered: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(filtered.as_array().unwrap().len(), 1);
    assert_eq!(filtered[0]["name"], "urn:dtn:sensor:temp_cpu");
    assert_eq!(filtered[0]["value"], 45.2);

    // 5. GET /api/senml/v1/history?id=urn:dtn:sensor:bus_v&start=0&end=2000000000000 - Open MCT history test
    let res = srv
        .get("/api/senml/v1/history?id=urn:dtn:sensor:bus_v&start=0&end=2000000000000")
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let history: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(history[0]["utc"], 1731000100000i64);
    assert_eq!(history[0]["value"], 28.1);

    // 6. POST /api/senml/v1/dashboard/cards - Add the discovered metric to dashboard
    let res = srv
        .post_json(
            "/api/senml/v1/dashboard/cards",
            serde_json::json!({
                "name": "urn:dtn:sensor:new_unconfigured_metric",
                "label": "Consommation Courant"
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    // Verify dashboard now has 3 cards
    let res = srv
        .get("/api/senml/v1/metrics/latest?dashboard_only=true")
        .await;
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let latest_dash3: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(latest_dash3.as_array().unwrap().len(), 3);
}
