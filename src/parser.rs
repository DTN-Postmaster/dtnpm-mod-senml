//! RFC 8428 SenML parsing and normalization for JSON and CBOR payloads.

use serde::{Deserialize, Serialize};

/// Maximum accepted raw SenML payload size (1 MiB).
pub const MAX_SENML_PAYLOAD_BYTES: usize = 1024 * 1024;
/// Maximum accepted number of SenML records in a single pack.
pub const MAX_SENML_RECORDS: usize = 4096;
/// RFC 8428 default SenML version.
const SENML_VERSION: i128 = 10;

/// Normalized SenML record representing a single measurement point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedSenmlRecord {
    /// Full resolved metric name `(bn || "") + (n || "")`.
    pub name: String,
    /// Numeric measurement value (if present).
    pub value: Option<f64>,
    /// Textual / boolean measurement representation (if present, or formatted numeric).
    pub str_value: Option<String>,
    /// Engineering unit `u || bu || ""`.
    pub unit: Option<String>,
    /// Absolute POSIX timestamp in milliseconds.
    pub timestamp_ms: i64,
}

/// Parses a raw payload into normalized SenML records.
/// Automatically detects JSON (`application/senml+json`) or CBOR (`application/senml+cbor`).
/// Relative times ($t \le 2^{28}$) are anchored to `fallback_timestamp_ms` (bundle creation time).
pub fn parse_senml_payload(
    data: &[u8],
    fallback_timestamp_ms: i64,
) -> anyhow::Result<Vec<NormalizedSenmlRecord>> {
    if data.len() > MAX_SENML_PAYLOAD_BYTES {
        anyhow::bail!(
            "SenML payload size {} bytes exceeds 1 MiB limit ({} bytes)",
            data.len(),
            MAX_SENML_PAYLOAD_BYTES
        );
    }

    let trimmed = match std::str::from_utf8(data) {
        Ok(s) => s.trim_start(),
        Err(_) => "",
    };

    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        let records = parse_json_senml(data, fallback_timestamp_ms)?;
        if records.is_empty() {
            anyhow::bail!("SenML JSON pack contains no valid records");
        }
        return Ok(records);
    }

    // Try CBOR decoding
    if let Ok(records) = parse_cbor_senml(data, fallback_timestamp_ms) {
        if records.is_empty() {
            anyhow::bail!("SenML CBOR pack contains no valid records");
        }
        return Ok(records);
    }

    // Fallback: try JSON if not tried yet
    if !trimmed.is_empty() {
        let records = parse_json_senml(data, fallback_timestamp_ms)?;
        if !records.is_empty() {
            return Ok(records);
        }
    }

    anyhow::bail!("Payload could not be parsed as RFC 8428 SenML JSON or CBOR")
}

fn calculate_time_ms(time_sec: f64, fallback_timestamp_ms: i64) -> i64 {
    // RFC 8428 Section 4.5.3: Values > 2**28 (~1978) represent absolute seconds since 1970 UTC.
    // Values <= 2**28 represent relative seconds from base time / current time.
    if !time_sec.is_finite() {
        return fallback_timestamp_ms;
    }
    let delta = (time_sec * 1000.0).round();
    if !delta.is_finite() || delta > (i64::MAX as f64) || delta < (i64::MIN as f64) {
        return fallback_timestamp_ms;
    }
    let delta_ms = delta as i64;
    if time_sec > 268_435_456.0 {
        delta_ms
    } else {
        fallback_timestamp_ms.saturating_add(delta_ms)
    }
}

fn json_finite_f64(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> anyhow::Result<Option<f64>> {
    let Some(val) = map.get(key) else {
        return Ok(None);
    };
    let f = val
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("SenML JSON field '{key}' must be a finite number"))?;
    if !f.is_finite() {
        anyhow::bail!("SenML JSON field '{key}' must be finite");
    }
    Ok(Some(f))
}

