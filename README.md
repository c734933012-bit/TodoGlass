# TodoGlass

Windows 桌面上的透明待辦清單。你可以把它置頂，在標題列展開輸入框新增待辦，點左側圓圈完成，也可以拖曳排序、加上顏色標記，或安排每週重複的任務。

## 下載與使用

歡迎免費下載使用。到本專案的 [Releases](https://github.com/c734933012-bit/TodoGlass/releases) 下載 `TodoGlass-2.2.0-Windows-x64.zip`，完整解壓縮至可寫入的資料夾，再執行 `TodoGlass.exe`。請保留同一資料夾內的 `WebView2Loader.dll`。適用於有 Microsoft Edge WebView2 Runtime 的 64 位元 Windows。

## 標題旁的圓點

| 距離上次新增或完成待辦 | 顏色 |
| --- | --- |
| 未滿 12 小時 | 🟢 綠色 |
| 滿 12 小時、未滿 24 小時 | 🟡 黃色 |
| 滿 24 小時 | 🔴 紅色 |

滑鼠移到圓點上可查看計時起點。新安裝及從舊版升級時，從第一次開啟 2.2.0 起算。新增或完成待辦會立即重新計時，也包括經配對客戶端實際新增或完成的項目。編輯、排序、刪除、自動排程和單純的同步讀取都會保留原來的計時。

## 本機資料與配對

待辦與設定存放在 `%LOCALAPPDATA%\TodoGlass\data`，程式也會在執行檔旁建立同步副本。從設定的「同步配對與資料還原」可查看本機同步狀態、重新連線、配對客戶端或還原備份。配對金鑰等同於這台電腦的待辦讀寫權限，請保存在本機。介面細節見 [同步協定](ORI_SYNC.md)。

## 從原始碼建置

在 Windows 上安裝 Rust 與相符的 C/C++ 工具鏈、WebView2 Runtime，再於包含 `Cargo.toml` 的目錄執行以下命令。Windows 發行包使用 x86_64 Windows GNU 工具鏈建置。

```sh
cargo test --release --locked
cargo build --release --locked
```

輸出位於 `target/release`。前端是 HTML、CSS 和 JavaScript，隨程式嵌入。Inter 字型和玻璃效果元件的原始授權分別見 [Inter](ui/Inter-LICENSE.txt) 與 [Liquid Glass](LiquidGlass-LICENSE.txt)。
