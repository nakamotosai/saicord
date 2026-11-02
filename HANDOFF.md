# HANDOFF

## 一分钟接手

```bash
cd F:/projects/saicord
cargo build --release          # 产物 target/release/saicord.exe
./target/release/saicord.exe   # 双击也行
```

运行后：拖框 → 「● 开始」→ F9 → 桌面出 mp4。

## 代码地图

| 文件 | 职责 |
|---|---|
| `src/main.rs` | 全部窗口与流程：遮罩、工具条、录制边框、计时条、提示条、消息循环 |
| `src/rec.rs` | WGC 采集 handler + MF 编码器 + 裁切/翻转 |
| `src/audio.rs` | WASAPI loopback 线程：抓默认播放设备，float32→i16 直送编码器 |
| `src/util.rs` | 宽字符串、桌面路径、时间戳、mm:ss |

## 关键实现点

- 裁切：`frame.buffer_crop(x0,y0,x1,y1)` 走 GPU `CopySubresourceRegion`，回读量只等于选区
- 方向：MF 的 buffer 路径要 bottom-up，`rec.rs` 里在拷贝时顺手翻转
- 音频起点：`first_frame` 原子标志，采集线程在首帧到达前丢弃采样，保证音画同起
- 音频来源：只录系统声音（`AUDCLNT_STREAMFLAGS_LOOPBACK`），麦克风不在范围内
- 缩放：`s(v) = v * dpi/96`，任何新尺寸都必须过 `s()`
- 边框：四个独立窗口贴在选区**外侧**，不用 `WDA_EXCLUDEFROMCAPTURE`

## 已知坑

- `Monitor::from_index` 1-based；设备名 `\\.\DISPLAYn` 的 n 才是索引
- `windows-capture` 的 `start_free_threaded` 返回的 `CaptureControl`：先取 `callback()`，再 `stop()`（`stop` 会消费 self 并 join）
- `IMMDevice::Activate` 需要 `Win32_System_Com_StructuredStorage` + `Win32_System_Variant` 两个 feature 同时开，否则方法根本不存在
- 停止时顺序必须是：停音频 → 停采集 → `encoder.finish()`，否则音频会在封装结束后继续写入
- 调试日志在 `F:\tmp\saicord.log`（只在出错时写）

## 环境

- Rust 1.91 (MSVC)、Windows 11 25H2、3840×2160 @200%
- 无第三方运行时依赖；不需要 Node、ffmpeg