fn parse_json_senml(
    data: &[u8],
    fallback_timestamp_ms: i64,
) -> anyhow::Result<Vec<NormalizedSenmlRecord>> {
    let root: serde_json::Value = serde_json::from_slice(data)?;
    let array = match root {
        serde_json::Value::Array(arr) => arr,
        serde_json::Value::Object(_) => vec![root],
        _ => anyhow::bail!("Expected JSON array or object for SenML"),
    };

    if array.len() > MAX_SENML_RECORDS {
        anyhow::bail!(
            "SenML pack has {} records, exceeding limit of {}",
            array.len(),
            MAX_SENML_RECORDS
        );
    }

    let mut records = Vec::with_capacity(array.len());
    let mut base_name = String::new();
    let mut base_time = 0.0f64;
    let mut base_unit: Option<String> = None;
    let mut base_value = 0.0f64;
    let mut base_sum = 0.0f64;

    for item in array {
        let Some(map) = item.as_object() else {
            anyhow::bail!("Each SenML JSON record must be an object");
        };

        if let Some(bver_val) = map.get("bver") {
            let bver = bver_val
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("SenML 'bver' must be integer 10"))?;
            if i128::from(bver) != SENML_VERSION {
                anyhow::bail!("Unsupported SenML version bver={bver}; expected {SENML_VERSION}");
            }
        }

        if let Some(bn) = map.get("bn").and_then(|v| v.as_str()) {
            base_name = bn.to_string();
        }
        if let Some(bt) = json_finite_f64(map, "bt")? {
            base_time = bt;
        }
        if let Some(bu) = map.get("bu").and_then(|v| v.as_str()) {
            base_unit = Some(bu.to_string());
        }
        if let Some(bv) = json_finite_f64(map, "bv")? {
            base_value = bv;
        }
        if let Some(bs) = json_finite_f64(map, "bs")? {
            base_sum = bs;
        }

        let v_opt = json_finite_f64(map, "v")?;
        let s_opt = json_finite_f64(map, "s")?;
        let vs_opt = map.get("vs").and_then(|v| v.as_str());
        let vb_opt = map.get("vb").and_then(|v| v.as_bool());
        let vd_opt = map.get("vd").and_then(|v| v.as_str());

        let (value, str_value) = if let Some(v) = v_opt {
            let total_v = v + base_value;
            if !total_v.is_finite() {
                anyhow::bail!("Non-finite SenML resolved value");
            }
            (Some(total_v), Some(format_float(total_v)))
        } else if let Some(vs) = vs_opt {
            (None, Some(vs.to_string()))
        } else if let Some(vb) = vb_opt {
            (Some(if vb { 1.0 } else { 0.0 }), Some(vb.to_string()))
        } else if let Some(vd) = vd_opt {
            (None, Some(vd.to_string()))
        } else if let Some(s) = s_opt {
            let total_s = s + base_sum;
            if !total_s.is_finite() {
                anyhow::bail!("Non-finite SenML resolved sum");
            }
            (Some(total_s), Some(format_float(total_s)))
        } else {
            anyhow::bail!("SenML record missing required value field (v, vs, vb, vd, or s)");
        };

        let n = map.get("n").and_then(|v| v.as_str());
        let full_name = format!("{}{}", base_name, n.unwrap_or_default());
        if full_name.is_empty() {
            anyhow::bail!("SenML record resolved to an empty metric name");
        }

        let u = map
            .get("u")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| base_unit.clone());

        let t = json_finite_f64(map, "t")?.unwrap_or(0.0);
        let abs_t = base_time + t;
        if !abs_t.is_finite() {
            anyhow::bail!("Non-finite SenML timestamp");
        }
        let timestamp_ms = calculate_time_ms(abs_t, fallback_timestamp_ms);

        records.push(NormalizedSenmlRecord {
            name: full_name,
            value,
            str_value,
            unit: u,
            timestamp_ms,
        });
    }

    Ok(records)
}

fn format_float(val: f64) -> String {
    if (val.fract() == 0.0) && (val.abs() < 1e15) {
        format!("{}", val as i64)
    } else {
        format!("{val}")
    }
}

fn cbor_key_matches(key: &ciborium::value::Value, str_key: &str, int_key: i128) -> bool {
    match key {
        ciborium::value::Value::Text(s) => s == str_key,
        ciborium::value::Value::Integer(i) => i128::from(*i) == int_key,
        _ => false,
    }
}

