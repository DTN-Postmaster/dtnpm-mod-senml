//! Permanent built-in DTN BPv7 service handling incoming `dtn://<station>/senml` bundles.

use bytes::Bytes;
use hardy_async::async_trait;
use hardy_bpa::services::{Result as ServiceResult, Service, ServiceSink, StatusNotify};
use hardy_bpa::stream::{Receiver, Segment, buffer_stream};
use hardy_bpv7::bundle::Id;
use hardy_bpv7::eid::Eid;
use hardy_bpv7::parse::parse;
use hardy_bpv7::status_report::ReasonCode;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::OnceCell;
use tracing::{debug, info, warn};

use super::manager::SenmlManager;
use super::parser::{MAX_SENML_PAYLOAD_BYTES, parse_senml_payload};
use dtnpm_core::security::{AuthStatus, DirectionalKeyStore, VerifyPolicy, verify_parsed_bundle};
use dtnpm_core::service_tracker::ServiceTracker;

/// Generous upper bound on BPv7 bundle envelope overhead around a 1 MiB SenML payload.
const MAX_SENML_BUNDLE_BYTES: u64 = (MAX_SENML_PAYLOAD_BYTES as u64) + 64 * 1024;

/// Permanent DTN BPv7 service for receiving, authenticating and ingesting SenML sensor telemetry.
#[derive(Clone)]
pub struct SenmlService {
    manager: SenmlManager,
    _service_tracker: ServiceTracker,
    keystore: Arc<DirectionalKeyStore>,
    sink: Arc<OnceCell<Box<dyn ServiceSink>>>,
}

impl SenmlService {
    /// Creates a new `SenmlService` bound to manager, service tracker and keystore.
    pub fn new(
        manager: SenmlManager,
        service_tracker: ServiceTracker,
        keystore: Arc<DirectionalKeyStore>,
    ) -> Self {
        Self {
            manager,
            _service_tracker: service_tracker,
            keystore,
            sink: Arc::new(OnceCell::new()),
        }
    }

