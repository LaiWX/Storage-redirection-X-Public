package org.srx.manager.ui.screen

import org.srx.manager.data.InstalledApp
import org.srx.manager.data.LogEntry

internal data class LogListItem(val entry: LogEntry, val key: String)

/** 从最旧记录开始编号，新增记录置顶时不会改变已有记录的 key。 */
internal fun buildLogListItems(logs: List<LogEntry>): List<LogListItem> {
  val seen = HashMap<String, Int>()
  return logs
      .asReversed()
      .map { entry ->
        val base = entry.contentKey()
        val ordinal = seen.getOrDefault(base, 0)
        seen[base] = ordinal + 1
        LogListItem(entry, "$base#$ordinal")
      }
      .asReversed()
}

/** 在已分配 key 的列表上搜索，重复记录分别保留。 */
internal fun filterLogListItems(
    items: List<LogListItem>,
    query: String,
    appsByPackage: Map<String, InstalledApp>,
): List<LogListItem> {
  val q = query.trim().lowercase()
  if (q.isBlank()) return items
  return items.filter { item ->
    val entry = item.entry
    val resolvedLabel = appsByPackage[entry.packageName]?.label.orEmpty()
    listOf(
            entry.label,
            resolvedLabel,
            entry.packageName,
            entry.processPackage,
            entry.callerPackage,
            entry.watchPackage,
            entry.eventRole,
            entry.correlationId,
            entry.observerPackage,
            entry.observationCount.toString(),
            entry.operation,
            entry.operationIntent,
            entry.action,
            entry.errorText,
            entry.path,
            entry.landingPath,
            entry.fromPath,
            entry.backendPath,
        )
        .any { value -> value.lowercase().contains(q) }
  }
}

private fun LogEntry.contentKey(): String =
    "$timestamp|$processPackage|$callerPackage|$packageName|$eventRole|$correlationId|$observationCount|$operation|$path|$ok"
