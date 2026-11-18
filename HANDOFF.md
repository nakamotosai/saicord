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
| `vendor/windows-capture/` | 上游 2.0.1 + 本地补丁(给 `VideoSettingsBuilder` 加 `hardware_acceleration(bool)`),`[patch.crates-io]` 挂载;见其 `PATCH.md` |

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
- 调试日志在 `%LOCALAPPDATA%\saicord\saicord.log`(只在出错时写)
- **起录依赖系统的 MF 硬件编码**:上游 `windows-capture` 的编码器写死 `MediaTranscoder::SetHardwareAccelerationEnabled(true)`。Windows 的 GRL 一旦缺失/过旧,MF 对硬件 MFT 的激活一律返回 `0xC00D715F MF_E_GRL_VERSION_TOO_LOW`,硬件转码拓扑建不起来,编码器初始化报 `0xC00D6D60`(`MF_E_INVALIDMEDIATYPE`)
  - 2026-11-18 起不再直接失败:`vendor/windows-capture/` 打了补丁,`rec::start` 先按硬件编码起,失败自动用 `hardware_acceleration(false)` 重试并把「已降级软件编码」写进日志 + 弹一次提示;`--no-hw` 可强制软件编码
  - 复现降级:把 `%ProgramData%\Microsoft\MF\Active.GRL` 改名(需管理员)→ `saicord.exe --test capture 200 200 960 540 2 X.mp4 noaudio 120` → 日志出 `改用软件编码重试` + `hardware_acceleration=false`,仍能出片(软件编码本机实测 1080p120≈154fps、1440p120≈84fps、4K60≈41fps,1440p 以上顶不住 120fps)
  - GRL 读自 `HKLM\SYSTEM\CurrentControlSet\Services\PEAuth` 的 `DataPath`(本机 = `C:\ProgramData\Microsoft\MF`)下的 `Active.GRL`;缺了就把它从 `C:\Windows\WinSxS\amd64_microsoft-windows-mfgrl_*\Active.GRL` 复制回去(需管理员,补完立刻生效,不用重启)。放 `System32` 无效、`sfc /scannow` 不修

## 环境

- Rust 1.91 (MSVC)、Windows 11 25H2、3840×2160 @200%
- 本机 VS BuildTools 装了但没在注册表注册(`vswhere -all -format json` 返回 `[]`),裸 shell 里 `cargo build` 会落到 Git 的 `link.exe` 报 `link: extra operand`;先 `call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"` 再 cargo(正解是 BuildTools `--repair`)
- 无第三方运行时依赖;不需要 Node、ffmpeg
