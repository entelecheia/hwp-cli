//! `hwp fields` — 필드/누름틀 목록(이름·종류·값·명령).

use std::path::Path;

use crate::commands::cat::load_document;

pub fn run(file: &Path, as_json: bool) -> anyhow::Result<()> {
    let doc = load_document(file)?;
    let fields = hwp_convert::list_fields(&doc);

    if as_json {
        let arr: Vec<_> = fields
            .iter()
            .map(|f| {
                serde_json::json!({
                    "kind": f.kind,
                    "ctrl_id": f.ctrl_id,
                    "name": f.name,
                    "command": f.command,
                    "value": f.value,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
        return Ok(());
    }

    if fields.is_empty() {
        println!("필드 없음");
        return Ok(());
    }
    println!("필드 {}개:", fields.len());
    for (i, f) in fields.iter().enumerate() {
        let name = f.name.as_deref().unwrap_or("(이름 없음)");
        println!(
            "  [{i}] {} {} · 이름={name:?} · 값={:?}",
            f.ctrl_id, f.kind, f.value
        );
        if f.name.is_none()
            && let Some(cmd) = &f.command
        {
            println!("        명령: {cmd}");
        }
    }
    Ok(())
}
