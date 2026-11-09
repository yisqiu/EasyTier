# 路由缓存校时修复：本轮验证记录

日期：2026-11-09。

修复实现提交：`54d3637ea3b2c1061db308d32dd7fc4205ac1fb8`。已推送 `origin/fix/monotonic-route-clock` 并经 `git ls-remote` 核对。后续记录提交只补充验收信息；源码与已通过 50 项测试的评审版本保持一致。

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

GitHub Actions 工作流独立放在已推送的 `ci/monotonic-route-clock-validation` 分支：基于修复分支追加一个 CI 提交。已有 Git 凭据管理器成功完成 workflow 推送，本轮未读取或修改令牌文件。修复分支仅包含源码、测试与文档；此前含 workflow 的提交 `622051ce` 已重新整理。当前 CI 范围与平台验收见文末更新。

源码提交前检查新增文件清单、暂存差异及密钥模式；不包含本机网络配置、密码、私钥、令牌、构建日志或二进制。

## CI 投入边界与分平台验收（2026-11-09 更新）

修复分支固定为 `e7b47470a088eaaf11f9b484f69919b11ae911c5`，实现提交为 `54d3637ea3b2c1061db308d32dd7fc4205ac1fb8`。本节及 workflow 收敛仅在 ci/monotonic-route-clock-validation 分支维护。

- 远程矩阵仅保留 Linux、macOS；每个任务最多 15 分钟。
- 仅运行 easytier crate 的 peers::cache_clock::tests 与 peers::peer_ospf_route::clock_tests。
- Windows 已完成本地 50 项测试；移除远程 Windows 任务和兼容库下载准备。
- 配置收敛采用 [skip ci] 提交消息，复用已启动的验证结果，避免再次启动整个矩阵。
- 外部下载/504 问题后续最多再重试一次；仍失败则记录未验证并停止，继续调整 workflow 需用户新指示。

|平台|本次已验证|尚未验证|
|---|---|---|
|Windows|本地新增 8 项及既有 OSPF 42 项均通过|远程任务因上游下载 504 失败，已从矩阵移除|
|Linux|云端新增路由、原生 CLOCK_BOOTTIME 测试及既有 OSPF 回归通过|实际物理挂起/唤醒场景未实测；两小时挂起使用注入时钟覆盖|
|macOS|云端通过：新增路由测试及原生 CLOCK_MONOTONIC 测试（Native suspend-aware clock）均通过；run ID 42582033922|实际物理挂起/唤醒场景未实测；两小时挂起使用注入时钟覆盖|

Linux 通过记录：https://github.com/yisqiu/EasyTier/actions/runs/42582033922/job/129042815872 。
macOS 通过记录：https://github.com/yisqiu/EasyTier/actions/runs/42582033922/job/129042816145 。
跨平台结果对应的修复源码与冻结修复分支一致；CI-only 调整不包含源码变化。上述记录保留“已通过”与“尚未验证”的区别，不以任务启动或交叉编译代替运行验收。
API 复核：run ID 42582033922（执行提交 9c57ac88）中 ubuntu-latest、macos-latest 的 job 及 Native suspend-aware clock 步骤结论均为 success。windows-latest 失败于 prepare-build 的上游兼容库下载 504；Windows 验收沿用本地 50 项通过结果。
