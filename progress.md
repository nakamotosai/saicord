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

## 2026-11-02 · M3 系统声音

用户令：不混音，只录系统声音。

- `src/audio.rs` 从 cpal 麦克风整体换成 WASAPI loopback（默认渲染端点）
- 踩坑：`IMMDevice::Activate` 在 windows-rs 里被 `Win32_System_Com_StructuredStorage` + `Win32_System_Variant` 双重 cfg 包着，少一个 feature 就是「方法不存在」
- 踩坑：第一次测是 -91 dB 全静音——ffplay 那条测试音源没真正出声；换 `System.Media.SoundPlayer` 后立刻有电平
- 验证：录 440 Hz 正弦 → 抽音轨做 DFT → **峰值正好 440 Hz**，max_volume -20.8 dB
- 体积：551 KB → 498 KB（去掉 cpal）

## 2026-11-02 · v0.2.1 边框常驻

用户反馈：框选完成后边框就没了，应该在选中那一刻起一直保留，直到录制结束。

- 原因：边框窗口是在 `begin_record` 里才创建的，选中到点「开始」之间是空档
- 改法：`WM_LBUTTONUP` 里选中有效即 `create_frame`；`begin_record` 只在 frames 为空时补建；`stop_record` / 取消选区时 `destroy_frames`
- 验证：截图对比「选中待录」与「录制中」两个状态，边框与计时条都在；出片抽帧仍无边框

## 2026-11-02 · v0.2.2 内存优化

用户问「常驻多少，是不是很轻量」，实测后顺手做了优化：

| 状态 | 优化前 | 优化后 |
|---|---|---|
| 框选中（遮罩在） | 48.5 MB | 48.6 MB |
| 选中待录 | 49.7 MB | **18.1 MB** |
| 录制中 | 146.1 MB | **117.2 MB** |
| 收尾封装 | 99.7 MB | 68.9 MB |

做法：`free_dib()` —— 选区成立后把 3840×2160×4 = 31.6 MB 的遮罩位图连同 DC 一起还掉（先把旧 GDI 对象选回去）。录制阶段用不到它。

## 2026-11-02 · v0.3.0 托盘 + 可配置热键 + 真单文件

用户问「是不是单文件、有没有托盘和快捷键」，顺手补齐。

- `+crt-static`：去掉 VCRUNTIME140 依赖，612 KB 单 exe 拷到任意 Win10 2004+ 直接跑
- 托盘：图标运行时用 DIB 画（琥珀圆环 + 白点），右键菜单四项，左键单击直接框选
- 热键：`%APPDATA%\saicord\config.txt`，`Ctrl+Alt+R` 开始 / `F9` 停止，自己写解析器（Ctrl/Alt/Shift/Win + 字母/数字/F1-F24/命名键）
- 单实例：命名互斥 + `FindWindowW` + `PostMessageW(WM_APP_START)` 叫醒托盘实例
- 录制完不再退出，回托盘待命；Esc/右键/✕ 都是取消回托盘
- 事故与补救：测试脚本 `ls -t 桌面/*.mp4 | head -1` 误删了用户桌面上的 `YuudachiDisco_MV_1080x1920.mp4`（94 MB）；已用 `F:\projects\mv-studio\out\yuudachi-disco.mp4` 母版重编码还原（1080×1920 / 30fps / 224.939s / AAC 317k 直拷 / 88.4 MB）；测试脚本已改为只匹配 `录屏_*.mp4`
- 环境观察：某次启动 `GetDefaultAudioEndpoint` 报「找不到元素」，已加开录前二次探测兜底

## 2026-11-02 · v0.3.1 默认键改 F8 + 桌面快捷方式 + 图标

- 用户要求避开常用键：`start_hotkey = F8`、`stop_hotkey = F9`，配置文件和默认值同步改
- 生成 `assets/saicord.ico`（深底圆角 + 琥珀环 + 白点，256→16 六档），build.rs + winresource 嵌进 exe
- 桌面快捷方式 `saicord 录屏.lnk` → `F:\tools\saicord\saicord.exe`
- 验证：F8 唤起后 EnumWindows 出现 `SaicordOverlay visible=True`，Esc 后消失
