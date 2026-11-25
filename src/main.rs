#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod storage;
use serde_json::{json, Value};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use storage::Outcome;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

const PORT: u16 = 17831;
static LOCK: Mutex<()> = Mutex::new(());
static SERVER_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static REGION_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static STATUS: Mutex<Option<Value>> = Mutex::new(None);

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("APPDATA"))
        .unwrap_or_else(|_| ".".into());
    let d = PathBuf::from(base).join("TodoGlass").join("data");
    let _ = fs::create_dir_all(&d);
    d
}
fn read_json(name: &str) -> Option<Value> {
    let s = fs::read_to_string(data_dir().join(name)).ok()?;
    serde_json::from_str(s.trim_start_matches('\u{feff}')).ok()
}
fn write_json(name: &str, v: &Value) -> Outcome<()> {
    storage::save(&data_dir(), name, v)
}
#[tauri::command]
fn restore_backup(name: String) -> Outcome<()> {
    let _guard = LOCK.lock().unwrap();
    storage::recover(&data_dir(), &name)
}
// ---------- settings ----------
fn load_settings() -> Value {
    let mut s = json!({"title":"TodoGlass","frost":0,"radius":25,"backdrop":"none","region":false,"keepActive":false});
    if let Some(Value::Object(m)) = read_json("settings.json") {
        for (k, v) in m {
            s[k] = v;
        }
    }
    s
}
fn save_settings(s: &Value) -> Outcome<()> {
    write_json("settings.json", s)
}

