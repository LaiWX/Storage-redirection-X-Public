package org.srx.manager.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MonitorLogParserTest {
  @Test
  fun keepsGalleryCreatorForOuaClassifierDirectories() {
    val raw =
        listOf(
                "2026-08-06 00:35:49|com.android.providers.media.module|com.coloros.gallery3d|MKDIR|" +
                    "/storage/emulated/0/oua_classifier|ret=0|errno=0|identify_method=caller|" +
                    "identify_reliability=high|op=mkdir",
                "2026-08-06 00:35:49|com.android.providers.media.module|com.coloros.gallery3d|MKDIR|" +
                    "/storage/emulated/0/oua_classifier/debug|ret=0|errno=0|identify_method=caller|" +
                    "identify_reliability=high|op=mkdir",
                "2026-08-06 00:35:49|com.android.providers.media.module|com.android.providers.media.module|CREATE|" +
                    "/storage/emulated/0/oua_classifier|ret=0|errno=0|identify_method=media_provider_fallback|" +
                    "identify_reliability=fallback|op=inotify|source=public_root|watch_package=com.coloros.gallery3d",
            )
            .joinToString("\n")

    val entries = parseMonitorLogEntries(raw)

    assertEquals(3, entries.size)
    assertEquals(
        setOf("com.coloros.gallery3d", "com.android.providers.media.module"),
        entries.map { it.packageName }.toSet(),
    )
    assertTrue(
        entries.filter { it.operation == "mkdir" }.all { it.packageName == "com.coloros.gallery3d" }
    )
  }

  @Test
  fun classifiesFilesystemFallbackAsObservation() {
    val raw =
        "2026-10-03 10:37:27|com.android.providers.media.module|com.android.providers.media.module|OPEN|" +
            "/storage/emulated/0/Pictures/.gs_fs0/.nomedia|ret=0|errno=0|" +
            "identify_method=media_provider_fallback|identify_reliability=fallback|" +
            "event_role=observation|op=open:write|source=allowed_real_path|" +
            "watch_package=com.taobao.idlefish"

    val entry = parseMonitorLogEntries(raw).single()

    assertEquals("observation", entry.eventRole)
    assertEquals("com.android.providers.media.module", entry.packageName)
    assertEquals("com.taobao.idlefish", entry.watchPackage)
  }

  @Test
  fun infersObservationRoleForLegacyFallbackRecord() {
    val raw =
        "2026-10-03 10:37:27|com.android.providers.media.module|com.android.providers.media.module|CREATE|" +
            "/storage/emulated/0/Pictures/.gs_fs0|ret=0|errno=0|" +
            "identify_method=media_provider_fallback|identify_reliability=fallback|op=inotify|" +
            "source=allowed_real_path|watch_package=com.taobao.idlefish"

    val entry = parseMonitorLogEntries(raw).single()

    assertEquals("observation", entry.eventRole)
  }

  @Test
  fun parsesEntriesBeyondPreviousPreviewLimit() {
    val raw =
        (0..500).joinToString("\n") { index ->
          "2026-06-10 10:20:${(index % 60).toString().padStart(2, '0')}|" +
              "com.example.app|com.example.app|OPEN|/storage/emulated/0/Download/$index.txt|" +
              "op=open|op_filter=open:read|ret=3"
        }

    val entries = parseMonitorLogEntries(raw)

    assertEquals(501, entries.size)
  }

  @Test
  fun prefersRealCaller() {
    val raw =
        "2026-06-10 10:20:30.000|com.android.providers.media.module|com.example.camera|open|" +
            "file:///storage/emulated/0/DCIM/.pending.js|op=open:create|ret=3|identify_method=caller|identify_reliability=high"

    val entries = parseMonitorLogEntries(raw) { "Label:$it" }

    assertEquals(1, entries.size)
    val entry = entries.single()
    assertEquals("com.example.camera", entry.packageName)
    assertEquals("Label:com.example.camera", entry.label)
    assertEquals("open", entry.operation)
    assertEquals("create", entry.operationIntent)
    assertEquals("带创建意图的文件打开", entry.action)
    assertEquals("file:///storage/emulated/0/DCIM/.pending.js", entry.path)
    assertTrue(entry.ok)
  }

  @Test
  fun keepsMountPreparationAlongsideCallerMkdir() {
    val raw =
        listOf(
                "2026-06-10 10:20:01|com.coloros.gallery3d|com.coloros.gallery3d|MKDIR|" +
                    "/storage/emulated/0/.MediaTrash|ret=0|errno=0|identify_method=mount_prep|" +
                    "identify_reliability=high|op=mkdir|source=mount_prep|" +
                    "backend=/data/media/0/Android/data/com.coloros.gallery3d/sdcard/.MediaTrash",
                "2026-06-10 10:20:20|com.android.providers.media.module|bin.mt.plus|MKDIR|" +
                    "/storage/emulated/0/.MediaTrash|ret=0|errno=0|identify_method=caller|" +
                    "identify_reliability=high|op=mkdir",
            )
            .joinToString("\n")

    val entries = parseMonitorLogEntries(raw)

    assertEquals(2, entries.size)
    assertEquals("mount_prep", entries.last().identifyMethod)
    assertEquals("bin.mt.plus", entries.first().callerPackage)
  }

  @Test
  fun keepsRequestPathForProviderOpen() {
    val raw =
        listOf(
                "2026-06-26 16:52:45|com.android.providers.media.module|com.tencent.mm|OPEN|" +
                    "/storage/emulated/0/Download/第三方下载/微信/storage.redirect.x-v1.2.55-local.zip|" +
                    "ret=183|errno=0|identify_method=path_config|identify_reliability=medium|" +
                    "op=open|op_filter=open:create|flags=0x20242",
                "2026-06-26 16:52:45|com.tencent.mm|com.tencent.mm|CREATE|" +
                    "/storage/emulated/0/Download/第三方下载/微信/storage.redirect.x-v1.2.55-local.zip|" +
                    "ret=0|errno=0|identify_method=watch_package|identify_reliability=medium|" +
                    "op=inotify|source=path_mapping|mask=0x100|" +
                    "backend=/data/media/0/Download/第三方下载/微信/storage.redirect.x-v1.2.55-local.zip|" +
                    "from=/storage/emulated/0/Download/Weixin/storage.redirect.x-v1.2.55-local.zip",
            )
            .joinToString("\n")

    val entries = parseMonitorLogEntries(raw)
    assertEquals(2, entries.size)
    val entry = entries.first()

    assertEquals("com.tencent.mm", entry.packageName)
    assertEquals("inotify", entry.operation)
    assertEquals("watch_package", entry.identifyMethod)
    assertEquals("path_mapping", entry.source)
    assertEquals(
        "/storage/emulated/0/Download/Weixin/storage.redirect.x-v1.2.55-local.zip",
        entry.fromPath,
    )
  }

  @Test
  fun parserKeepsMediaStorePendingCommitFinalRecord() {
    val raw =
        "2026-07-03 08:47:24|com.android.providers.media.module|com.tencent.mm|OPEN|" +
            "/storage/emulated/0/Download/WeiXin/storage-redirect-x-backup-20260619-130840.srxbak.zip|" +
            "ret=0|errno=0|identify_method=caller|identify_reliability=high|op=rename|" +
            "op_filter=open:create|from=/storage/emulated/0/Download/WeiXin/.pending-1783058689-storage-redirect-x-backup-20260619-130840.srxbak.zip|" +
            "source=media_store_pending_commit"

    val entry = parseMonitorLogEntries(raw).single()

    assertEquals("com.tencent.mm", entry.packageName)
    assertEquals("rename", entry.operation)
    assertEquals("create", entry.operationIntent)
    assertEquals("带创建意图的文件打开", entry.action)
    assertEquals(
        "/storage/emulated/0/Download/WeiXin/storage-redirect-x-backup-20260619-130840.srxbak.zip",
        entry.path,
    )
    assertEquals(
        "/storage/emulated/0/Download/WeiXin/.pending-1783058689-storage-redirect-x-backup-20260619-130840.srxbak.zip",
        entry.fromPath,
    )
    assertEquals("media_store_pending_commit", entry.source)
  }

  @Test
  fun describesOpenIntentSeparately() {
    val readEntry =
        parseMonitorLogEntries(
                "2026-07-28 21:37:20|com.android.providers.media.module|com.coloros.gallery3d|OPEN|" +
                    "/storage/emulated/0/.MediaTrash/.mediaTrash|ret=131|errno=0|" +
                    "identify_method=caller|identify_reliability=high|op=open|op_filter=open:read|flags=0x84000",
            )
            .single()
    val writeEntry =
        parseMonitorLogEntries(
                "2026-07-28 21:39:05|com.android.providers.media.module|com.example.app|OPEN|" +
                    "/storage/emulated/0/Download/a.json|ret=131|errno=0|" +
                    "identify_method=caller|identify_reliability=high|op=open|op_filter=open:write|flags=0x20202",
            )
            .single()
    val plainEntry =
        parseMonitorLogEntries(
                "2026-07-28 21:39:06|com.example.app|com.example.app|OPEN|" +
                    "/storage/emulated/0/Download/b.json|ret=5|errno=0|op=open",
            )
            .single()

    assertEquals("read", readEntry.operationIntent)
    assertEquals("文件读取请求", readEntry.action)
    assertEquals("write", writeEntry.operationIntent)
    assertEquals("文件写入请求", writeEntry.action)
    assertEquals("", plainEntry.operationIntent)
    assertEquals("文件打开请求", plainEntry.action)
  }

  @Test
  fun describesProviderOpenWithoutIntentSuffix() {
    val entry =
        parseMonitorLogEntries(
                "2026-07-28 21:40:00|com.android.externalstorage|com.example.app|OPEN|" +
                    "/storage/emulated/0/Download/c.apk|ret=0|errno=0|" +
                    "identify_method=saf_provider|identify_reliability=high|op=provider_open|" +
                    "op_filter=provider_open|source=saf_provider|caller_uid=10360"
            )
            .single()

    assertEquals("provider_open", entry.operation)
    assertEquals("", entry.operationIntent)
    assertEquals("Provider 打开请求", entry.action)
  }

  @Test
  fun parserSkipsOnlyMalformedLines() {
    val raw =
        listOf(
                "malformed",
                "2026-06-10 10:20:30.000|com.example.app|com.example.app|inotify|/storage/emulated/0/Download/a.txt|op=monitor_watch|ret=0",
                "2026-06-10 10:21:30.000|com.example.app|com.example.app|inotify|/storage/emulated/0/Download/b.txt|op=inotify|ret=0",
            )
            .joinToString("\n")

    val entries = parseMonitorLogEntries(raw)

    assertEquals(2, entries.size)
    assertEquals("inotify", entries.first().operation)
    assertEquals("/storage/emulated/0/Download/b.txt", entries.first().path)
  }

  @Test
  fun parserCachesLabelsPerPackage() {
    var resolveCount = 0
    val raw =
        listOf(
                "2026-06-10 10:20:30.000|com.example.app|com.example.app|mkdir|/storage/emulated/0/Download/a|op=mkdir|ret=0",
                "2026-06-10 10:21:30.000|com.example.app|com.example.app|mkdir|/storage/emulated/0/Download/b|op=mkdir|ret=0",
            )
            .joinToString("\n")

    val entries =
        parseMonitorLogEntries(raw) {
          resolveCount += 1
          "Label:$it"
        }

    assertEquals(2, entries.size)
    assertEquals(1, resolveCount)
    assertTrue(entries.all { it.label == "Label:com.example.app" })
  }

  @Test
  fun parserMapsFailedErrnoToReadableText() {
    val raw =
        "2026-06-10 10:20:30.000|com.example.app|com.example.app|unlink|" +
            "/storage/emulated/0/Download/a.txt|op=unlink|ret=-1|errno=13"

    val entry = parseMonitorLogEntries(raw).single()

    assertFalse(entry.ok)
    assertEquals("失败：权限被拒绝", entry.errorText)
    assertEquals("文件操作：unlink", entry.action)
  }

  @Test
  fun parserShowsReadOnlyRuleDenyReason() {
    val raw =
        "2026-06-10 10:20:30.000|com.example.app|com.example.app|open|" +
            "/storage/emulated/0/DCIM/a.jpg|op=open|ret=-1|errno=30|deny_reason=read_only_rule"

    val entry = parseMonitorLogEntries(raw).single()

    assertFalse(entry.ok)
    assertEquals("失败：命中只读模式规则", entry.errorText)
  }

  @Test
  fun keepsReadOnlyFailureSeparate() {
    val raw =
        listOf(
                "2026-06-11 14:35:04|com.android.providers.media.module|xyz.nextalone.nnngram|OPEN|" +
                    "/storage/emulated/0/Download/Nnngram/a.apk|ret=189|errno=0|op=open|op_filter=open:create|flags=0x88042",
                "2026-06-11 14:35:54|com.android.providers.media.module|xyz.nextalone.nnngram|OPEN|" +
                    "/storage/emulated/0/Download/Nnngram/a.apk|ret=-1|errno=30|op=openAssetFile|op_filter=open:create|" +
                    "flags=0x241|from=/storage/emulated/0/Download/第三方下载/Nnngram/a.apk|source=media_provider_open|" +
                    "caller_uid=10312|deny_reason=read_only_rule",
            )
            .joinToString("\n")

    val entries = parseMonitorLogEntries(raw)

    assertEquals(2, entries.size)
    assertFalse(entries[0].ok)
    assertEquals("失败：命中只读模式规则", entries[0].errorText)
    assertTrue(entries[1].ok)
  }

  @Test
  fun ignoresArchiveNameOnly() {
    val raw =
        "2026-06-13 19:16:41|com.android.providers.media.module|com.android.providers.media.module|CREATE|" +
            "/storage/emulated/0/Download/storage-redirect-x-logs-20260613-111638.tar.gz|" +
            "ret=0|errno=0|identify_method=owner_uid|identify_reliability=high|op=inotify"

    val entry = parseMonitorLogEntries(raw) { "Label:$it" }.single()

    assertFalse(entry.isModuleWebUiExport)
    assertEquals("com.android.providers.media.module", entry.packageName)
    assertEquals("Label:com.android.providers.media.module", entry.label)
    assertEquals("inotify", entry.operation)
    assertEquals("创建类文件操作", entry.action)
  }

  @Test
  fun ignoresArchiveProviderFallback() {
    val raw =
        "2026-06-13 19:16:41|com.android.providers.media.module|com.android.providers.media.module|CREATE|" +
            "/storage/emulated/0/Download/storage-redirect-x-logs-20260613-111638.tar.gz|" +
            "ret=0|errno=0|identify_method=media_provider_fallback|identify_reliability=fallback|" +
            "op=inotify|source=public_root|watch_package=cn.wps.moffice_eng"

    val entry = parseMonitorLogEntries(raw) { "Label:$it" }.single()

    assertFalse(entry.isModuleWebUiExport)
    assertEquals("com.android.providers.media.module", entry.packageName)
    assertEquals("Label:com.android.providers.media.module", entry.label)
    assertEquals("inotify", entry.operation)
    assertEquals("media_provider_fallback", entry.identifyMethod)
    assertEquals("cn.wps.moffice_eng", entry.watchPackage)
  }

  @Test
  fun labelsModuleDiagnosticExport() {
    val raw =
        "2026-06-13 19:16:41|storage.redirect.x|storage.redirect.x|OPEN|" +
            "/storage/emulated/0/Download/storage-redirect-x-logs-20260613-111638.tar.gz|" +
            "ret=0|errno=0|identify_method=module_export|identify_reliability=high|" +
            "op=provider_open|op_filter=provider_open:write|source=webui_export|export_kind=diagnostic"

    val entry = parseMonitorLogEntries(raw) { "Label:$it" }.single()

    assertTrue(entry.isModuleWebUiExport)
    assertEquals("storage.redirect.x", entry.packageName)
    assertEquals("存储重定向X", entry.label)
    assertEquals("export", entry.operation)
    assertEquals("日志包导出", entry.action)
    assertEquals("module_export", entry.identifyMethod)
  }

  @Test
  fun parserLabelsExplicitModuleBackupExportAsModule() {
    val raw =
        listOf(
                "2026-06-24 12:06:52|com.android.providers.media.module|com.android.providers.media.module|OPEN|" +
                    "/storage/emulated/0/Download/storage-redirect-x-backup-20260624-120650.srxbak.zip|" +
                    "ret=186|errno=0|identify_method=unknown|identify_reliability=none|" +
                    "op=open|op_filter=open:create|flags=0x200c2",
                "2026-06-24 12:06:52|storage.redirect.x|storage.redirect.x|OPEN|" +
                    "/storage/emulated/0/Download/storage-redirect-x-backup-20260624-120650.srxbak.zip|" +
                    "ret=0|errno=0|identify_method=module_export|identify_reliability=high|" +
                    "op=provider_open|op_filter=provider_open:write|source=webui_backup|export_kind=backup",
            )
            .joinToString("\n")

    val entries = parseMonitorLogEntries(raw)
    assertEquals(2, entries.size)
    val entry = entries.first()

    assertTrue(entry.isModuleWebUiExport)
    assertEquals("storage.redirect.x", entry.packageName)
    assertEquals("存储重定向X", entry.label)
    assertEquals("export", entry.operation)
    assertEquals("备份导出", entry.action)
    assertEquals("module_export", entry.identifyMethod)
  }

  @Test
  fun showsProviderFallbackForUnknownSaf() {
    val raw =
        "2026-06-13 19:16:41|com.android.providers.media.module|com.android.providers.media.module|CREATE|" +
            "/storage/emulated/0/Download/export.zip|" +
            "ret=0|errno=0|identify_method=media_provider_fallback|identify_reliability=fallback|" +
            "op=inotify|source=public_root|watch_package=cn.wps.moffice_eng"

    val entry = parseMonitorLogEntries(raw) { "Label:$it" }.single()

    assertFalse(entry.isModuleWebUiExport)
    assertEquals("com.android.providers.media.module", entry.packageName)
    assertEquals("Label:com.android.providers.media.module", entry.label)
    assertEquals("media_provider_fallback", entry.identifyMethod)
  }

  @Test
  fun parserKeepsManagerAppDiagnosticArchiveAttribution() {
    val raw =
        "2026-06-13 19:16:41|org.srx.manager|org.srx.manager|CREATE|" +
            "/storage/emulated/0/Download/storage-redirect-x-logs-20260613-111638.tar.gz|" +
            "ret=0|errno=0|identify_method=caller|identify_reliability=high|op=open"

    val entry = parseMonitorLogEntries(raw) { "Label:$it" }.single()

    assertFalse(entry.isModuleWebUiExport)
    assertEquals("org.srx.manager", entry.packageName)
    assertEquals("Label:org.srx.manager", entry.label)
  }

  @Test
  fun keepsProviderOperationSuffixes() {
    val readEntry =
        parseMonitorLogEntries(
                "2026-07-02 21:54:54|com.android.externalstorage|com.yxer.packageinstalles|OPEN|" +
                    "/storage/emulated/0/Download/第三方下载/1DMP/app.apk|" +
                    "ret=0|errno=0|identify_method=saf_provider|identify_reliability=high|" +
                    "op=provider_open|op_filter=provider_open:read|source=saf_provider|caller_uid=10360",
            )
            .single()
    val createEntry =
        parseMonitorLogEntries(
                "2026-07-02 21:58:15|com.android.externalstorage|org.srx.manager|OPEN|" +
                    "/storage/emulated/0/Download/第三方下载/storage-redirect-x-logs-20260702-215719.tar.gz|" +
                    "ret=0|errno=0|identify_method=saf_provider|identify_reliability=high|" +
                    "op=provider_open|op_filter=provider_open:create|source=saf_provider|caller_uid=10266",
            )
            .single()

    assertEquals("provider_open", readEntry.operation)
    assertEquals("read", readEntry.operationIntent)
    assertEquals("provider_open", createEntry.operation)
    assertEquals("create", createEntry.operationIntent)
  }

  @Test
  fun ignoresAllowedPathWatcherContext() {
    val raw =
        "2026-06-18 04:35:21|com.android.providers.media.module|com.android.providers.media.module|CREATE|" +
            "/storage/emulated/0/Download/第三方下载/DLManager/thumbs|" +
            "ret=0|errno=0|identify_method=owner_uid|identify_reliability=high|op=inotify|" +
            "source=allowed_real_path|mask=0x40000100|" +
            "backend=/data/media/0/Download/第三方下载/DLManager/thumbs|watch_package=cn.wps.moffice_eng"

    val entry = parseMonitorLogEntries(raw).single()

    assertEquals("com.android.providers.media.module", entry.packageName)
    assertEquals("cn.wps.moffice_eng", entry.watchPackage)
  }

  @Test
  fun preservesHistoricalRecordsWithPrivateBackendPath() {
    val raw =
        "2026-07-03 12:30:41|com.android.mtp,com.android.providers.downloads,com.android.providers.media,com.android.soundpicker|" +
            "com.android.providers.downloads|MKDIR|/storage/emulated/0/.xlDownload|" +
            "ret=0|errno=0|identify_method=java_stack|identify_reliability=medium|op=mkdir|" +
            "backend=/data/media/0/Android/data/com.android.providers.downloads/sdcard/.xlDownload|" +
            "from=/storage/emulated/0/.xlDownload|source=sandbox_path"

    val entries =
        parseMonitorLogEntries(
            raw,
        )

    assertEquals(1, entries.size)
  }

  @Test
  fun preservesHistoricalReadRecords() {
    val raw =
        "2026-07-03 12:30:41|com.example.app|com.example.app|OPEN|" +
            "/storage/emulated/0/Download/a.txt|ret=10|errno=0|op=open|op_filter=open:read"

    val entries = parseMonitorLogEntries(raw)

    assertEquals(1, entries.size)
  }

  @Test
  fun preservesRawOperationAndCreateIntent() {
    val raw =
        "2026-07-12 12:34:56|com.example.app|com.example.app|OPEN|" +
            "/storage/emulated/0/Download/a.txt|ret=8|errno=0|" +
            "op=openat2|op_filter=openat2:create|flags=0x40"

    val entry = parseMonitorLogEntries(raw).single()

    assertEquals("openat2", entry.operation)
    assertEquals("openat2:create", entry.filterOperation)
    assertEquals("create", entry.operationIntent)
  }

  @Test
  fun preservesHistoricalRecordsAcrossPathsAndIntents() {
    val raw =
        listOf(
                monitorLine("open:create", "Download/alpha/file.txt"),
                monitorLine("open:read", "Download/beta/file.txt"),
                monitorLine("mkdir", "Android/data/com.example/cache/item"),
            )
            .joinToString("\n")

    val entries =
        parseMonitorLogEntries(
            raw,
        )

    assertEquals(3, entries.size)
    assertEquals(setOf("create", "read", ""), entries.map { it.operationIntent }.toSet())
  }

  @Test
  fun preservesIndependentRepeatedEvents() {
    val request =
        monitorLine("open:create", "Download/a.txt") + "|event_role=request|correlation_id=c1"
    val observation =
        monitorLine("inotify", "Download/a.txt") + "|event_role=observation|correlation_id=c1"
    assertEquals(
        3,
        parseMonitorLogEntries(listOf(request, request, observation).joinToString("\n")).size,
    )
  }

  @Test
  fun readsProducerAggregationFieldsWithoutReaggregating() {
    val raw =
        monitorLine("fuse_create", "Download/a.txt") +
            "|combined_action=create_write|observation_count=3|observer_package=srx_daemon"
    val entry = parseMonitorLogEntries(raw).single()
    assertEquals("创建并写入", entry.action)
    assertEquals(3, entry.observationCount)
    assertEquals("srx_daemon", entry.observerPackage)
  }

  @Test
  fun keepsLegalPendingNamesAndJavascriptNamesAsWritten() {
    val raw =
        listOf(
                monitorLine("open:create", "Download/.pending-123-test.txt"),
                monitorLine("open:create", "Download/.test.js"),
            )
            .joinToString("\n")
    assertEquals(
        listOf(
            "/storage/emulated/0/Download/.test.js",
            "/storage/emulated/0/Download/.pending-123-test.txt",
        ),
        parseMonitorLogEntries(raw).map { it.path },
    )
  }

  private fun monitorLine(operation: String, relativePath: String): String =
      "2026-07-25 12:30:41|com.example.app|com.example.app|OPEN|" +
          "/storage/emulated/0/$relativePath|ret=10|errno=0|op=${operation.substringBefore(':')}|" +
          "op_filter=$operation"
}
