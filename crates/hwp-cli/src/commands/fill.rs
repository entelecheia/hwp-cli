//! `hwp fill` — 충실도 보존 템플릿 채우기.
//!
//! `Contents/section*.xml`의 `{{name}}` 텍스트만 외과 치환하고 나머지 패키지
//! 엔트리(미리보기·compat·BinData)는 바이트 보존한다. hwpx 입력 전용.

use std::collections::BTreeMap;
use std::path::Path;

pub fn run(
    input: &Path,
    output: &Path,
    set: &[String],
    data: Option<&Path>,
    json: bool,
) -> anyhow::Result<()> {
    let mut values: BTreeMap<String, String> = BTreeMap::new();

    if let Some(d) = data {
        let text = std::fs::read_to_string(d)?;
        let map: BTreeMap<String, serde_json::Value> = serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("--data JSON 파싱 실패 ({}): {e}", d.display()))?;
        for (k, v) in map {
            let s = match v {
                serde_json::Value::String(s) => s,
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            };
            values.insert(k, s);
        }
    }
    for pair in set {
        let (k, v) = pair
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--set 형식은 name=value 여야 합니다: {pair}"))?;
        values.insert(k.to_string(), v.to_string());
    }
    if values.is_empty() {
        anyhow::bail!("치환 값이 없습니다 (--set name=value / --data values.json)");
    }

    let counts = hwpx::patch::fill_placeholders(input, output, &values)
        .map_err(|e| anyhow::anyhow!("fill 실패: {e}"))?;
    let total: usize = counts.values().sum();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "output": output.display().to_string(),
                "replaced": total,
                "counts": counts,
            }))?
        );
    } else {
        let unfilled: Vec<&String> = counts
            .iter()
            .filter(|(_, n)| **n == 0)
            .map(|(k, _)| k)
            .collect();
        if !unfilled.is_empty() {
            eprintln!(
                "[hwp] ⚠️  미치환 자리표시자 {}개: {}",
                unfilled.len(),
                unfilled
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        eprintln!("[hwp] {total}건 치환 -> {}", output.display());
    }
    Ok(())
}