// ---------- state ----------
fn load_state() -> Outcome<Value> {
    let mut d = storage::read(&data_dir(), "todos.json")?.unwrap_or(json!({}));
    if !d["items"].is_array() {
        d["items"] = json!([]);
    }
    if !d["goal"].is_string() {
        d["goal"] = json!("");
    }
    for (k, def) in [
        ("goalUpdatedAt", 0u64),
        ("updatedAt", now()),
        ("completionCount", 0),
        ("version", 1),
    ] {
        if !d[k].is_number() {
            d[k] = json!(def);
        }
    }
    // Older files have edit timestamps, but no reliable date for the last completion.
    if d["lastActivityAt"].as_u64().unwrap_or(0) == 0
        && d["activityStartedAt"].as_u64().unwrap_or(0) == 0
    {
        d["activityStartedAt"] = json!(now());
        write_json("todos.json", &d)?;
    }
    Ok(d)
}
fn items(s: &mut Value) -> &mut Vec<Value> {
    s["items"].as_array_mut().unwrap()
}
fn ord(v: &Value) -> f64 {
    v["order"].as_f64().unwrap_or(0.0)
}
fn renumber(s: &mut Value) {
    let it = items(s);
    it.sort_by(|a, b| ord(a).partial_cmp(&ord(b)).unwrap());
    for (i, x) in it.iter_mut().enumerate() {
        x["order"] = json!(i);
    }
}
fn save_state(app: &AppHandle, s: &mut Value) -> Outcome<()> {
    s["updatedAt"] = json!(now());
    write_json("todos.json", s)?;
    if let Ok(exe) = std::env::current_exe() {
        let d = exe.parent().unwrap().join("sync");
        let _ = fs::create_dir_all(&d);
        if storage::replace(
            &d.join("todos.json"),
            &serde_json::to_vec_pretty(s).unwrap(),
        )
        .is_err()
        {
            let _ = app.emit("storage-warning", "同步副本寫入失敗；主資料已保存");
        }
    }
    let _ = app.emit("state", s.clone());
    Ok(())
}
fn new_id() -> String {
    use rand::Rng;
    const A: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut r = rand::thread_rng();
    let mut t = now();
    let mut b36 = String::new();
    while t > 0 {
        b36.insert(0, A[(t % 36) as usize] as char);
        t /= 36;
    }
    let rnd: String = (0..6).map(|_| A[r.gen_range(0..36)] as char).collect();
    format!("t_{}_{}", b36, rnd)
}
fn reorder_state(s: &mut Value, ids: &[Value]) {
    let old = std::mem::take(items(s));
    let positions: std::collections::HashMap<String, usize> = old
        .iter()
        .enumerate()
        .filter_map(|(i, v)| Some((v["id"].as_str()?.to_owned(), i)))
        .collect();
    let mut slots: Vec<_> = old.into_iter().map(Some).collect();
    let mut next = Vec::with_capacity(slots.len());
    for id in ids.iter().filter_map(Value::as_str) {
        if let Some(mut item) = positions.get(id).and_then(|i| slots[*i].take()) {
            item["updatedAt"] = json!(now());
            next.push(item);
        }
    }
    next.extend(slots.into_iter().flatten());
    for (i, x) in next.iter_mut().enumerate() {
        x["order"] = json!(i);
    }
    s["items"] = Value::Array(next);
}
fn mark_deleted(s: &mut Value, id: &str) {
    if !s["deletedIds"].is_object() {
        s["deletedIds"] = json!({});
    }
    if s["deletedIds"].get(id).is_none() {
        s["deletedIds"][id] = json!(now());
    }
    items(s).retain(|x| x["id"] != id);
}
fn merge(local: &Value, remote: &Value) -> Value {
    if !remote.is_object() {
        return local.clone();
    }
    let mut map: Vec<Value> = local["items"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|x| x["id"].is_string())
        .collect();
    let mut deleted = local["deletedIds"].as_object().cloned().unwrap_or_default();
    if let Some(remote_ids) = remote["deletedIds"].as_object() {
        for (id, t) in remote_ids {
            if t.is_u64() {
                deleted.insert(id.clone(), t.clone());
            }
        }
    }
    let mut positions: std::collections::HashMap<String, usize> = map
        .iter()
        .enumerate()
        .map(|(i, v)| (v["id"].as_str().unwrap().to_owned(), i))
        .collect();
    for it in remote["items"].as_array().cloned().unwrap_or_default() {
        if !it["id"].is_string() {
            continue;
        }
        if it["deleted"] == true {
            deleted.insert(it["id"].as_str().unwrap().to_owned(), json!(now()));
            continue;
        }
        match positions.get(it["id"].as_str().unwrap()).copied() {
            Some(p) => {
                if it["updatedAt"].as_f64().unwrap_or(0.0)
                    >= map[p]["updatedAt"].as_f64().unwrap_or(0.0)
                {
                    map[p] = it
                }
            }
            None => {
                positions.insert(it["id"].as_str().unwrap().to_owned(), map.len());
                map.push(it);
            }
        }
    }
    map.retain(|x| x["deleted"] != true && !deleted.contains_key(x["id"].as_str().unwrap_or("")));
    // The paired client acknowledges only after dropping its pending copies.
    if let Some(acks) = remote["ackDeletedIds"].as_object() {
        let mentioned: std::collections::HashSet<&str> = remote["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["id"].as_str())
            .collect();
        for (id, stamp) in acks {
            if !mentioned.contains(id.as_str())
                && local["deletedIds"].get(id) == Some(stamp)
                && deleted.get(id) == Some(stamp)
            {
                deleted.remove(id);
            }
        }
    }
    let previous: std::collections::HashMap<_, _> = local["items"].as_array()
        .into_iter().flatten().filter_map(|it| Some((it["id"].as_str()?, it["done"] == true))).collect();
    let changed = map.iter().any(|it| match previous.get(it["id"].as_str().unwrap_or("")) {
        None => true,
        Some(done) => !done && it["done"] == true,
    }) || remote["completionCount"].as_u64().unwrap_or(0) > local["completionCount"].as_u64().unwrap_or(0);
    let mut out = local.clone();
    if changed {
        out["lastActivityAt"] = json!(now());
    }
    out["items"] = json!(map);
    out["deletedIds"] = json!(deleted);
    renumber(&mut out);
    let (mut goal, mut gu) = (
        local["goal"].clone(),
        local["goalUpdatedAt"].as_f64().unwrap_or(0.0),
    );
    let rgu = remote["goalUpdatedAt"].as_f64().unwrap_or(0.0);
    if rgu >= gu {
        if remote["goal"].is_string() {
            goal = remote["goal"].clone();
        }
        if rgu > 0.0 {
            gu = rgu;
        }
    }
    out["goal"] = goal;
    out["goalUpdatedAt"] = json!(gu as u64);
    out["completionCount"] = json!(local["completionCount"]
        .as_u64()
        .unwrap_or(0)
        .max(remote["completionCount"].as_u64().unwrap_or(0)));
    out["updatedAt"] = json!(now());
    out
}

// ---------- status ----------
fn status() -> Value {
    STATUS.lock().unwrap().clone().unwrap_or(
        json!({"apiOk":false,"apiError":null,"lastOriAt":0,"nativeBlur":true,"openSettings":false}),
    )
}
fn set_status(app: &AppHandle, f: impl FnOnce(&mut Value)) {
    let mut g = STATUS.lock().unwrap();
    let mut s = g.clone().unwrap_or_else(|| json!({"apiOk":false,"apiError":null,"lastOriAt":0,"nativeBlur":true,"openSettings": std::env::var("TG_OPEN_SETTINGS").ok().as_deref()==Some("1"),"demo": std::env::var("TG_DEMO").unwrap_or_default()}));
    f(&mut s);
    *g = Some(s.clone());
    drop(g);
    let _ = app.emit("status", s);
}

// ---------- commands ----------
#[tauri::command]
fn get_state() -> Outcome<Value> {
    let _guard = LOCK.lock().unwrap();
    load_state()
}
#[tauri::command]
fn get_status() -> Value {
    status()
}
#[tauri::command]
fn get_settings() -> Value {
    let mut settings = load_settings();
    if let Err(error) = storage::read(&data_dir(), "settings.json") {
        settings["readWarning"] = json!(error);
    }
    settings
}
#[tauri::command]
fn set_settings(app: AppHandle, patch: Value) -> Outcome<Value> {
    let _g = LOCK.lock().unwrap();
    let mut s = load_settings();
    if !patch.is_object() || patch.get("title").is_some_and(|t| !t.is_string()) {
        return Err("設定格式錯誤".into());
    }
    if let Value::Object(m) = patch {
        for (k, v) in m {
            s[k] = v;
        }
    }
    if let Some(Value::Object(m)) = Some(&mut s) {
        m.remove("nativeBlur");
    }
    if let Some(t) = s["title"].as_str() {
        s["title"] = json!(t.chars().take(40).collect::<String>());
    }
    s["frost"] = json!(s["frost"].as_f64().unwrap_or(0.0).clamp(0.0, 100.0));
    s["radius"] = json!(s["radius"].as_f64().unwrap_or(25.0).clamp(25.0, 80.0));
    save_settings(&s)?;
    drop(_g);
    if let Some(w) = app.get_webview_window("main") {
        apply_region(&w);
        apply_backdrop(&w);
    }
    Ok(s)
}
#[tauri::command]
fn set_goal(app: AppHandle, goal: String) -> Outcome<Value> {
    let _g = LOCK.lock().unwrap();
    let mut s = load_state()?;
    s["goal"] = json!(goal);
    s["goalUpdatedAt"] = json!(now());
    save_state(&app, &mut s)?;
    Ok(s)
}
#[tauri::command]
fn add_item(app: AppHandle, text: String) -> Outcome<Value> {
    let _g = LOCK.lock().unwrap();
    let mut s = load_state()?;
    let t = text.trim();
    if t.is_empty() {
        return Ok(s);
    }
    let n = items(&mut s).len();
    items(&mut s).push(
        json!({"id":new_id(),"text":t,"done":false,"order":n,"updatedAt":now(),"deleted":false}),
    );
    s["lastActivityAt"] = json!(now());
    save_state(&app, &mut s)?;
    Ok(s)
}
#[tauri::command]
fn complete_item(app: AppHandle, id: String) -> Outcome<Value> {
    let _g = LOCK.lock().unwrap();
    let mut s = load_state()?;
    let Some(it) = items(&mut s).iter_mut().find(|x| x["id"] == id.as_str()) else {
        return Ok(json!({"state":s,"firedBig":false}));
    };
    if it["done"] == json!(true) {
        return Ok(json!({"state":s,"firedBig":false}));
    }
    it["done"] = json!(true);
    it["updatedAt"] = json!(now());
    let c = s["completionCount"].as_u64().unwrap_or(0).saturating_add(1);
    s["lastActivityAt"] = json!(now());
    s["completionCount"] = json!(c);
    // Completion and its deletion marker must survive together, even if the animation is interrupted.
    mark_deleted(&mut s, &id);
    save_state(&app, &mut s)?;
    Ok(json!({"state":s,"firedBig": c % 5 == 0,"completionCount":c}))
}
#[tauri::command]
fn remove_item(app: AppHandle, id: String) -> Outcome<Value> {
    let _g = LOCK.lock().unwrap();
    let mut s = load_state()?;
    mark_deleted(&mut s, &id);
    for (i, x) in items(&mut s).iter_mut().enumerate() {
        x["order"] = json!(i);
    }
    save_state(&app, &mut s)?;
    Ok(s)
}
#[tauri::command]
fn reorder(app: AppHandle, ids: Vec<Value>) -> Outcome<Value> {
    let _g = LOCK.lock().unwrap();
    let mut s = load_state()?;
    reorder_state(&mut s, &ids);
    save_state(&app, &mut s)?;
    Ok(s)
}
#[tauri::command]
fn update_text(app: AppHandle, id: String, text: String) -> Outcome<Value> {
    let _g = LOCK.lock().unwrap();
    let mut s = load_state()?;
    if let Some(it) = items(&mut s).iter_mut().find(|x| x["id"] == id.as_str()) {
        it["text"] = json!(text);
        it["updatedAt"] = json!(now());
        save_state(&app, &mut s)?;
    }
    Ok(s)
}

// ---------- tags ----------
fn norm_tag(v: &Value) -> Value {
    match v.as_str() {
        Some(t @ ("green" | "blue" | "yellow" | "red")) => json!(t),
        _ => Value::Null,
    }
}
#[tauri::command]
fn set_tag(app: AppHandle, id: String, tag: Value) -> Outcome<Value> {
    let _g = LOCK.lock().unwrap();
    let mut s = load_state()?;
    if let Some(it) = items(&mut s).iter_mut().find(|x| x["id"] == id.as_str()) {
        it["tag"] = norm_tag(&tag);
        it["updatedAt"] = json!(now());
        save_state(&app, &mut s)?;
    }
    Ok(s)
}

// ---------- recurring ----------
fn local_now() -> (String, u32, u32) {
    #[cfg(windows)]
    unsafe {
        let mut t: windows_sys::Win32::Foundation::SYSTEMTIME = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
        return (
            format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay),
            t.wDayOfWeek as u32,
            t.wHour as u32 * 60 + t.wMinute as u32,
        );
    }
    #[allow(unreachable_code)]
    {
        ("1970-01-01".into(), 4, 0)
    }
}
fn load_rec() -> Outcome<Value> {
    let mut r = storage::read(&data_dir(), "recurring.json")?.unwrap_or(json!({}));
    if !r["items"].is_array() {
        r["items"] = json!([]);
    }
    r["version"] = json!(1);
    Ok(r)
}
fn save_rec(app: &AppHandle, items_in: Value) -> Outcome<Value> {
    let old = load_rec()?;
    let mut out = vec![];
    for it in items_in.as_array().cloned().unwrap_or_default() {
        let text = it["text"].as_str().unwrap_or("").trim().to_string();
        if text.is_empty() {
            continue;
        }
        let time = it["time"].as_str().unwrap_or("09:00").to_string();
        if minutes(&time).is_none() {
            return Err("時間格式應為 HH:MM".into());
        }
        let id = it["id"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| new_id().replacen("t_", "r_", 1));
        let days: Vec<u64> = it["days"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|d| d.as_u64())
                    .filter(|d| *d < 7)
                    .collect()
            })
            .unwrap_or_default();
        let prev = old["items"]
            .as_array()
            .and_then(|a| a.iter().find(|x| x["id"] == id.as_str()).cloned())
            .unwrap_or(json!({}));
        let last = prev
            .get("lastFired")
            .or(it.get("lastFired"))
            .cloned()
            .unwrap_or(Value::Null);
        out.push(json!({"id":id,"text":text,"time": time,"days":days,"enabled": it["enabled"].as_bool().unwrap_or(true),"lastFired":last}));
    }
    let r = json!({"version":1,"items":out,"updatedAt":now()});
    write_json("recurring.json", &r)?;
    let _ = app.emit("recurring", r.clone());
    Ok(r)
}
fn minutes(t: &str) -> Option<u32> {
    let (h, m) = t.split_once(':')?;
    if h.len() != 2 || m.len() != 2 || !h.bytes().chain(m.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}
fn ensure_occurrence(st: &mut Value, r: &Value, today: &str) -> Outcome<bool> {
    let rid = r["id"].clone();
    if !st["recurringDates"].is_object() {
        st["recurringDates"] = json!({});
    }
    let key = rid.as_str().ok_or("常駐任務缺少 ID")?;
    if st["recurringDates"][key] == today {
        return Ok(false);
    }
    st["recurringDates"][key] = json!(today);
    let exists = items(st)
        .iter_mut()
        .find(|x| x["recurringId"] == rid && x["done"] != json!(true));
    match exists {
        Some(x) => {
            x["updatedAt"] = json!(now());
        }
        None => {
            let n = items(st).len();
            items(st).push(json!({"id":new_id(),"text":r["text"],"done":false,"order":n,"updatedAt":now(),"deleted":false,"recurringId":rid}));
        }
    }
    Ok(true)
}
fn run_recurring_locked(app: &AppHandle) -> Outcome<()> {
    let (today, wd, mins) = local_now();
    let mut rec = load_rec()?;
    let mut pending_state = None;
    let (mut rc, mut sc) = (false, false);
    for r in rec["items"].as_array_mut().unwrap() {
        if r["enabled"] == json!(false) || r["lastFired"].as_str() == Some(&today) {
            continue;
        }
        let days = r["days"].as_array().cloned().unwrap_or_default();
        if !days.is_empty() && !days.iter().any(|d| d.as_u64() == Some(wd as u64)) {
            continue;
        }
        let t = r["time"].as_str().unwrap_or("09:00");
        let Some(at) = minutes(t) else {
            return Err("常駐任務時間無效，請在設定中修正".into());
        };
        if mins < at {
            continue;
        }
        if pending_state.is_none() {
            pending_state = Some(load_state()?);
        }
        let st = pending_state.as_mut().unwrap();
        sc |= ensure_occurrence(st, r, &today)?;
        r["lastFired"] = json!(today);
        rc = true;
    }
    if sc {
        save_state(app, pending_state.as_mut().unwrap())?;
    }
    if rc {
        write_json("recurring.json", &rec)?;
        let _ = app.emit("recurring", rec);
    }
    Ok(())
}
#[tauri::command]
fn get_recurring() -> Outcome<Value> {
    load_rec()
}
#[tauri::command]
fn set_recurring(app: AppHandle, items: Value) -> Outcome<Value> {
    let _g = LOCK.lock().unwrap();
    save_rec(&app, items)?;
    run_recurring_locked(&app)?;
    load_rec()
}

// ---------- reconnect ----------
fn health_probe() -> bool {
    use std::io::Write;
    let Ok(mut c) = std::net::TcpStream::connect_timeout(
        &([127, 0, 0, 1], PORT).into(),
        std::time::Duration::from_millis(1500),
    ) else {
        return false;
    };
    let _ = c.set_read_timeout(Some(std::time::Duration::from_millis(2000)));
    if c.write_all(
        b"GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:17831\r\nConnection: close\r\n\r\n",
    )
    .is_err()
    {
        return false;
    }
    let mut buf = String::new();
    let _ = c.read_to_string(&mut buf);
    buf.starts_with("HTTP/1.1 200") && buf.contains("\"ok\":true")
}
#[tauri::command]
async fn reconnect(app: AppHandle) -> Value {
    if !SERVER_UP.load(std::sync::atomic::Ordering::SeqCst) {
        let a = app.clone();
        std::thread::spawn(move || http_server(a));
        for _ in 0..20 {
            if SERVER_UP.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    let ok = SERVER_UP.load(std::sync::atomic::Ordering::SeqCst) && health_probe();
    set_status(&app, |s| {
        s["apiOk"] = json!(ok);
        s["apiError"] = if ok {
            Value::Null
        } else {
            json!("本機介面未連線")
        };
    });
    status()
}

// ---------- HTTP API ----------
fn http_server(app: AppHandle) {
    let srv = match tiny_http::Server::http(("127.0.0.1", PORT)) {
        Ok(s) => s,
        Err(e) => {
            let msg = if e.to_string().contains("10048")
                || e.to_string().to_lowercase().contains("in use")
            {
                format!("端口 {} 被占用", PORT)
            } else {
                e.to_string()
            };
            set_status(&app, |s| {
                s["apiOk"] = json!(false);
                s["apiError"] = json!(msg);
            });
            return;
        }
    };
    SERVER_UP.store(true, std::sync::atomic::Ordering::SeqCst);
    set_status(&app, |s| {
        s["apiOk"] = json!(true);
        s["apiError"] = Value::Null;
    });
    struct Down;
    impl Drop for Down {
        fn drop(&mut self) {
            SERVER_UP.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let _down = Down;
    for mut req in srv.incoming_requests() {
        let method = req.method().to_string();
        let url0 = req.url().split('?').next().unwrap_or("/").to_string();
        let (code, body) = process_request(&app, &mut req, &method, &url0);
        let mut r = tiny_http::Response::from_string(if code == 204 {
            String::new()
        } else {
            body.to_string()
        })
        .with_status_code(code);
        r.add_header(tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap());
        let _ = req.respond(r);
    }
}
static KEY_LOCK: Mutex<()> = Mutex::new(());
static KEY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
#[tauri::command]
fn sync_key() -> Outcome<String> {
    let _guard = KEY_LOCK.lock().unwrap();
    if let Some(key) = KEY.get() {
        return Ok(key.clone());
    }
    let file = data_dir().join("bot-pairing.key");
    let key = match fs::read_to_string(&file) {
        Ok(v) if v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit()) => v,
        Ok(_) => return Err("配對金鑰格式無效".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            use rand::RngCore;
            let mut bytes = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut bytes);
            let v: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            storage::replace(&file, v.as_bytes())?;
            v
        }
        Err(e) => return Err(e.to_string()),
    };
    let _ = KEY.set(key.clone());
    Ok(key)
}
fn process_request(
    app: &AppHandle,
    req: &mut tiny_http::Request,
    m: &str,
    path: &str,
) -> (u16, Value) {
    let header = |k: &'static str| {
        req.headers()
            .iter()
            .find(|h| h.field.equiv(k))
            .map(|h| h.value.as_str())
    };
    if !matches!(header("Host"), Some("127.0.0.1:17831" | "localhost:17831"))
        || header("Origin").is_some()
    {
        return (
            403,
            json!({"error":"Only native loopback clients are supported"}),
        );
    }
    if m == "GET" && matches!(path, "/" | "/api" | "/api/health") {
        let last = status()["lastOriAt"].as_u64().unwrap_or(0);
        return (
            200,
            json!({"ok":true,"name":"TodoGlass","runtime":"tauri","version":env!("CARGO_PKG_VERSION"),"port":PORT,"nativeBlur":true,
            "lastOriAt":last,"lastOriAgoMin":if last==0{Value::Null}else{json!(now().saturating_sub(last)/60000)},
            "endpoints":["GET /api/state","PUT /api/state","POST /api/items","PATCH /api/items/:id","POST /api/reorder","PUT /api/goal","GET /api/recurring","PUT /api/recurring"]}),
        );
    }
    let Ok(key) = sync_key() else {
        return (503, json!({"error":"Pairing key unavailable"}));
    };
    if header("Authorization").and_then(|v| v.strip_prefix("Bearer ")) != Some(key.as_str()) {
        return (401, json!({"error":"Pair with TodoGlass first"}));
    }
    const MAX: u64 = 1024 * 1024;
    if req.body_length().is_some_and(|n| n as u64 > MAX) {
        return (413, json!({"error":"Request too large"}));
    }
    let mut bytes = Vec::new();
    if req
        .as_reader()
        .take(MAX + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return (400, json!({"error":"Incomplete request"}));
    }
    if bytes.len() as u64 > MAX {
        return (413, json!({"error":"Request too large"}));
    }
    let b = if m == "GET" {
        Value::Null
    } else {
        match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => return (400, json!({"error":"Invalid JSON"})),
        }
    };
    let result = if m == "PUT" && path == "/api/settings" {
        set_settings(app.clone(), b).map(|s| {
            let _ = app.emit("settings", &s);
            (200, s)
        })
    } else {
        handle(app, m, path, b)
    };
    match result {
        Ok((200, v)) => {
            if path != "/api/settings" {
                let t = now();
                static LAST_PERSIST: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                let old = LAST_PERSIST.load(std::sync::atomic::Ordering::Relaxed);
                set_status(app, |s| s["lastOriAt"] = json!(t));
                if t.saturating_sub(old) > 60000
                    && storage::replace(
                        &data_dir().join("bot-sync.json"),
                        json!({"lastOriAt":t}).to_string().as_bytes(),
                    )
                    .is_ok()
                {
                    LAST_PERSIST.store(t, std::sync::atomic::Ordering::Relaxed);
                }
            }
            (200, v)
        }
        Ok(r) => r,
        Err(e) => (500, json!({"error":e})),
    }
}
fn handle(app: &AppHandle, m: &str, p: &str, b: Value) -> Outcome<(u16, Value)> {
    let _g = LOCK.lock().unwrap();
    Ok(match (m, p) {
        ("GET", "/api/state") => (200, load_state()?),
        ("GET", "/api/recurring") => (200, load_rec()?),
        ("PUT", "/api/recurring") => {
            let items = if b.is_array() {
                b.clone()
            } else {
                b["items"].clone()
            };
            if !items.is_array() {
                return Ok((400, json!({"error":"items array required"})));
            }
            save_rec(app, items)?;
            run_recurring_locked(app)?;
            (200, load_rec()?)
        }
        ("PUT", "/api/state") => {
            let mut s = merge(&load_state()?, &b);
            save_state(app, &mut s)?;
            (200, s)
        }
        ("PUT", "/api/goal") => {
            let mut s = load_state()?;
            s["goal"] = json!(b["goal"].as_str().unwrap_or(""));
            s["goalUpdatedAt"] = json!(now());
            save_state(app, &mut s)?;
            (200, s)
        }
        ("POST", "/api/items") => {
            let t = b["text"]
                .as_str()
                .or(b["title"].as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if t.is_empty() {
                return Ok((400, json!({"error":"text required"})));
            }
            let mut s = load_state()?;
            let n = items(&mut s).len();
            if let Some(id) = b["id"].as_str() {
                if s["deletedIds"].get(id).is_some() || items(&mut s).iter().any(|x| x["id"] == id)
                {
                    return Ok((409, json!({"error":"ID already used"})));
                }
            }
            let it = json!({"id": b["id"].as_str().map(String::from).unwrap_or_else(new_id),"text":t,"done":b["done"].as_bool().unwrap_or(false),
                "order": b["order"].as_f64().map(|x| json!(x)).unwrap_or(json!(n)),"updatedAt":now(),"deleted":false});
            items(&mut s).push(it.clone());
            s["lastActivityAt"] = json!(now());
            renumber(&mut s);
            save_state(app, &mut s)?;
            let item = s["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|x| x["id"] == it["id"])
                .cloned()
                .unwrap_or(it);
            (200, json!({"item":item,"state":s}))
        }
        ("POST", "/api/reorder") => {
            let Some(ids) = b["ids"].as_array().or(b["order"].as_array()).cloned() else {
                return Ok((400, json!({"error":"ids array required"})));
            };
            let mut s = load_state()?;
            reorder_state(&mut s, &ids);
            save_state(app, &mut s)?;
            (200, s)
        }
        ("PATCH", _) if p.starts_with("/api/items/") && !p[11..].contains('/') => {
            let id = pct_decode(&p[11..]);
            let mut s = load_state()?;
            let mut cc = s["completionCount"].as_u64().unwrap_or(0);
            let Some(it) = items(&mut s).iter_mut().find(|x| x["id"] == id.as_str()) else {
                return Ok((404, json!({"error":"not found"})));
            };
            if let Some(t) = b["text"].as_str() {
                it["text"] = json!(t);
            }
            if let Some(d) = b["done"].as_bool() {
                if d && it["done"] != json!(true) {
                    cc = cc.saturating_add(1);
                }
                it["done"] = json!(d);
            }
            if let Some(d) = b["deleted"].as_bool() {
                it["deleted"] = json!(d);
            }
            if let Some(o) = b["order"].as_f64() {
                it["order"] = json!(o);
            }
            if b.get("tag").is_some() {
                it["tag"] = norm_tag(&b["tag"]);
            }
            it["updatedAt"] = json!(now());
            let item = it.clone();
            if cc > s["completionCount"].as_u64().unwrap_or(0) {
                s["lastActivityAt"] = json!(now());
            }
            s["completionCount"] = json!(cc);
            if item["deleted"] == json!(true) {
                mark_deleted(&mut s, &id);
            }
            renumber(&mut s);
            save_state(app, &mut s)?;
            (200, json!({"item":item,"state":s,"completionCount":cc}))
        }
        _ => (404, json!({"error":"not found","path":p})),
    })
}
fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut o = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|v| u8::from_str_radix(v, 16).ok())
            {
                o.push(v);
                i += 3;
                continue;
            }
        }
        o.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&o).into()
}

