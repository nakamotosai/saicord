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

## 2026-11-02 · v0.4.0 实机压测

无界面测试模式 `--test encode|capture|loop|info`（不进单实例锁、不建任何窗口）。

跑了的矩阵与结论：
- 编码器 1080p30/60/4K30：帧数 900/1200/450 全部无损，文件恒为 CFR 33.3ms，无丢帧无抖动
- 首帧会丢 1 帧（MF sink writer 预热），第 2 帧被复制一次补位，影响 33ms
- 静止画面 PSNR 54.9dB；高速彩色测试图 33dB（chroma 子采样下限）
- 音画：120s 录制，视频 120.033s / 音频 120.000s，漂移 33ms
- 采集节奏：143Hz 屏 + 30fps 最小更新间隔 → 实际 28.6fps，30fps CFR 里约 4% 是补帧
- 连续 40 次起停：句柄 +1.1/次、内存未持续增长（第三方库每会话留 1 个句柄）
- 修掉的 bug：<145×49 选区起录失败留空文件；0 帧停止写出 0 字节 MP4；finish 失败仍报成功；快捷键占用无提示
- 起录延迟：`rec::start` 1027ms（编码器初始化 533ms + WGC/D3D 建会话 494ms）；已改为先显示录制框/计时条再起采集，计时从第一帧开始
- 全链路 UI 测试（PostMessage 模拟拖选，不动真实鼠标）：F8 → 选区 → 开始 → 3s → F9 → 落盘 1400×1000 H.264+AAC，声音同步

## 2026-11-02 · v0.5.0 帧率可配（默认 60）

- 配置 `fps`（默认 60，可 24/30/60/90/120），托盘菜单循环切换并写回
- 采集门限由固定 33ms 改为「按显示刷新收帧 + 目标间隔 45% 放行」，实测：30fps 重复帧 8%→0%，60fps 约 3.8%
- 码率分档 1080p 30/60/120fps = 8/12/16 Mbps，4K = 16/24/32
- 实测：60fps 输出 600 帧/10s 恒定 60 CFR，UI 路径 118 帧/1.95s，起录反馈 73ms

## 2026-11-02 · v0.6.0 自动更新

- `src/update.rs`：WinHTTP 走系统 TLS（不引第三方网络库，+120KB）、解析 /releases/latest 的 tag_name 与 saicord.exe 资产、按 semver 比对
- 启动 6s 后查一次、之后每 6h；有新版本 → 托盘气泡 + 屏幕 toast + 菜单项「升级到 vX.Y.Z」
- 升级：下载（校验 MZ 头、≥256KB）→ 当前 exe 改名 .old → 复制新 exe 就位 → 起新进程 → 退出；启动时清 .old
- 实测：造 v9.9.9 测试 release → 检测 newer=true → 下载替换成功（F:/tmp 副本自替换，881,664 字节）→ 装回 v0.6.0 后报「已是最新 0.6.0」；测试 release 已删
- 踩坑两条：1) 早先想用 `cmd /c timeout & start` 延迟重启，引号被 cmd 解析坏（命令行里出现字面量 `\"`）；2) 直接用 `Command::new(exe)` 立刻起新进程，会撞上单实例互斥被判成第二实例→退出。终解：新进程带 `--wait-for <pid>`，先 OpenProcess+WaitForSingleObject 等旧进程退出再抢锁
- 端到端实测：装 v0.6.0 → 造 v9.9.9 release → `--upgrade` → 下载替换 → 新进程等 0.21s → 正常启动（mutex lasterr=0）→ .old 自动清掉 → 只剩一个进程；测试 release 已删
