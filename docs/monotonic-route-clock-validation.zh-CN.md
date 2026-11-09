# 路由缓存校时修复：本轮验证记录

日期：2026-11-09。

## 基线与范围

- 基于官方提交 `8428a89d2dabc94c97d370ec607c6ca142473626`，对应本机 `2.6.4-8428a89d`。
- 分支 `fix/monotonic-route-clock`，保留 origin 与 upstream 两个远端。
- 修复覆盖本地 peer/foreign-network 缓存年龄、定期续期和增量同步游标。
- 新增时钟和测试文件与修复一起提交；最终使用 `git format-patch` 导出。
- 本轮工作仅涉及源码、隔离测试及分支同步。现网程序、配置、服务、网卡和计划任务均未替换或重启。

## 已完成的验证

Windows 本机以 GNU Rust 工具链、便携 LLVM-MinGW 和 rust-lld 编译实际 easytier crate 的测试程序。
未安装 Visual Studio Build Tools。该上游的 WinDivert C 编译参数包含 MSVC 选项，本机测试环境使用位于仓库外的编译包装器去除这些选项、预包含 stddef.h；源代码与依赖清单保持本轮指定范围。

|检查|结果|
|---|---|
|新增路由回归测试|6 项通过|
|Windows 原生时钟、原生读失败后的连续计时|2 项通过|
|原有 OSPF 路由测试|42 项通过，包含双节点、多节点、断开重连、原始字段转发与凭据回归|
|时钟模块 Linux x86_64 交叉 cargo check|通过|
|时钟模块 macOS ARM64 交叉 cargo check|通过|
|相对官方基线的 protobuf schema、Cargo.toml、Cargo.lock 差异|零差异|

路由测试覆盖系统时间前跳 1200/1245 秒、回拨 1200/86400 秒、挂起两小时、90 秒不可达清理、正常小时续期、foreign-network 版本递增、增量同步重试及确认后的去重。全部通过注入时钟模拟，Windows 实际时间保持原样。

Linux/macOS 完成的是原生时钟模块交叉编译检查；这些平台的整套运行测试，以及独立的“补丁程序 + 官方程序”三节点联调，仍留待后续隔离验收。Windows 的 42 项既有测试属于同一补丁程序内的路由回归。

## 兼容与挂起语义

线上 `last_update` 继续使用实际墙上时间，协议格式及字段含义沿用官方；新增单调时间保存在本地旁表中。连接更新时间和同步游标属于私有内存数据。凭据有效期继续使用实际日期。

Linux/Android 使用 CLOCK_BOOTTIME；macOS/iOS 使用 Darwin CLOCK_MONOTONIC；Windows 使用 Instant/QPC。前两类原生时钟将挂起时间计入年龄。原生读失败时从最后一次读数继续用 Instant 计时；回退时的挂起覆盖能力取决于平台。

同一进程内，foreign-network 发布版本取“旧已发布版本 + 1”和当前 epoch 秒的较大值。跨进程重启时仍沿用官方“新条目版本取 epoch 秒”的初始化方式：若此前发生系统时间回拨，旧接收端可能保留更大版本。官方基线已有相同初始化及比较逻辑；本轮将该既有风险记入说明，未增加持久化版本状态文件。

## 官方更新与发布

更新步骤见 [维护说明](monotonic-route-clock.md#updating-from-upstream)。建议新建升级分支后 rebase 或 merge upstream，解决源码迁移冲突后再运行同一组回归。新版官方已将文件迁到 easytier-core，需要将本地计时元数据和时钟注入一起迁移。

GitHub Actions 工作流单独放在本地 `ci/monotonic-route-clock-validation` 分支：基于修复分支追加一个 workflow 提交，等待用户补齐 workflow 权限后再推送。修复分支仅包含源码、测试与文档；此前含 workflow 的提交 `622051ce` 已重新整理。云端三平台验证尚待执行。

源码提交前检查新增文件清单、暂存差异及密钥模式；不包含本机网络配置、密码、私钥、令牌、构建日志或二进制。
