package org.srx.manager.data

import org.srx.manager.root.isSafePackageName

internal fun parseMonitorLogEntries(
    raw: String,
    labelResolver: (String) -> String = { it },
): List<LogEntry> {
  val labelCache = mutableMapOf<String, String>()
  return raw.lineSequence()
      .filter { it.isNotBlank() }
      .mapNotNull { parseMonitorLogLine(it, labelCache, labelResolver) }
      .toList()
      .asReversed()
}

private fun parseMonitorLogLine(
    line: String,
    labelCache: MutableMap<String, String>,
    labelResolver: (String) -> String,
): LogEntry? {
  val parts = line.split('|')
  if (parts.size < 5 || parts[0].isBlank() || parts[1].isBlank() || parts[4].isBlank()) return null
  val extras =
      parts
          .drop(5)
          .mapNotNull {
            val index = it.indexOf('=')
            if (index <= 0) null else it.substring(0, index) to it.substring(index + 1)
          }
          .toMap()
  val ret = extras["ret"]?.toIntOrNull()
  val ok = ret?.let { it >= 0 } ?: !line.contains("error", true)
  val resultGroup =
      when {
        ok -> "ok"
        extras["deny_reason"] == "read_only_rule" -> "deny:read_only_rule"
        else -> "error:${extras["errno"].orEmpty().ifBlank { "unknown" }}"
      }
  val processPkg = parts.getOrNull(1).orEmpty()
  val callerPkg = parts.getOrNull(2).orEmpty()
  val watchPkg = extras["watch_package"].orEmpty()
  val identifyMethod = extras["identify_method"].orEmpty()
  val identifyReliability = extras["identify_reliability"].orEmpty()
  val rawOperation = extras["op"] ?: parts.getOrNull(3).orEmpty()
  val eventRole = inferMonitorEventRole(extras["event_role"], identifyMethod, rawOperation)
  val correlationId = extras["correlation_id"].orEmpty()
  val observerPackage = extras["observer_package"].orEmpty()
  val source = extras["source"].orEmpty()
  val filterOperation = extras["op_filter"] ?: extras["op"] ?: parts.getOrNull(3).orEmpty()
  val operationIntent = monitorOperationIntent(filterOperation)
  val landingPath = parts.getOrNull(4).orEmpty()
  val isModuleExport = isModuleExportRecord(extras, processPkg, callerPkg, watchPkg)
  val pkg =
      if (isModuleExport) ModulePackageName
      else selectMonitorDisplayPackage(processPkg, callerPkg, watchPkg, identifyMethod)
  val operation =
      if (isModuleExport) "export"
      else formatMonitorLogOperationBadge(rawOperation.ifBlank { filterOperation })
  return LogEntry(
      timestamp = parts[0],
      timeText = parts[0].takeIf { it.length >= 16 }?.substring(11, 16) ?: "--:--",
      processPackage = processPkg,
      callerPackage = callerPkg,
      packageName = pkg,
      label =
          if (isModuleExport) ModuleDisplayLabel
          else resolveCachedLabel(pkg, labelCache, labelResolver),
      operation = operation,
      action =
          if (isModuleExport) describeModuleExportOperation(extras)
          else if (extras["combined_action"] == "create_write") "创建并写入"
          else describeMonitorOperation(filterOperation),
      path = landingPath,
      ok = ok,
      errorText = if (ok) "" else describeMonitorFailure(ret, extras),
      fromPath = extras["from"].orEmpty(),
      backendPath = extras["backend"].orEmpty(),
      landingPath = landingPath,
      watchPackage = watchPkg,
      identifyMethod = identifyMethod,
      identifyReliability = identifyReliability,
      eventRole = eventRole,
      correlationId = correlationId,
      observerPackage = observerPackage,
      observationCount = extras["observation_count"]?.toIntOrNull()?.coerceAtLeast(0) ?: 0,
      source = source,
      resultGroup = resultGroup,
      filterOperation = filterOperation,
      operationIntent = operationIntent,
      isModuleWebUiExport = isModuleExport,
  )
}

private fun inferMonitorEventRole(
    explicitRole: String?,
    identifyMethod: String,
    rawOperation: String,
): String {
  if (explicitRole == "request" || explicitRole == "observation") return explicitRole
  if (identifyMethod == "media_provider_fallback" || identifyMethod == "daemon_inotify") {
    return "observation"
  }
  if (
      identifyMethod == "caller" ||
          identifyMethod == "provider_open" ||
          identifyMethod == "provider_mkdir" ||
          identifyMethod == "saf_provider" ||
          rawOperation.startsWith("provider_open")
  ) {
    return "request"
  }
  return "request"
}

private fun resolveCachedLabel(
    packageName: String,
    cache: MutableMap<String, String>,
    labelResolver: (String) -> String,
): String {
  if (packageName.isBlank() || packageName == "-") return packageName
  if (packageName == ModulePackageName) return ModuleDisplayLabel
  return cache.getOrPut(packageName) { labelResolver(packageName) }
}

