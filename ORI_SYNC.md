# TodoGlass ↔ Ori 同步协议 / Sync Protocol

中英对照。TodoGlass 运行在电脑 A（Windows）上，在本机开启 HTTP API，供 Ori（Grok Bot）通过 **Shell + machineId** 实时读写待办。

Computer A machineId: `<YOUR_MACHINE_ID>`  
API base: `http://127.0.0.1:17831`  
数据文件: `%LOCALAPPDATA%\TodoGlass\data\todos.json`  
安装旁镜像: `C:\Users\<WINDOWS_USER>\AppData\Local\TodoGlassApp\tauri\sync\todos.json`

> 2.1 版仅接受本机原生客户端。健康检查可直接调用；其他请求需要配对。请在 TodoGlass 设置中展开「同步配對與資料還原」，查看配对密钥，保存在本机受信任客户端。单个请求体上限 1 MiB。不要把密钥发到聊天或写入日志。
>
> 下列 PowerShell 示例使用此 Windows 用户专用的本机配对文件，只把密钥发给指定的回环服务。在查看配对密钥后，先定义请求头：
>
> ```powershell
> $pairingPath = Join-Path $env:LOCALAPPDATA 'TodoGlass/data/bot-pairing.key'
> $headers = @{ Authorization = 'Bearer ' + [IO.File]::ReadAllText($pairingPath).Trim() }
> ```

---

## 数据模型 / Schema

```json
{
  "version": 1,
  "goal": "",  // unused
  "goalUpdatedAt": 1730000000000,
  "updatedAt": 1730000000000,
  "completionCount": 12,
  "lastActivityAt": 1730000000000,
  "items": [
    {
      "id": "t_xxx",
      "text": "列阅读清单",
      "done": false,
      "order": 0,
      "updatedAt": 1730000000000,
      "deleted": false
    }
  ]
}
```

合并规则 / Merge: **按条目 `updatedAt` 最后写入获胜（last-write-wins）**；`goal` 按 `goalUpdatedAt`；列表按 `order` 排序。桌面完成项在同一次写入中移除，并在 `deletedIds` 中保留 ID 和删除时间。合并时忽略已删除 ID；新任务应使用新 ID。

---

## Ori 可复制命令 / Copy-paste for Ori

以下命令在 **电脑 A** 上执行（Shell 带 `machineId: <YOUR_MACHINE_ID>`）。PowerShell：

### 1. 健康检查 / Health

```powershell
Invoke-RestMethod http://127.0.0.1:17831/api/health
```

### 1b. 待辦活動燈

標題旁的圓點從最近一次新增或完成待辦開始計時。未滿 12 小時為綠色，滿 12 小時為黃色，滿 24 小時為紅色。配對客戶端實際新增或完成也會計入。自動週期排程、讀取、改字、排序及刪除不會重設計時。首次啟用從開始記錄起算。配對狀態與重新連線可在設定內查看。

### 2. 读取全部状态 / Read state

```powershell
Invoke-RestMethod -Headers $headers http://127.0.0.1:17831/api/state | ConvertTo-Json -Depth 6
```

### 3. （已停用）当前目标 / Goal — UNUSED

界面已移除「当前目标」。`/api/goal` 仍可调用但不会显示，请勿使用。 The goal field is no longer shown in the UI; the endpoint is kept harmless. Do not use.

<!-- legacy -->

```powershell
Invoke-RestMethod -Headers $headers -Method PUT -Uri http://127.0.0.1:17831/api/goal `
  -ContentType 'application/json; charset=utf-8' `
  -Body '{"goal":"本周读完一本书"}'
```

### 4. 添加待办 / Add item

```powershell
Invoke-RestMethod -Headers $headers -Method POST -Uri http://127.0.0.1:17831/api/items `
  -ContentType 'application/json; charset=utf-8' `
  -Body '{"text":"整理书架"}'
```

### 5. 完成待办 / Complete item

先读 state 拿到 `id`，再：

```powershell
Invoke-RestMethod -Headers $headers -Method PATCH -Uri http://127.0.0.1:17831/api/items/t_REPLACE_ID `
  -ContentType 'application/json; charset=utf-8' `
  -Body '{"done":true}'
```

桌面端若该项仍显示，用户点圆圈会播动画；Ori 直接标记 `done` 后应用会在下次刷新时移除（推荐再调一次删除风格：同一 PATCH 带 `"deleted":true`，或由用户在 UI 完成）。

