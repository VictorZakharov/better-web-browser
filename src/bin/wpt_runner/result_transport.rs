//! Bounded callback reporting, independent of upstream test size.
use crate::report::{HarnessReport, RESULT_MARKER};
use serde::Deserialize;

const CHUNK_MARKER: &str = "__BREEZE_WPT_CHUNK__";
const MAX_REPORT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Deserialize)]
struct Chunk {
    index: usize,
    data: String,
}

pub(crate) fn harness_report(console: &[String]) -> Result<Option<HarnessReport>, String> {
    let Some((end, payload)) = console.iter().enumerate().rev().find_map(|(index, line)| {
        line.find(RESULT_MARKER)
            .map(|start| (index, &line[start + RESULT_MARKER.len()..]))
    }) else {
        return Ok(None);
    };
    let marker: serde_json::Value =
        serde_json::from_str(payload).map_err(|error| error.to_string())?;
    let payload = if let Some(count) = marker.get("chunks") {
        let count = count
            .as_u64()
            .filter(|count| (1..=128).contains(count))
            .ok_or("invalid WPT chunk count")? as usize;
        let mut payload = String::new();
        let mut next = 0;
        for line in &console[..end] {
            let Some(start) = line.find(CHUNK_MARKER) else {
                continue;
            };
            let chunk: Chunk = serde_json::from_str(&line[start + CHUNK_MARKER.len()..])
                .map_err(|error| format!("invalid WPT chunk: {error}"))?;
            if chunk.index != next
                || next >= count
                || payload.len() + chunk.data.len() > MAX_REPORT_BYTES
            {
                return Err("WPT result chunks exceed budget or are out of order".into());
            }
            payload.push_str(&chunk.data);
            next += 1;
        }
        if next != count {
            return Err("incomplete WPT callback report".into());
        }
        payload
    } else {
        payload.to_owned()
    };
    serde_json::from_str(&payload)
        .map(Some)
        .map_err(|error| format!("parse testharness callback report: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chunk(index: usize, data: &str) -> String {
        format!(
            "log: {CHUNK_MARKER}{}",
            serde_json::json!({"index":index,"data":data})
        )
    }
    fn end(count: usize) -> String {
        format!(
            "log: {RESULT_MARKER}{}",
            serde_json::json!({"chunks":count})
        )
    }
    #[test]
    fn assembles_every_subtest_without_changing_statuses() {
        let lines = [
            chunk(0, r#"{"overall":{"status":"OK"},"tests":["#),
            chunk(1, r#"{"name":"example","status":"PASS"}]}"#),
            end(2),
        ];
        let report = harness_report(&lines).unwrap().unwrap();
        assert_eq!(report.overall.status, "OK");
        assert_eq!(report.tests.len(), 1);
    }
    #[test]
    fn refuses_missing_duplicate_and_out_of_order_chunks() {
        for lines in [
            vec![end(1)],
            vec![chunk(1, "{}"), end(1)],
            vec![chunk(0, "{}"), chunk(0, "{}"), end(2)],
            vec![end(129)],
        ] {
            assert!(harness_report(&lines).is_err());
        }
    }
}