fn parse_cbor_senml(
    data: &[u8],
    fallback_timestamp_ms: i64,
) -> anyhow::Result<Vec<NormalizedSenmlRecord>> {
    let cbor_val: ciborium::value::Value = ciborium::from_reader(data)?;
    let items = match cbor_val {
        ciborium::value::Value::Array(arr) => arr,
        ciborium::value::Value::Map(_) => vec![cbor_val],
        _ => anyhow::bail!("Expected CBOR array or map for SenML"),
    };

    if items.len() > MAX_SENML_RECORDS {
        anyhow::bail!(
            "SenML CBOR pack has {} records, exceeding limit of {}",
            items.len(),
            MAX_SENML_RECORDS
        );
    }

    let mut records = Vec::with_capacity(items.len());
    let mut base_name = String::new();
    let mut base_time = 0.0f64;
    let mut base_unit: Option<String> = None;
    let mut base_value = 0.0f64;
    let mut base_sum = 0.0f64;

    for item in items {
        let ciborium::value::Value::Map(entries) = item else {
            anyhow::bail!("Each SenML CBOR record must be a map");
        };

        // Reject any non-finite floats anywhere in the CBOR record map.
        for (k, v) in &entries {
            if let ciborium::value::Value::Float(f) = v
                && !f.is_finite()
            {
                anyhow::bail!("Non-finite float in CBOR SenML record");
            }
            if cbor_key_matches(k, "bver", -1) {
                match v {
                    ciborium::value::Value::Integer(i) if i128::from(*i) == SENML_VERSION => {}
                    _ => anyhow::bail!("Unsupported SenML CBOR version; expected {SENML_VERSION}"),
                }
            }
        }

        let get_str = |str_key: &str, int_key: i128| -> Option<String> {
            for (k, v) in &entries {
                if cbor_key_matches(k, str_key, int_key) {
                    match v {
                        ciborium::value::Value::Text(s) => return Some(s.clone()),
                        ciborium::value::Value::Bytes(b) => {
                            return String::from_utf8(b.clone()).ok();
                        }
                        _ => return None,
                    }
                }
            }
            None
        };

        let get_f64 = |str_key: &str, int_key: i128| -> anyhow::Result<Option<f64>> {
            for (k, v) in &entries {
                if cbor_key_matches(k, str_key, int_key) {
                    match v {
                        ciborium::value::Value::Float(f) => {
                            if !f.is_finite() {
                                anyhow::bail!("Non-finite CBOR float for '{str_key}'");
                            }
                            return Ok(Some(*f));
                        }
                        ciborium::value::Value::Integer(i) => {
                            let val = i128::from(*i) as f64;
                            if !val.is_finite() {
                                anyhow::bail!("Non-finite CBOR integer conversion for '{str_key}'");
                            }
                            return Ok(Some(val));
                        }
                        _ => anyhow::bail!("Invalid non-numeric CBOR value for '{str_key}'"),
                    }
                }
            }
            Ok(None)
        };

        let get_bool = |str_key: &str, int_key: i128| -> Option<bool> {
            for (k, v) in &entries {
                if cbor_key_matches(k, str_key, int_key)
                    && let ciborium::value::Value::Bool(b) = v
                {
                    return Some(*b);
                }
            }
            None
        };

        // RFC 8428 Section 6 integer labels:
        // bver: -1, bn: -2, bt: -3, bu: -4, bv: -5, bs: -6
        if let Some(bn) = get_str("bn", -2) {
            base_name = bn;
        }
        if let Some(bt) = get_f64("bt", -3)? {
            base_time = bt;
        }
        if let Some(bu) = get_str("bu", -4) {
            base_unit = Some(bu);
        }
        if let Some(bv) = get_f64("bv", -5)? {
            base_value = bv;
        }
        if let Some(bs) = get_f64("bs", -6)? {
            base_sum = bs;
        }

        // n: 0, u: 1, v: 2, vs: 3, vb: 4, s: 5, t: 6, vd: 8
        let v_opt = get_f64("v", 2)?;
        let vs_opt = get_str("vs", 3);
        let vb_opt = get_bool("vb", 4);
        let vd_opt = get_str("vd", 8);
        let s_opt = get_f64("s", 5)?;

        let (value, str_value) = if let Some(v) = v_opt {
            let total = v + base_value;
            if !total.is_finite() {
                anyhow::bail!("Non-finite CBOR SenML resolved value");
            }
            (Some(total), Some(format_float(total)))
        } else if let Some(vs) = vs_opt {
            (None, Some(vs))
        } else if let Some(vb) = vb_opt {
            (Some(if vb { 1.0 } else { 0.0 }), Some(vb.to_string()))
        } else if let Some(vd) = vd_opt {
            (None, Some(vd))
        } else if let Some(s) = s_opt {
            let total_s = s + base_sum;
            if !total_s.is_finite() {
                anyhow::bail!("Non-finite CBOR SenML resolved sum");
            }
            (Some(total_s), Some(format_float(total_s)))
        } else {
            anyhow::bail!("SenML CBOR record missing required value field (v, vs, vb, vd, or s)");
        };

        let n = get_str("n", 0);
        let full_name = format!("{}{}", base_name, n.as_deref().unwrap_or_default());
        if full_name.is_empty() {
            anyhow::bail!("SenML CBOR record resolved to an empty metric name");
        }

        let u = get_str("u", 1).or_else(|| base_unit.clone());
        let t = get_f64("t", 6)?.unwrap_or(0.0);
        let abs_t = base_time + t;
        if !abs_t.is_finite() {
            anyhow::bail!("Non-finite CBOR SenML timestamp");
        }
        let timestamp_ms = calculate_time_ms(abs_t, fallback_timestamp_ms);

        records.push(NormalizedSenmlRecord {
            name: full_name,
            value,
            str_value,
            unit: u,
            timestamp_ms,
        });
    }

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_senml_pack() {
        let json_data = r#"[
            {"bver": 10, "bn": "urn:dtn:node1:", "bt": 1731000000, "bu": "V", "n": "bus_voltage", "v": 27.9},
            {"n": "temp/cpu", "u": "Cel", "v": 42.5},
            {"n": "status/online", "vb": true},
            {"n": "state/mode", "vs": "STANDBY"},
            {"bs": 100.0, "n": "energy_total", "s": 25.5}
        ]"#;

        let records = parse_senml_payload(json_data.as_bytes(), 1700000000000).unwrap();
        assert_eq!(records.len(), 5);

        assert_eq!(records[0].name, "urn:dtn:node1:bus_voltage");
        assert_eq!(records[0].value, Some(27.9));
        assert_eq!(records[0].unit.as_deref(), Some("V"));
        assert_eq!(records[0].timestamp_ms, 1731000000000);

        assert_eq!(records[1].name, "urn:dtn:node1:temp/cpu");
        assert_eq!(records[1].value, Some(42.5));
        assert_eq!(records[1].unit.as_deref(), Some("Cel"));
        assert_eq!(records[1].timestamp_ms, 1731000000000);

        assert_eq!(records[2].name, "urn:dtn:node1:status/online");
        assert_eq!(records[2].value, Some(1.0));
        assert_eq!(records[2].str_value.as_deref(), Some("true"));

        assert_eq!(records[3].name, "urn:dtn:node1:state/mode");
        assert_eq!(records[3].value, None);
        assert_eq!(records[3].str_value.as_deref(), Some("STANDBY"));

        assert_eq!(records[4].name, "urn:dtn:node1:energy_total");
        assert_eq!(records[4].value, Some(125.5));
        assert_eq!(records[4].str_value.as_deref(), Some("125.5"));
    }

    #[test]
    fn test_parse_relative_time_anchoring() {
        let json_data = r#"[
            {"bn": "urn:sensor:", "n": "hum", "v": 55.0, "t": -10}
        ]"#;

        let fallback_ms = 1731000000000i64;
        let records = parse_senml_payload(json_data.as_bytes(), fallback_ms).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].timestamp_ms, fallback_ms - 10000);
    }

    #[test]
    fn test_parse_cbor_senml_pack_and_sum() {
        use ciborium::value::Value;
        let entries = vec![
            Value::Map(vec![
                (Value::Integer((-1).into()), Value::Integer(10.into())),
                (
                    Value::Integer((-2).into()),
                    Value::Text("urn:cbor:station:".to_string()),
                ),
                (Value::Integer((-3).into()), Value::Float(1731000050.0)),
                (Value::Integer((-6).into()), Value::Float(10.0)),
                (
                    Value::Integer(0.into()),
                    Value::Text("solar_amps".to_string()),
                ),
                (Value::Integer(1.into()), Value::Text("A".to_string())),
                (Value::Integer(2.into()), Value::Float(4.8)),
            ]),
            Value::Map(vec![
                (
                    Value::Integer(0.into()),
                    Value::Text("rx_packets".to_string()),
                ),
                (Value::Integer(5.into()), Value::Float(7.0)),
            ]),
        ];

        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(&Value::Array(entries), &mut cbor_bytes).unwrap();

        let records = parse_senml_payload(&cbor_bytes, 1700000000000).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].name, "urn:cbor:station:solar_amps");
        assert_eq!(records[0].unit.as_deref(), Some("A"));
        assert_eq!(records[0].value, Some(4.8));
        assert_eq!(records[0].timestamp_ms, 1731000050000);

        assert_eq!(records[1].name, "urn:cbor:station:rx_packets");
        assert_eq!(records[1].value, Some(17.0));
    }

    #[test]
    fn test_rejects_oversized_payload_and_too_many_records() {
        let oversized = vec![b' '; MAX_SENML_PAYLOAD_BYTES + 1];
        assert!(parse_senml_payload(&oversized, 1700000000000).is_err());

        let too_many: Vec<serde_json::Value> = (0..=MAX_SENML_RECORDS)
            .map(|i| serde_json::json!({"n": format!("m{i}"), "v": 1.0}))
            .collect();
        let bytes = serde_json::to_vec(&too_many).unwrap();
        assert!(bytes.len() < MAX_SENML_PAYLOAD_BYTES);
        assert!(parse_senml_payload(&bytes, 1700000000000).is_err());
    }

    #[test]
    fn test_rejects_invalid_bver_and_missing_value_fields() {
        let bad_bver_json = br#"[{"bver": 9, "n": "temp", "v": 20.0}]"#;
        assert!(parse_senml_payload(bad_bver_json, 1700000000000).is_err());

        let no_val_json = br#"[{"bn": "urn:s:", "n": "temp"}]"#;
        assert!(parse_senml_payload(no_val_json, 1700000000000).is_err());

        // CBOR with bad bver
        use ciborium::value::Value;
        let bad_bver_cbor = vec![Value::Map(vec![
            (Value::Integer((-1).into()), Value::Integer(11.into())),
            (Value::Integer(0.into()), Value::Text("m".into())),
            (Value::Integer(2.into()), Value::Float(1.0)),
        ])];
        let mut buf = Vec::new();
        ciborium::into_writer(&Value::Array(bad_bver_cbor), &mut buf).unwrap();
        assert!(parse_senml_payload(&buf, 1700000000000).is_err());

        // CBOR with no value field
        let no_val_cbor = vec![Value::Map(vec![(
            Value::Integer(0.into()),
            Value::Text("m".into()),
        )])];
        buf.clear();
        ciborium::into_writer(&Value::Array(no_val_cbor), &mut buf).unwrap();
        assert!(parse_senml_payload(&buf, 1700000000000).is_err());
    }

    #[test]
    fn test_rejects_non_finite_cbor_floats() {
        use ciborium::value::Value;
        for bad_float in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let entries = vec![Value::Map(vec![
                (Value::Integer(0.into()), Value::Text("bad".into())),
                (Value::Integer(2.into()), Value::Float(bad_float)),
            ])];
            let mut buf = Vec::new();
            ciborium::into_writer(&Value::Array(entries), &mut buf).unwrap();
            assert!(
                parse_senml_payload(&buf, 1700000000000).is_err(),
                "must reject non-finite float {bad_float}"
            );
        }
    }

    #[test]
    fn test_calculate_time_ms_resilience() {
        let fallback = 1_700_000_000_000;
        assert_eq!(calculate_time_ms(f64::NAN, fallback), fallback);
        assert_eq!(calculate_time_ms(f64::INFINITY, fallback), fallback);
        assert_eq!(calculate_time_ms(f64::NEG_INFINITY, fallback), fallback);
        assert_eq!(calculate_time_ms(f64::MAX, fallback), fallback);
        assert_eq!(calculate_time_ms(10.0, fallback), fallback + 10_000);
        assert_eq!(
            calculate_time_ms(1_731_000_000.0, fallback),
            1_731_000_000_000
        );
    }
}
