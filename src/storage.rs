use serde_json::Value;
use std::{
    fs,
    io::{ErrorKind, Write},
    path::Path,
};

pub type Outcome<T> = Result<T, String>;

fn validate(name: &str, bytes: &[u8]) -> Outcome<Value> {
    let data: Value =
        serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
            .map_err(|_| format!("{name} 無法讀取，請從設定還原備份"))?;
    if !data.is_object()
        || (matches!(name, "todos.json" | "recurring.json")
            && !data
                .get("items")
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().all(Value::is_object)))
    {
        return Err(format!("{name} 格式異常，已保留原檔"));
    }
    Ok(data)
}
pub fn read(dir: &Path, name: &str) -> Outcome<Option<Value>> {
    match fs::read(dir.join(name)) {
        Ok(bytes) => validate(name, &bytes).map(Some),
        Err(e) if e.kind() == ErrorKind::NotFound && !dir.join(format!("{name}.bak")).exists() => {
            Ok(None)
        }
        Err(e) => Err(format!("讀取 {name} 失敗：{e}")),
    }
}
pub fn replace(path: &Path, bytes: &[u8]) -> Outcome<()> {
    let run = || -> std::io::Result<()> {
        let dir = path.parent().ok_or(ErrorKind::InvalidInput)?;
        fs::create_dir_all(dir)?;
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        tmp.write_all(bytes)?;
        tmp.as_file().sync_all()?;
        tmp.persist(path)?;
        Ok(())
    };
    run().map_err(|e| {
        format!(
            "儲存 {} 失敗：{e}",
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    })
}
pub fn save(dir: &Path, name: &str, value: &Value) -> Outcome<()> {
    let target = dir.join(name);
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    validate(name, &bytes)?;
    match fs::read(&target) {
        Ok(previous) => {
            validate(name, &previous)?;
            if previous == bytes {
                return Ok(());
            }
            replace(&dir.join(format!("{name}.bak")), &previous)?;
        }
        Err(e) if e.kind() == ErrorKind::NotFound && !dir.join(format!("{name}.bak")).exists() => {}
        Err(e) => return Err(format!("讀取 {name} 失敗：{e}")),
    }
    replace(&target, &bytes)
}
pub fn recover(dir: &Path, name: &str) -> Outcome<()> {
    if !matches!(name, "todos.json" | "recurring.json" | "settings.json") {
        return Err("未知檔案".into());
    }
    let backup =
        fs::read(dir.join(format!("{name}.bak"))).map_err(|e| format!("找不到可用備份：{e}"))?;
    validate(name, &backup)?;
    let target = dir.join(name);
    match fs::read(&target) {
        Ok(bytes) => replace(
            &dir.join(format!("{name}.before-restore-{}", super::now())),
            &bytes,
        )?,
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    replace(&target, &backup)
}