    async fn handle_delivered_bundle(&self, data: Bytes) -> ServiceResult<()> {
        let Ok(parsed) = parse(data) else {
            debug!("SenmlService: failed to parse delivered bundle");
            return Ok(());
        };

        // BPSec verification
        let auth_status = verify_parsed_bundle(&parsed, &self.keystore);
        let policy = self.keystore.get_policy();
        if policy == VerifyPolicy::Strict && auth_status != AuthStatus::Valid {
            warn!(
                auth_status = ?auth_status,
                "SenmlService: dropping delivered bundle due to strict policy without valid BPSec signature"
            );
            return Ok(());
        }

        let bundle = &parsed.bundle;
        let source_eid = &bundle.primary.id.source;

        if source_eid.is_null() || bundle.primary.flags.is_admin_record {
            debug!("SenmlService: ignoring null source or administrative bundle");
            return Ok(());
        }

        // Extract payload block (#1)
        let Some(payload_slice) = bundle
            .blocks
            .get(&1)
            .and_then(|block| block.payload(&parsed.data))
        else {
            debug!("SenmlService: delivered bundle has no payload block");
            return Ok(());
        };

        let creation_time_ms = bundle
            .primary
            .id
            .timestamp
            .as_datetime()
            .unwrap_or_else(OffsetDateTime::now_utc)
            .unix_timestamp_nanos()
            / 1_000_000;

        match parse_senml_payload(payload_slice, creation_time_ms as i64) {
            Ok(records) => {
                if records.is_empty() {
                    debug!("SenmlService: parsed empty SenML records list");
                    return Ok(());
                }

                info!(
                    "SenmlService: ingesting {} SenML records from {}",
                    records.len(),
                    source_eid
                );

                match self.manager.storage().insert_records(&records) {
                    Ok(points) => {
                        self.manager.broadcast_points(points);
                    }
                    Err(e) => {
                        warn!("SenmlService: failed to insert SenML records into SQLite: {e}");
                    }
                }
            }
            Err(e) => {
                debug!("SenmlService: failed to parse payload as SenML: {e}");
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Service for SenmlService {
    async fn on_register(&self, _source: &Eid, sink: Box<dyn ServiceSink>) {
        let _ = self.sink.set(sink);
        debug!("SenmlService registered with BPA");
    }

    async fn on_unregister(&self) {
        debug!("SenmlService unregistered from BPA");
    }

    async fn on_status_notify(
        &self,
        _bundle_id: &Id,
        _from: &Eid,
        _kind: StatusNotify,
        _reason: ReasonCode,
        _timestamp: Option<OffsetDateTime>,
    ) {
    }

    async fn on_deliver(
        &self,
        bundle_id: &Id,
        _expiry: OffsetDateTime,
        total_len: u64,
        stream: &mut dyn Receiver<Segment>,
    ) -> ServiceResult<()> {
        debug!("SenmlService on_deliver called for bundle_id={bundle_id:?}, len={total_len}");
        if total_len > MAX_SENML_BUNDLE_BYTES {
            warn!(
                bundle_id = ?bundle_id,
                total_len,
                max_bundle_bytes = MAX_SENML_BUNDLE_BYTES,
                "SenmlService: dropping incoming bundle before buffering because total_len exceeds SenML limit"
            );
            return Ok(());
        }
        let data = buffer_stream(stream, total_len).await?;
        self.handle_delivered_bundle(data).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SenmlStorage;
    use dtnpm_core::MockBpa;
    use dtnpm_core::security::{Direction, KeyEntry, sign_bundle_with_key};
    use hardy_bpa::stream::RecvError;
    use hardy_bpv7::builder::Builder;
    use hardy_bpv7::creation_timestamp::CreationTimestamp;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    fn make_senml_service(keystore: Arc<DirectionalKeyStore>) -> (SenmlService, SenmlManager) {
        let storage = SenmlStorage::new(None, None).unwrap();
        let manager = SenmlManager::new(storage);
        let bpa = Arc::new(MockBpa::new("dtn://stata/"));
        let tracker = ServiceTracker::new(bpa);
        (
            SenmlService::new(manager.clone(), tracker, keystore),
            manager,
        )
    }

    fn build_senml_bundle(src: &str, dst: &str, payload: &[u8]) -> Vec<u8> {
        let src_eid: Eid = src.parse().unwrap();
        let dst_eid: Eid = dst.parse().unwrap();
        let (_bundle, raw) = Builder::new(src_eid, dst_eid)
            .with_lifetime(Duration::from_secs(3600))
            .with_payload(payload.to_vec().into())
            .build(CreationTimestamp::now())
            .unwrap();
        raw.into_vec()
    }

    #[tokio::test]
    async fn strict_policy_rejects_unsigned_missing_key_and_invalid_accepts_valid() {
        let keystore = Arc::new(DirectionalKeyStore::empty());
        keystore.set_policy(VerifyPolicy::Strict).unwrap();
        let valid_secret = b"senml_inbound_secret_key_32_byte".to_vec();
        keystore
            .add_key(
                Direction::Inbound,
                "dtn://f4jxq/*".to_string(),
                valid_secret.clone(),
                None,
            )
            .unwrap();

        let (service, manager) = make_senml_service(keystore);
        let payload = br#"[{"bn":"urn:dtn:s:","n":"temp","v":21.5}]"#;

        // 1. Unsigned bundle must be dropped
        let unsigned = build_senml_bundle("dtn://f4jxq/senml", "dtn://stata/senml", payload);
        service
            .handle_delivered_bundle(Bytes::from(unsigned.clone()))
            .await
            .unwrap();
        assert!(manager.storage().list_metrics().unwrap().is_empty());

        // 2. MissingKey bundle must be dropped
        let unknown_raw = build_senml_bundle("dtn://unknown/senml", "dtn://stata/senml", payload);
        let unknown_parsed = parse(Bytes::from(unknown_raw.clone())).unwrap();
        let unknown_key = KeyEntry {
            id: "u1".into(),
            direction: Direction::Outbound,
            pattern: "dtn://stata/*".into(),
            label: None,
            secret: b"other_secret_key_32_bytes_123456".to_vec(),
            created_at: "now".into(),
        };
        let missing_signed =
            sign_bundle_with_key(&unknown_parsed.bundle, &unknown_raw, &unknown_key, None).unwrap();
        service
            .handle_delivered_bundle(Bytes::from(missing_signed))
            .await
            .unwrap();
        assert!(manager.storage().list_metrics().unwrap().is_empty());

        // 3. Invalid signature must be dropped
        let parsed = parse(Bytes::from(unsigned.clone())).unwrap();
        let wrong_key = KeyEntry {
            id: "bad".into(),
            direction: Direction::Outbound,
            pattern: "dtn://stata/*".into(),
            label: None,
            secret: b"wrong_secret_key_32_bytes_123456".to_vec(),
            created_at: "now".into(),
        };
        let invalid_signed =
            sign_bundle_with_key(&parsed.bundle, &unsigned, &wrong_key, None).unwrap();
        service
            .handle_delivered_bundle(Bytes::from(invalid_signed))
            .await
            .unwrap();
        assert!(manager.storage().list_metrics().unwrap().is_empty());

        // 4. Valid signature must be accepted
        let good_key = KeyEntry {
            id: "good".into(),
            direction: Direction::Outbound,
            pattern: "dtn://stata/*".into(),
            label: None,
            secret: valid_secret,
            created_at: "now".into(),
        };
        let valid_signed =
            sign_bundle_with_key(&parsed.bundle, &unsigned, &good_key, None).unwrap();
        service
            .handle_delivered_bundle(Bytes::from(valid_signed))
            .await
            .unwrap();
        assert_eq!(manager.storage().list_metrics().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn warn_and_ignore_policies_accept_unsigned_bundles() {
        let payload = br#"[{"bn":"urn:dtn:s:","n":"volt","v":13.8}]"#;
        for policy in [VerifyPolicy::Warn, VerifyPolicy::Ignore] {
            let keystore = Arc::new(DirectionalKeyStore::empty());
            keystore.set_policy(policy).unwrap();
            let (service, manager) = make_senml_service(keystore);
            let unsigned = build_senml_bundle("dtn://f4jxq/senml", "dtn://stata/senml", payload);
            service
                .handle_delivered_bundle(Bytes::from(unsigned))
                .await
                .unwrap();
            assert_eq!(
                manager.storage().list_metrics().unwrap().len(),
                1,
                "policy {policy:?} should accept unsigned"
            );
        }
    }

    struct TrackingRejectStream {
        recv_called: Arc<AtomicBool>,
    }

    #[async_trait]
    impl Receiver<Segment> for TrackingRejectStream {
        async fn recv(&mut self) -> core::result::Result<Segment, RecvError> {
            self.recv_called.store(true, Ordering::SeqCst);
            Err(RecvError)
        }
    }

    #[tokio::test]
    async fn on_deliver_rejects_oversized_bundle_before_buffering_stream() {
        let keystore = Arc::new(DirectionalKeyStore::empty());
        let (service, _manager) = make_senml_service(keystore);
        let recv_called = Arc::new(AtomicBool::new(false));
        let mut stream = TrackingRejectStream {
            recv_called: recv_called.clone(),
        };
        let raw = build_senml_bundle("dtn://f4jxq/senml", "dtn://stata/senml", b"[]");
        let parsed = parse(Bytes::from(raw)).unwrap();

        service
            .on_deliver(
                &parsed.bundle.primary.id,
                OffsetDateTime::now_utc(),
                MAX_SENML_BUNDLE_BYTES + 1,
                &mut stream,
            )
            .await
            .unwrap();

        assert!(
            !recv_called.load(Ordering::SeqCst),
            "buffer_stream must not pull segments when total_len exceeds SenML limit"
        );
    }
}