更干净的「完成并移除」：

```powershell
Invoke-RestMethod -Headers $headers -Method PATCH -Uri http://127.0.0.1:17831/api/items/t_REPLACE_ID `
  -ContentType 'application/json; charset=utf-8' `
  -Body '{"done":true,"deleted":true}'
```

### 6. 改文案 / Edit text

```powershell
Invoke-RestMethod -Headers $headers -Method PATCH -Uri http://127.0.0.1:17831/api/items/t_REPLACE_ID `
  -ContentType 'application/json; charset=utf-8' `
  -Body '{"text":"新的待办文案"}'
```

### 7. 拖动排序 / Reorder

按期望顺序传全部 id：

```powershell
Invoke-RestMethod -Headers $headers -Method POST -Uri http://127.0.0.1:17831/api/reorder `
  -ContentType 'application/json; charset=utf-8' `
  -Body '{"ids":["t_id1","t_id2","t_id3"]}'
```

### 8. 整表替换合并 / Put full state (merge)

```powershell
$json = Get-Content -Raw 'C:\Users\<WINDOWS_USER>\AppData\Local\TodoGlassApp\tauri\sync\todos.json'
Invoke-RestMethod -Headers $headers -Method PUT -Uri http://127.0.0.1:17831/api/state `
  -ContentType 'application/json; charset=utf-8' `
  -Body $json
```

### 9. 仅读文件（应用未开 API 时的兜底）/ File fallback

```powershell
Get-Content -Raw "$env:LOCALAPPDATA\TodoGlass\data\todos.json"
# 或安装目录镜像
Get-Content -Raw 'C:\Users\<WINDOWS_USER>\AppData\Local\TodoGlassApp\tauri\sync\todos.json'
```

写文件后需应用在运行且下次保存才会推 UI；**优先用 HTTP API**。

---

## 给 Ori 的行为约定

1. 改待办前先 `GET /api/state`。
2. 新增用 `POST /api/items`，不要自己发明冲突 id（可省略 id）。
3. 完成用 `PATCH` + `done/deleted`。
4. 不要关掉 TodoGlass；它托盘常驻，关闭只能托盘右键「退出」。
5. 盒子（box）无法访问 `127.0.0.1:17831`，必须指定电脑 A 的 machineId。

---



## Runtime (2026-11-07)
- App is now the Tauri build at %LOCALAPPDATA%\TodoGlassApp\tauri\TodoGlass.exe (Electron folder kept as a backup and not started).
- API, port 17831 and data files are unchanged. /api/health also returns "runtime":"tauri".
- PUT /api/settings (internal; does not count as an Ori sync) patches settings.json live.

## Tags (2026-11-07)
- Every todo item may have `"tag"`: `"green" | "blue" | "yellow" | "red" | null`.
- Set or clear it with `PATCH /api/items/:id {"tag":"blue"}` (use `null` or `""` to clear). Any other value is stored as `null`.

## Recurring tasks (常驻任务)
- Stored in `%LOCALAPPDATA%\TodoGlass\data\recurring.json`: `{version, updatedAt, items:[{id, text, time:"HH:MM", days:[0-6], enabled, lastFired:"YYYY-MM-DD"}]}`.
- `days`: 0 = Sunday … 6 = Saturday. An empty list means every day.
- `GET /api/recurring` returns that file.
- `PUT /api/recurring {"items":[...]}` (or a bare array) replaces the whole list. Items without an `id` get a new one, and `lastFired` is preserved for existing ids.
- The app checks every 20 s, and right away at start-up and after each PUT. Once local time is past `time` on a matching day and `lastFired` isn't today, it adds a todo carrying `"recurringId": <id>`. If an uncompleted todo with that `recurringId` already exists, it only refreshes that todo's `updatedAt` and adds no duplicate. It then sets `lastFired` to today. If the app was off at the scheduled time, the task is added on the next start that day.

## 资料还原
设置中可分别还原待办、常驻任务、设置的上一版。还原前的现有文件另存为带时间戳的副本。数据损坏时，应用拒绝覆盖该文件。移除配对前应先退出 TodoGlass，再从本机用户数据目录移除配对文件；重新启动后可以重新配对。
配对客户端清除本地数据和待发送队列里的旧副本后，可在完整合并时回传 ackDeletedIds（ID 对应原删除时间）。主机只清理精确匹配且未随本次 items 再送出的删除记录。没有确认的删除记录会持续保留。
