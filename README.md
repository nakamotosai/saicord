# dogsc-rs

极简区域录屏。**框选 → 开始 → 自动出片到桌面**，没有编辑器、没有设备选择、没有设置页。

## 用法

1. 双击 `dogsc-rs.exe`（或开始菜单 **DogSC-RS**）
2. 在屏幕上拖出要录的区域（Esc / 右键取消）
3. 点「● 开始」开录；麦克风可随时开关
4. **F9** 停止（或点计时条）
5. 视频自动保存到 `桌面\录屏_YYYYMMDD_HHMMSS.mp4`

## 实现

| 环节 | 用的东西 |
|---|---|
| 抓屏 | Windows Graphics Capture（D3D11 侧裁切，只回读选区） |
| 编码 | Media Foundation 硬件 H.264 + AAC，MP4 |
| 音频 | cpal 采默认麦克风，PCM 直接喂编码器 |
| 选区遮罩 | 分层窗口 + per-pixel alpha（`UpdateLayeredWindow`） |
| 录制边框 | 画在选区**外侧**，所以永远不会进画面 |

实测：600×400 / 30fps / 3s ≈ 90 帧，空闲 48 MB，录制中约 150 MB。

## 构建

```bash
cargo build --release
# 产物 target/release/dogsc-rs.exe
```

依赖 Rust 1.85+（`windows-capture` 用 edition 2024）与 MSVC 工具链。

## 已知限制

- 只支持光标所在的那块显示器；多屏跨屏框选不支持
- 选区最小 32×32（逻辑像素）
- 系统声音（扬声器）不录，只录麦克风
- 停止只认 F9 / 点计时条；没有托盘、没有开机自启

## 许可

MIT。抓屏库 `windows-capture` 为 MIT；界面与交互参考 DogSC（laogou717, MIT）的产品思路，代码为独立 Rust 实现。
