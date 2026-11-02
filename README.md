# saicord

**框选 → 开始 → 自动出片到桌面。** Windows 上的极简区域录屏，Rust 原生实现。

没有编辑器、没有设备选择、没有设置页、没有开机自启。双击就跑，F9 停，视频已经在桌面上了。

```
双击 saicord.exe（或按 F8）
  └─ 拖出要录的矩形
       └─ 点「● 开始」
            └─ F9 停止
                 └─ 桌面\录屏_20261102_104139.mp4
                      └─ 回到托盘，随时再来一次
```

Esc / 右键 = 取消框选，回托盘。

## 为什么又一个录屏工具

因为现成的太重。同类 Electron 方案常驻 6 个进程、674 MB 内存；saicord 是单个 500 KB 的可执行文件、单进程：框选时 48 MB（那 31.6 MB 是 4K 遮罩位图，选完立即释放），待机 18 MB，录制中 117 MB（4K 全屏采集池 + 硬件编码器）。功能砍到只剩一条链路：**抓屏 → 编码 → 落盘**。

## 功能

- 拖拽框选任意矩形区域，实时显示尺寸
- 录制期间**选区边框常亮**（画在选区外侧，不会进画面）
- 系统声音（扬声器 loopback）开关，音画同步起算
- F9 或点计时条停止，自动保存到桌面
- 按显示器 DPI 缩放整套界面（200% 下按钮不会变小）
- Esc / 右键随时取消

## 托盘与快捷键

右下角托盘图标：左键单击 = 开始框选；右键菜单 = 开始录制 / 停止录制 / 打开配置文件 / 退出。录制完自动回到托盘，不会退出。

快捷键写在 `%APPDATA%\saicord\config.txt`，改完重启生效：

```
# 键名写法：Ctrl / Alt / Shift / Win 用 + 连接
fps          = 120     # 三档：30=低 / 60=中 / 120=高
update_check = on      # 启动时查新版本，on / off
start_hotkey = F8
stop_hotkey  = F9
```

帧率也可以在托盘右键菜单里点「帧率：高 120 fps」循环切换（30 → 60 → 120），会写回配置文件，下次录制生效。默认 **120fps（高）**。

支持 `F8`、`F9`、`Shift+F8`、`Ctrl+Alt+R`、单字母等。工具条上的提示会跟着变。

单实例：再双击一次 exe 不会起第二个进程，而是把托盘里的那个叫出来开始框选。

`saicord.exe --tray` 只驻留托盘、不弹框选（适合开机自启）。

## 自动更新

启动 6 秒后自动查一次 GitHub 最新 release（之后每 6 小时一次）。有新版本时：

1. 右下角弹系统通知，屏幕下方弹一条提示；
2. 托盘右键菜单多出一项「**升级到 vX.Y.Z（自动下载安装）**」；
3. 点它就下载官方 exe → 把正在运行的自己改名备份 → 换上新文件 → 自动重启，下次启动清掉备份。

想关掉：配置里 `update_check = off`。托盘菜单里的「检查更新」可随时手动查一次。

命令行也可以直接升级：`saicord.exe --upgrade`（启动后立刻查+下载+替换+重启，适合计划任务）。

## 技术栈

| 环节 | 实现 |
|---|---|
| 抓屏 | Windows Graphics Capture（D3D11 侧 `CopySubresourceRegion` 裁切，只回读选区） |
| 编码 | Media Foundation 硬件 H.264 + AAC，MP4 封装 |
| 音频 | WASAPI loopback 采默认播放设备，float32→16-bit PCM 直接送编码器 |
| 选区遮罩 | 分层窗口 + per-pixel alpha（`UpdateLayeredWindow`） |
| 界面 | 原生 Win32，无 GUI 框架 |

零第三方运行时依赖，不打包 ffmpeg，不需要 Node。

## 构建

```bash
cargo build --release
# target/release/saicord.exe
```

需要 Rust 1.85+（依赖 `windows-capture` 使用 edition 2024）和 MSVC 工具链。

## 已知限制

- 只覆盖光标所在的显示器；跨屏框选不支持
- 选区最小 32×32 逻辑像素
- 只录系统声音（扬声器），不录麦克风；两者不做混音
- 停止只认快捷键 / 点击计时条

## Roadmap

- [ ] 麦克风采集（与系统声二选一，不混音）
- [ ] 多显示器 / 跨屏选区
- [ ] 保存目录可配置
- [ ] 录制后「打开所在文件夹」

## English

Minimal region screen recorder for Windows. Drag a rectangle, press start, press F9, get an MP4 with system audio on your Desktop. Built in Rust on Windows Graphics Capture + Media Foundation — no Electron, no ffmpeg, no editor UI.

## 许可

MIT

## 已知限制

- 硬件编码器要求选区不小于约 145×49 像素，界面已限制为 160×64
- 高刷新率显示器（144Hz）上录 60fps 会有约 4% 重复帧（143Hz 除不尽 60fps），录 30fps 时为 0
- 点「开始」到真正出画面约 1 秒（编码器初始化），这段时间计时条显示「启动中」且不计时
- 关闭声音时 MP4 里仍会带一条空的音轨（编码库实现限制，0 字节）
