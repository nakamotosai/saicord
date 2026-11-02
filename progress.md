# progress

## 2026-11-02 · 立项 → M2 开源首发

**起点**：用户要一个「框选 → 录 → 出片」的轻量录屏，原 Electron 方案常驻 674 MB / 6 进程。

**过程**

1. 评估 fork Electron 版（29k 行、编辑器占 11.7k）后改为 Rust 重写，只保留一条链路。
2. 技术选型见 `docs/adr/0001-capture-stack.md`：WGC + Media Foundation + cpal。
3. 首版调试踩坑：
   - `Monitor::from_index` 是 1-based，传 0 报 `IndexIsLowerThanOne`
   - 分层窗口 alpha 通道（`0xAARRGGBB`）写错会整层全透明——与 sainput `overlay.rs` 里记录的老坑同源
   - 截图看不到录制边框，原因是 `WDA_EXCLUDEFROMCAPTURE` 把窗口从捕获里排除了；改为把边框画在选区外侧后两者兼得
4. 用户反馈 200% 缩放下工具条按钮过小 → 全量改为按 `GetDpiForWindow/96` 缩放。
5. 更名 `saicord`，清理所有移植痕迹，补齐十真相源与设计锁。

**实测数据**

| 项 | 值 |
|---|---|
| 二进制 | 551 KB 单文件 |
| 空闲内存 | 47.7 MB / 6 线程 |
| 录制内存 | 149.7 MB |
| 输出 | H.264 600×400 30fps + AAC 48k，抽帧琥珀像素 0 |

**下一步**：GitHub 公开仓 + Gitea 镜像 + v0.1.0 release；之后进 M3（系统声音）。