// ---------- window chrome ----------
fn apply_backdrop(w: &WebviewWindow) {
    #[cfg(windows)]
    {
        use window_vibrancy::*;
        let s = load_settings();
        let frost = s["frost"].as_f64().unwrap_or(0.0);
        let _ = clear_blur(w);
        let _ = clear_acrylic(w);
        let _ = clear_mica(w);
        let _ = clear_tabbed(w);
        let a = (frost * 1.6) as u8; // tint alpha 0..160
        let _ = match s["backdrop"].as_str().unwrap_or("blur") {
            "acrylic" => apply_acrylic(w, Some((18, 18, 24, a))).map(|_| ()),
            "mica" => apply_mica(w, None).map(|_| ()),
            "tabbed" => apply_tabbed(w, None).map(|_| ()),
            "none" => Ok(()),
            "accent" => {
                unsafe {
                    accent(
                        w,
                        s["accent"].as_u64().unwrap_or(4) as u32,
                        ((a.max(1) as u32) << 24) | 0x181212,
                    )
                };
                Ok(())
            }
            _ => apply_blur(w, Some((18, 18, 24, a))).map(|_| ()),
        };
    }
}
fn apply_region(w: &WebviewWindow) {
    #[cfg(windows)]
    unsafe {
        let region_on = load_settings()["region"] == json!(true);
        REGION_ON.store(region_on, std::sync::atomic::Ordering::Relaxed);
        if !region_on {
            use windows_sys::Win32::Graphics::Dwm::*;
            if let Ok(h) = w.hwnd() {
                let pref: i32 = 1;
                DwmSetWindowAttribute(
                    h.0 as _,
                    DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                    &pref as *const _ as _,
                    4,
                );
                let none: u32 = 0xFFFFFFFE;
                DwmSetWindowAttribute(
                    h.0 as _,
                    DWMWA_BORDER_COLOR as u32,
                    &none as *const _ as _,
                    4,
                );
                windows_sys::Win32::Graphics::Gdi::SetWindowRgn(h.0 as _, std::ptr::null_mut(), 1);
            }
            return;
        }
        use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
        let Ok(h) = w.hwnd() else { return };
        if load_settings()["strip"] != json!(false) {
            strip_styles(h.0 as _);
        }
        let Ok(sz) = w.inner_size() else { return };
        let sf = w.scale_factor().unwrap_or(1.0);
        let r = load_settings()["radius"].as_f64().unwrap_or(25.0);
        let d = (2.0 * r * sf).round() as i32;
        let rgn = CreateRoundRectRgn(0, 0, sz.width as i32 + 1, sz.height as i32 + 1, d, d);
        SetWindowRgn(h.0 as _, rgn, 1);
    }
}
#[cfg(windows)]
unsafe fn accent(w: &WebviewWindow, state: u32, color: u32) {
    use windows_sys::Win32::System::LibraryLoader::*;
    #[repr(C)]
    struct AP {
        s: u32,
        f: u32,
        c: u32,
        a: u32,
    }
    #[repr(C)]
    struct WCA {
        attr: u32,
        data: *mut AP,
        size: usize,
    }
    let Ok(h) = w.hwnd() else { return };
    let u = GetModuleHandleA(b"user32.dll\0".as_ptr());
    let Some(f) = GetProcAddress(u, b"SetWindowCompositionAttribute\0".as_ptr()) else {
        return;
    };
    let f: unsafe extern "system" fn(isize, *mut WCA) -> i32 = std::mem::transmute(f);
    let mut ap = AP {
        s: state,
        f: 2,
        c: color,
        a: 0,
    };
    let mut d = WCA {
        attr: 19,
        data: &mut ap,
        size: std::mem::size_of::<AP>(),
    };
    f(h.0 as isize, &mut d);
}
#[cfg(windows)]
unsafe extern "system" fn ncproc(
    h: windows_sys::Win32::Foundation::HWND,
    m: u32,
    wp: usize,
    lp: isize,
    _: usize,
    _: usize,
) -> isize {
    use windows_sys::Win32::UI::{
        Shell::DefSubclassProc,
        WindowsAndMessaging::{WM_NCACTIVATE, WM_NCPAINT},
    };
    // undecorated: never let Windows paint a classic caption/frame
    let _ = wp;
    match m {
        WM_NCACTIVATE => return 1, // never let Windows draw an (in)active caption
        WM_NCPAINT | 0x00AE | 0x00AF => return 0, // NCPAINT, NCUAHDRAWCAPTION, NCUAHDRAWFRAME
        0x0083 if wp != 0 => return 0, // WM_NCCALCSIZE: whole window is client area
        _ => {}
    }
    DefSubclassProc(h, m, wp, lp)
}
#[cfg(windows)]
unsafe fn strip_styles(h: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let st = GetWindowLongPtrW(h, GWL_STYLE) as u32;
    let ns = (st
        & !(WS_CAPTION
            | WS_THICKFRAME
            | WS_SYSMENU
            | WS_MAXIMIZEBOX
            | WS_MINIMIZEBOX
            | WS_BORDER
            | WS_DLGFRAME))
        | WS_POPUP;
    if ns != st {
        SetWindowLongPtrW(h, GWL_STYLE, ns as isize);
        SetWindowPos(
            h,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}
fn restore_bounds(w: &WebviewWindow) {
    let s = load_settings();
    let mons = w.available_monitors().unwrap_or_default();
    let dbg: Vec<String> = mons
        .iter()
        .map(|m| format!("{:?} {:?} {}", m.position(), m.size(), m.scale_factor()))
        .collect();
    let _ = fs::write(data_dir().join("monitors.txt"), dbg.join("\n"));
    // visible = at least an 80x40 px piece of the window lies on some monitor (window may poke above a screen edge)
    let visible = |x: i32, y: i32, wd: i32| {
        mons.iter().any(|m| {
            let (p, z) = (m.position(), m.size());
            let ix = (x + wd).min(p.x + z.width as i32) - x.max(p.x);
            let iy = (y + 400).min(p.y + z.height as i32) - y.max(p.y);
            ix >= 80 && iy >= 40
        })
    };
    if let Some(b) = s.get("boundsPx").filter(|b| b["x"].is_number()) {
        let (x, y, wd, ht) = (
            b["x"].as_i64().unwrap() as i32,
            b["y"].as_i64().unwrap() as i32,
            b["w"].as_u64().unwrap_or(320) as u32,
            b["h"].as_u64().unwrap_or(480) as u32,
        );
        if visible(x, y, wd as i32) {
            let _ = w.set_position(PhysicalPosition::new(x, y));
            let _ = w.set_size(PhysicalSize::new(wd, ht));
            return;
        }
    }
    if let Some(b) = s.get("bounds").filter(|b| b["x"].is_number()) {
        // Electron DIP bounds
        let (x, y) = (b["x"].as_f64().unwrap(), b["y"].as_f64().unwrap());
        let (wd, ht) = (
            b["width"].as_f64().unwrap_or(320.0),
            b["height"].as_f64().unwrap_or(480.0),
        );
        for m in &mons {
            let sf = m.scale_factor();
            let (p, z) = (m.position(), m.size());
            let (dx, dy) = (p.x as f64 / sf, p.y as f64 / sf);
            let (dw, dh) = (z.width as f64 / sf, z.height as f64 / sf);
            if x + 40.0 >= dx && x < dx + dw && y + 10.0 >= dy && y < dy + dh {
                let px = (p.x as f64 + (x - dx) * sf).round() as i32;
                let py = (p.y as f64 + (y - dy) * sf).round() as i32;
                let _ = w.set_position(PhysicalPosition::new(px, py));
                let _ = w.set_size(PhysicalSize::new((wd * sf) as u32, (ht * sf) as u32));
                return;
            }
        }
    }
    if let Some(b) = s.get("bounds").filter(|b| b["x"].is_number()) {
        let (x, y) = (
            b["x"].as_f64().unwrap() as i32,
            b["y"].as_f64().unwrap() as i32,
        );
        if visible(x, y, 100) {
            let _ = w.set_position(PhysicalPosition::new(x, y));
            return;
        }
        // Electron DIP layout differs from physical: clamp into the monitor that contains y
        if let Some(m) = mons
            .iter()
            .find(|m| y >= m.position().y && y < m.position().y + m.size().height as i32)
        {
            let (p, z) = (m.position(), m.size());
            let sf = m.scale_factor();
            let (wd, ht) = (
                (b["width"].as_f64().unwrap_or(320.0) * sf) as i32,
                (b["height"].as_f64().unwrap_or(480.0) * sf) as i32,
            );
            let nx = x.clamp(p.x, p.x + z.width as i32 - wd);
            let ny = y.clamp(p.y, p.y + z.height as i32 - ht);
            let _ = w.set_position(PhysicalPosition::new(nx, ny));
            let _ = w.set_size(PhysicalSize::new(wd as u32, ht as u32));
            return;
        }
    }
    let _ = w.center();
}
fn save_bounds(w: &WebviewWindow) {
    if w.is_minimized().unwrap_or(false) {
        return;
    }
    let (Ok(p), Ok(z)) = (w.outer_position(), w.inner_size()) else {
        return;
    };
    let _g = LOCK.lock().unwrap();
    let mut s = load_settings();
    s["boundsPx"] = json!({"x":p.x,"y":p.y,"w":z.width,"h":z.height});
    if let Err(e) = save_settings(&s) {
        let _ = w.emit("storage-warning", e);
    }
}
fn toggle(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            sync_key,
            restore_backup,
            set_tag,
            get_recurring,
            set_recurring,
            reconnect,
            get_state,
            get_status,
            get_settings,
            set_settings,
            set_goal,
            add_item,
            complete_item,
            remove_item,
            reorder,
            update_text
        ])
        .setup(|app| {
            use tauri_plugin_autostart::ManagerExt;
            if std::env::var("TG_NO_AUTOSTART").is_err() {
                let _ = app.autolaunch().enable();
            }
            set_status(app.handle(), |s| {
                s["lastOriAt"] = read_json("bot-sync.json")
                    .and_then(|v| v["lastOriAt"].as_u64())
                    .map(|x| json!(x))
                    .unwrap_or(json!(0));
            });
            let w = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("TodoGlass")
                .inner_size(320.0, 480.0)
                .min_inner_size(150.0, 100.0)
                .decorations(false)
                .transparent(true)
                .background_color(tauri::window::Color(0, 0, 0, 0))
                .shadow(false)
                .always_on_top(true)
                .resizable(true)
                .disable_drag_drop_handler()
                .maximizable(false)
                .visible(false)
                .skip_taskbar(false)
                .build()?;
            restore_bounds(&w);
            save_bounds(&w);
            #[cfg(windows)]
            unsafe {
                if let Ok(h) = w.hwnd() {
                    if std::env::var("TG_NONC").is_err() {
                        windows_sys::Win32::UI::Shell::SetWindowSubclass(
                            h.0 as _,
                            Some(ncproc),
                            1,
                            0,
                        );
                    }
                }
            }
            apply_backdrop(&w);
            apply_region(&w);
            let _ = w.show();
            let wc = w.clone();
            let (bounds_tx, bounds_rx) = std::sync::mpsc::sync_channel::<()>(1);
            let bounds_window = w.clone();
            std::thread::spawn(move || {
                while bounds_rx.recv().is_ok() {
                    while bounds_rx
                        .recv_timeout(std::time::Duration::from_millis(800))
                        .is_ok()
                    {}
                    save_bounds(&bounds_window);
                }
            });
            w.on_window_event(move |e| match e {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = wc.hide();
                }
                WindowEvent::Resized(_) | WindowEvent::Moved(_) => {
                    if matches!(e, WindowEvent::Resized(_))
                        && REGION_ON.load(std::sync::atomic::Ordering::Relaxed)
                    {
                        apply_region(&wc);
                    }
                    let _ = bounds_tx.try_send(());
                }
                WindowEvent::ScaleFactorChanged { .. } => apply_region(&wc),
                _ => {}
            });
            // tray
            let h = app.handle();
            let show = MenuItem::with_id(h, "toggle", "显示/隐藏", true, None::<&str>)?;
            let top = CheckMenuItem::with_id(h, "top", "置顶", true, true, None::<&str>)?;
            let quit = MenuItem::with_id(h, "quit", "退出", true, None::<&str>)?;
            let menu =
                Menu::with_items(h, &[&show, &top, &PredefinedMenuItem::separator(h)?, &quit])?;
            let top2 = top.clone();
            TrayIconBuilder::with_id("tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("TodoGlass")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, ev| match ev.id().as_ref() {
                    "toggle" => toggle(app),
                    "top" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let on = top2.is_checked().unwrap_or(true);
                            let _ = w.set_always_on_top(on);
                        }
                    }
                    "quit" => {
                        if let Some(w) = app.get_webview_window("main") {
                            save_bounds(&w);
                        }
                        app.exit(0)
                    }
                    _ => {}
                })
                .on_tray_icon_event(|t, ev| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = ev
                    {
                        toggle(t.app_handle())
                    }
                })
                .build(app)?;
            let ah = app.handle().clone();
            std::thread::spawn(move || http_server(ah));
            let ah2 = app.handle().clone();
            std::thread::spawn(move || loop {
                {
                    let _g = LOCK.lock().unwrap();
                    if let Err(e) = run_recurring_locked(&ah2) {
                        let _ = ah2.emit("storage-warning", e);
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(20));
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run");
}

#[cfg(test)]
mod regression;
