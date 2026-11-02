# saicord — 门规

## 每轮改动

- 动 `src/` 前，先把当前版本备份到 `F:\archive\<YYYYMMDD-HHMM>-saicord-<主题>\`
- `cargo build --release` 通过、实机录一段能出片，**当轮必须 commit**，不许攒
- 提交同轮抬 `Cargo.toml` 的 version（0.1.0 → 0.1.1 …），保证「exe 版本 = 存档点」

## 技术红线

- 不引入 Electron / Node / 打包 ffmpeg；编码走 Media Foundation，抓屏走 WGC
- 不加编辑器、不加设备选择器、不加开机自启 —— 产品定位是「一条链路」
- 新增 UI 元素必须按 `scale`（dpi/96）换算，禁止写死物理像素
- 录制边框、计时条一律画在**选区外侧**，绝不靠 `WDA_EXCLUDEFROMCAPTURE` 隐藏

## 验收（报完成前必做）

1. `cargo build --release` 零 error
2. 实机跑一遍：框选 → 开始 → F9 → 桌面出片
3. `ffprobe` 确认分辨率/时长/音轨
4. 抽一帧确认画面里没有边框（琥珀色像素 = 0）

## 文案

- 界面文案中文；代码注释中文；README 中英双语
