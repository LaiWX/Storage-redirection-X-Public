# Fork 版本与发行

下游仓库为 `LaiWX/Storage-redirection-X-Public`，维护分支为 `SRX-R`。
上游基线来自 `z1298808165/Storage-redirection-X-Public`。

## 版本来源

`fork-version.properties` 的 `patch` 是全局发行计数，首版为 `0`。
定制发行前手动递增，合入上游时不重置，构建、失败和重试均不自动递增。
仅合入上游新版本时，可以保持 patch 不变。

统一入口为 `.github/scripts/resolve_fork_version.py`：

```powershell
python .github/scripts/resolve_fork_version.py --format json # 解析完整版本
```

解析器读取上游 `Cargo.toml`、`.github/build-version-baseline.json` 和
`update.json`，匹配当前基础版本。已发布的同基础正式版优先；否则使用已记录
的最高 CI 编号。不根据本地提交、脏工作区或 Actions 运行次数增加上游编号。
未知基线会报错，需要先合入上游对应的发行记录。

- CI 基线：`1.2.60-ci.36.patch.0`。
- 正式基线：`1.2.60-patch.0`。

App 和 WebUI 保留标准 SemVer 比较，包括数字 prerelease 标识符。
正式上游基线的 `-patch.N` 虽然在 SemVer 语法中属于 prerelease，仍由发行流程
明确放入 stable 频道；频道判断不依赖该后缀。

APK 与模块共享版本名和 versionCode。安装编号从 `2000000` 开始，依据
`.github/fork-release-state.json` 中成功发行的记录顺序分配。解析和本地构建
不修改记录；未发行的候选版本重复构建与失败重试复用下一编号。
成功发行同一版本后不重复发布，新完整版本获得下一编号。

## 本地签名构建

本机保留 `build/srx-env.ps1` 工具链配置。用 PowerShell 7 执行：

```powershell
. ./build/srx-env.ps1 # 加载本机工具链配置
./scripts/build-local-module.ps1 -NoAdb -OutputDir build/deliverables/patch.0
./scripts/build-local-app.ps1 -NoAdb -OutputDir build/deliverables/patch.0
```

App 入口会自动读取原有用户目录中的
`.android/storage-redirect-x-signing/signing.json`，使用 Windows DPAPI 解密密码。
私钥和密码不进入仓库。显式设置的签名环境变量优先，脚本退出时恢复原值。
包名仍为 `org.srx.manager`。

## Actions 自动发行

普通提交执行格式、Python、App、WebUI 和 Android 13–17 测试门禁。
完整版本相对成功发行记录变化时，发布 `v<完整版本>`，CI 基线进入 beta，
正式基线进入 stable。受限场景和“仅验证CI”提交继续只做验证。
手动推送 tag 时，tag 必须匹配解析结果，使用同一 CI 工作流并强制全场景测试。

发行串行执行，发布前要求完整签名 Secrets：

- `SRX_APP_SIGNING_KEYSTORE_BASE64`
- `SRX_APP_SIGNING_STORE_PASSWORD`
- `SRX_APP_SIGNING_KEY_ALIAS`
- `SRX_APP_SIGNING_KEY_PASSWORD`

本轮没有上传这些 Secrets，配置完成后远端自动签名发行才能运行。

Release 附带 `fork-release.json`，记录完整版本、上游基线、源码提交、频道与
versionCode。草稿失败可以重试；已发布 Release 不会因后续记录提交失败而删除。
后续运行先恢复已发布的 provenance，再分配新编号，避免记录提交失败导致编号复用。
发布成功后才更新 `fork-update.json` 和独立发行状态，并以
`Fork release: ... [skip ci]` 提交，避免循环发行。

App、WebUI 和模块的更新地址均指向本 fork 的 `fork-update.json`；raw GitHub
地址支持 jsDelivr 回退。上游的版本文件和 `update.json` 保持原样。
清单同时保留 App/WebUI 的 stable/beta 频道，以及模块管理器需要的顶层
`version`、`versionCode`、`zipUrl`、`changelog`；模块入口指向最新成功发行。
本地构建不会生成远端发行记录或虚假的下载地址。
