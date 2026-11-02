# ADR 0001 · 抓屏与编码技术栈

## 处境

需要在 Windows 10/11 上做一个 500 KB 级、空闲内存 < 60 MB 的区域录屏。可选路线有：Electron + `desktopCapturer` + ffmpeg（现成但重）、GDI `BitBlt`（简单但慢，1080p 30fps 吃满一个核）、DXGI Desktop Duplication（快，但远程桌面/虚拟显示下不可用）、Windows Graphics Capture（系统原生、GPU 侧裁切、支持光标合成）。

编码侧：打包 ffmpeg（+80 MB，但万能）、Media Foundation 硬件编码（系统自带，零体积）、纯 Rust 编码器（openh264/x264-sys，质量与构建复杂度都不划算）。

## 拍板

**抓屏用 Windows Graphics Capture，编码用 Media Foundation，音频用 cpal。**

WGC 的帧是 D3D11 纹理，`CopySubresourceRegion` 只把选区那部分拷进 staging texture，回读量 = 选区面积而不是整屏（3840×2160 全屏回读是 33 MB/帧，裁到 600×400 只剩 0.96 MB）。

MF 的 `IMFSinkWriter` 直接吃 H.264 + AAC，产出的 MP4 微信/浏览器/剪映都能打开，且不增加任何分发体积。

## 落选者

- **Electron + desktopCapturer + ffmpeg-static**：正是被替换掉的形态。6 进程、674 MB 常驻、111 MB 分发体积，与本项目「轻」的第一原则冲突。
- **GDI BitBlt**：实现最简单，但每帧一次全屏 GDI 拷贝，200% 缩放的 4K 屏上直接把帧率拖到 15 以下。
- **DXGI Desktop Duplication**：性能好，但远程桌面会话、虚拟显示适配器、混合显卡机器上会直接不可用（本机就带一个 GameViewer 虚拟显示）。
- **纯 Rust 编码器**：省掉系统依赖，但 H.264 编码质量/速度/构建复杂度三方面都不如系统硬件编码。

## 代价

- 绑定 Windows 10 2004+（WGC 与 `WDA_EXCLUDEFROMCAPTURE` 的下限）
- 无独显或驱动异常时 MF 会回退软编，帧率下降
- 依赖 `windows-capture`（MIT）作为 WGC 的安全封装，后续若它停更需要自己接 WinRT
