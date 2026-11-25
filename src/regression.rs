use super::*;
#[test]
fn corrupt_file_is_preserved_and_backup_is_recoverable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    let name = "todos.json";
    let original = json!({"items":[{"id":"before","text":"測試"}]});
    storage::save(path, name, &original).unwrap();
    storage::save(path, name, &json!({"items":[]})).unwrap();
    fs::write(path.join(name), b"{").unwrap();
    assert!(storage::read(path, name).is_err());
    assert!(storage::save(path, name, &json!({"items":[]})).is_err());
    assert_eq!(fs::read(path.join(name)).unwrap(), b"{");
    storage::recover(path, name).unwrap();
    assert_eq!(storage::read(path, name).unwrap().unwrap(), original);
}
#[test]
fn deletion_survives_offline_merge() {
    let offline = json!({"items":[{"id":"removed","order":0},{"id":"live","order":1}]});
    let mut current = offline.clone();
    mark_deleted(&mut current, "removed");
    let merged = merge(&current, &offline);
    assert_eq!(merged["items"].as_array().unwrap().len(), 1);
    assert_eq!(merged["items"][0]["id"], "live");
    assert!(merged["deletedIds"]["removed"].is_number());
    let ack = json!({"items":[],"ackDeletedIds":merged["deletedIds"]});
    let confirmed = merge(&merged, &ack);
    assert!(confirmed["deletedIds"].get("removed").is_none());
}
#[test]
fn retry_after_completion_does_not_repeat_today() {
    let mut state = json!({"items":[]});
    let rule = json!({"id":"rule","text":"sample"});
    assert!(ensure_occurrence(&mut state, &rule, "2026-11-25").unwrap());
    let id = state["items"][0]["id"].as_str().unwrap().to_owned();
    mark_deleted(&mut state, &id);
    assert!(!ensure_occurrence(&mut state, &rule, "2026-11-25").unwrap());
    assert!(state["items"].as_array().unwrap().is_empty());
    assert!(ensure_occurrence(&mut state, &rule, "2026-11-26").unwrap());
}
#[test]
fn legacy_order_and_day_format() {
    let mut state = json!({"items":[{"id":"a"},{"id":"b"},{"id":"c"}]});
    reorder_state(&mut state, &[json!("b"), json!("b")]);
    assert_eq!(state["items"][0]["id"], "b");
    assert_eq!(state["items"][1]["id"], "a");
    assert_eq!(minutes("23:59"), Some(1439));
    assert_eq!(minutes("九點"), None);
    assert_eq!(minutes("08-30"), None);
}
#[test]
fn merge_activity_tracks_only_new_tasks_and_completions() {
    let local = json!({"items":[{"id":"a","done":false,"updatedAt":10}], "completionCount":0,"lastActivityAt":1});
    for operation in [
        json!({"items":[{"id":"a","text":"edited","updatedAt":20}]}),
        json!({"items":[{"id":"a","done":true,"updatedAt":1}]}),
        json!({"deletedIds":{"a":20}}),
        json!({"lastActivityAt":9999}),
    ] {
        assert_eq!(merge(&local,&operation)["lastActivityAt"], 1);
    }
    for operation in [
        json!({"items":[{"id":"b","updatedAt":20}]}),
        json!({"items":[{"id":"a","done":true,"updatedAt":20}]}),
        json!({"completionCount":1,"deletedIds":{"a":20}}),
    ] {
        let changed = merge(&local,&operation);
        assert!(changed["lastActivityAt"].as_u64().unwrap() > 1);
        assert_eq!(merge(&changed,&operation)["lastActivityAt"], changed["lastActivityAt"]);
    }
}