private fun selectMonitorDisplayPackage(
    processPkg: String,
    callerPkg: String,
    watchPkg: String,
    identifyMethod: String,
): String =
    listOf(
            callerPkg.takeIf { it != processPkg && !it.isIntermediateLogPackage() },
            watchPkg.takeIf { identifyMethod == "watch_package" && !it.isIntermediateLogPackage() },
            callerPkg.takeIf { !it.isIntermediateLogPackage() },
            processPkg.takeIf { !it.isIntermediateLogPackage() },
            callerPkg,
            processPkg,
        )
        .firstOrNull { isSinglePackageName(it.orEmpty()) } ?: processPkg

private fun isSinglePackageName(value: String): Boolean =
    isSafePackageName(value) && value.contains('.')

private fun String?.isIntermediateLogPackage(): Boolean {
  val value = this.orEmpty()
  return value == "com.google.android.providers.media.module" ||
      value == "com.android.providers.media.module" ||
      value == "com.android.providers.media" ||
      value == "com.android.providers.downloads" ||
      value == "com.android.providers.downloads.ui" ||
      value == "com.android.externalstorage" ||
      value == "com.android.mtp" ||
      value.contains(".documentsui") ||
      value.contains(".photopicker")
}

private fun isModuleExportRecord(
    extras: Map<String, String>,
    processPkg: String,
    callerPkg: String,
    watchPkg: String,
): Boolean =
    extras["identify_method"] == "module_export" ||
        extras["source"] == "webui_export" ||
        extras["source"] == "webui_backup" ||
        listOf(processPkg, callerPkg, watchPkg).any { it == ModulePackageName }

private fun describeModuleExportOperation(extras: Map<String, String>): String =
    when (extras["export_kind"].orEmpty().lowercase()) {
      "backup" -> "备份导出"
      "diagnostic",
      "logs",
      "log" -> "日志包导出"
      else ->
          when (extras["source"]) {
            "webui_backup" -> "备份导出"
            "webui_export" -> "日志包导出"
            else -> "模块导出"
          }
    }

// 打开类操作必须按 read/write/create 意图分别描述：读取探测和创建落点是不同行为，
// 统一写成创建会把相册、扫描器的目录读取误报成创建。
private fun describeMonitorOperation(op: String): String {
  val value = op.lowercase()
  val intent = monitorOperationIntent(value)
  return when (value.monitorOperationBaseName()) {
    "provider_open" ->
        when (intent) {
          "read" -> "Provider 读取请求"
          "write" -> "Provider 写入请求"
          "create" -> "Provider 创建请求"
          else -> "Provider 打开请求"
        }
    "open",
    "openat",
    "openat2" ->
        when (intent) {
          "read" -> "文件读取请求"
          "write" -> "文件写入请求"
          "create" -> "带创建意图的文件打开"
          else -> "文件打开请求"
        }
    "mkdir",
    "mkdirat" -> "目录创建请求"
    "mknod",
    "mknodat" -> "文件节点创建请求"
    "create",
    "fuse_create",
    "inotify" -> "创建类文件操作"
    else -> if (op.isBlank()) "文件操作记录" else "文件操作：$op"
  }
}

private fun String.monitorOperationBaseName(): String =
    lowercase().removeSuffix(":create").removeSuffix(":write").removeSuffix(":read")

private fun formatMonitorLogOperationBadge(op: String): String {
  val value = op.trim().lowercase()
  if (value.isBlank()) return "unknown"
  return value.monitorOperationBaseName()
}

private fun monitorOperationIntent(operation: String): String =
    Regex(":(read|write|create)$", RegexOption.IGNORE_CASE)
        .find(operation.trim())
        ?.groupValues
        ?.get(1)
        ?.lowercase()
        .orEmpty()

private fun describeMonitorFailure(ret: Int?, extras: Map<String, String>): String {
  if (ret != null && ret >= 0) return ""
  if (extras["deny_reason"] == "read_only_rule") {
    return "失败：命中只读模式规则"
  }
  return describeMonitorErrno(ret, extras["errno"]?.toIntOrNull())
}

private fun describeMonitorErrno(ret: Int?, errno: Int?): String {
  if (ret != null && ret >= 0) return ""
  return when (errno) {
    1 -> "失败：无权限"
    2 -> "失败：路径不存在"
    13 -> "失败：权限被拒绝"
    20 -> "失败：不是目录"
    21 -> "失败：是目录"
    28 -> "失败：空间不足"
    30 -> "失败：只读文件系统"
    107 -> "失败：传输端未连接"
    null,
    0 -> "失败"
    else -> "失败 errno=$errno"
  }
}

private const val ModulePackageName = "storage.redirect.x"
private const val ModuleDisplayLabel = "存储重定向X"
