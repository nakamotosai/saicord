# 本地补丁:windows-capture 2.0.1

这个目录是 <https://crates.io/crates/windows-capture> **2.0.1** 的原样源码 + 一处改动,
由 `Cargo.toml` 的 `[patch.crates-io]` 挂上。

## 为什么改

上游把硬件编码写死:

```rust
media_transcoder.SetHardwareAccelerationEnabled(true)?;   // src/encoder.rs,两处
```

当机器的 Media Foundation 无法激活硬件 MFT 时(GRL 缺失或被吊销、驱动被 MF 拉黑、
TDR 之后没恢复等),`MediaTranscoder` 建拓扑失败,`VideoEncoder::new` 直接返回
`0xC00D6D60 MF_E_INVALIDMEDIATYPE`,调用方没有任何退路 —— 录屏软件在这种机器上就完全不能用了。

## 改了什么

`src/encoder.rs` 单文件(完整 diff 见 `hw-acceleration.patch`):

- `VideoSettingsBuilder` 新增字段 `hardware_acceleration: bool`,默认 `true`;
- 新增 builder 方法 `pub const fn hardware_acceleration(self, enabled: bool)`,与 `.bitrate()` / `.frame_rate()` 同级;
- 私有 `build()` 多返回这个开关,`VideoEncoder::new` / `new_from_stream` 用它替代写死的 `true`。

默认值与上游完全一致,不传就不改变行为。

saicord 侧:`rec::start` 先按硬件编码起,失败自动用 `hardware_acceleration(false)` 重试一次
(日志与界面提示"已用软件编码");`--no-hw` 可直接强制软件编码。

## 升级上游时怎么重打

1. 用新版本覆盖本目录源码:`cargo vendor` 或从 `~/.cargo/registry/src/.../windows-capture-<ver>` 复制;
2. 把 `hw-acceleration.patch` 里的四处改动手工重放(`git apply` 多半也能过,行号会偏):
   字段、默认值、builder 方法、`build()` 返回值、两处 `SetHardwareAccelerationEnabled`;
3. `cargo build` 跑 `saicord --test capture`(一次默认、一次 `--no-hw`)确认两条路都能出片;
4. 上游若已支持关硬件加速,删掉本目录与 `[patch.crates-io]` 直接用上游 API。

## 想还给上游

`hardware_acceleration(bool)` 是纯 additive 的,适合直接提 PR:NiiightmareXD/windows-capture。
本补丁生成时间 2026-11-18。