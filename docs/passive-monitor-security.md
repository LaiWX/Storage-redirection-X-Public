# 仅监控权限边界修复（patch.2）

仅监控现在使用应用 Monitor hook 和系统侧 inotify 观察，保留 Android 的原始存储视图与权限检查。监控本身不会请求存储挂载、登记共享宿主策略或启用 SQLite 访问豁免。

## 原因与修复

旧实现将未配置应用的存储根交给共享宿主或 scoped FUSE。仅监控策略把请求路由到真实后端；宿主以自身身份打开 `/data/media/<user>`，没有保留请求者的 MediaProvider 权限、目录隔离和脱敏语义。UID 策略登记不是系统权限授权。

启动、reconcile、companion、策略登记、FUSE 根规划及会话构造均阻止仅监控代理。普通应用的 Monitor hook 直接调用原始操作，记录返回值并保留 errno，不创建父目录、不修复权限、不重试底层文件、不删除 SQLite sidecar。MediaProvider 的 SQLite 兼容例外仅适用于明确启用重定向的目标配置，监控开关和路径 token 不构成授权。
