package org.srx.manager.data

import java.io.IOException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class UpdateCheckerTest {
  private val checker = UpdateChecker("SRX-Manager/test")

  @Test
  fun forkVersionsKeepSemVerOrder() {
    val pairs =
        listOf(
            "1.2.60-ci.36.patch.0" to "1.2.60-ci.36.patch.1",
            "1.2.60-ci.36.patch.2" to "1.2.60-ci.36.patch.10",
            "1.2.60-ci.36.patch.10" to "1.2.60-ci.37.patch.10",
            "1.2.60-ci.37.patch.10" to "1.2.60-patch.10",
            "1.2.60-patch.10" to "1.2.61-ci.1.patch.10",
        )
    for ((old, new) in pairs) {
      val channel = if (new.contains("-ci.")) "beta" else "stable"
      val json = """{"schema":1,"$channel":{"version":"$new","tag":"v$new"}}"""
      val update = checker.checkManifest(json, "example/repo", old, UpdateChannel.All)
      requireNotNull(update)
      assertEquals(new, update.versionName)
      assertEquals(channel == "beta", update.prerelease)
      assertNull(checker.checkManifest(json, "example/repo", new, UpdateChannel.All))
      val older = """{"schema":1,"$channel":{"version":"$old","tag":"v$old"}}"""
      assertNull(checker.checkManifest(older, "example/repo", new, UpdateChannel.All))
    }
  }

  @Test
  fun forkManifestSupportsMirrorFallback() {
    assertEquals(
        listOf(
            "https://raw.githubusercontent.com/example/repo/SRX-R/fork-update.json",
            "https://cdn.jsdelivr.net/gh/example/repo@SRX-R/fork-update.json",
        ),
        updateManifestUrls("https://raw.githubusercontent.com/example/repo/SRX-R/fork-update.json"),
    )
  }

  @Test
  fun stableChannelReadsStableManifestEntry() {
    val update =
        checker.checkManifest(
            manifestJson = manifest,
            repository = "example/repo",
            currentVersionName = "1.2.56",
            channel = UpdateChannel.Stable,
        )

    requireNotNull(update)
    assertEquals("v1.2.57", update.tagName)
    assertEquals("1.2.57", update.versionName)
    assertEquals("Storage Redirect X v1.2.57", update.title)
    assertEquals("https://github.com/example/repo/releases/tag/v1.2.57", update.htmlUrl)
    assertEquals(UpdateChannel.Stable, update.channel)
    assertEquals(false, update.prerelease)
    assertEquals("## 模块更新\n- 修复模块。", update.releaseNotes)
  }

  @Test
  fun betaChannelReadsBetaManifestEntry() {
    val update =
        checker.checkManifest(
            manifestJson = manifest,
            repository = "example/repo",
            currentVersionName = "1.2.56",
            channel = UpdateChannel.Beta,
        )

    requireNotNull(update)
    assertEquals("ci-build-123", update.tagName)
    assertEquals("1.2.58-ci.123", update.versionName)
    assertEquals(UpdateChannel.Beta, update.channel)
    assertTrue(update.prerelease)
  }

  @Test
  fun allChannelChoosesHighestVersion() {
    val update =
        checker.checkManifest(
            manifestJson = manifest,
            repository = "example/repo",
            currentVersionName = "1.2.56",
            channel = UpdateChannel.All,
        )

    requireNotNull(update)
    assertEquals("ci-build-123", update.tagName)
    assertEquals("1.2.58-ci.123", update.versionName)
    assertEquals("1.2.58-ci.123.apk", update.downloadUrl)
  }

  @Test
  fun currentOrNewerVersionReturnsNoUpdate() {
    val update =
        checker.checkManifest(
            manifestJson = manifest,
            repository = "example/repo",
            currentVersionName = "1.2.58-ci.123",
            channel = UpdateChannel.All,
        )

    assertNull(update)
  }

  @Test
  fun rawGitHubManifestUsesJsDelivrAsFallback() = runBlocking {
    val requests = mutableListOf<String>()
    val fallbackChecker =
        UpdateChecker("SRX-Manager/test") { url ->
          requests += url
          if (url.contains("raw.githubusercontent.com")) throw IOException("HTTP 404")
          manifest
        }

    val update =
        fallbackChecker.check(
            manifestUrl = "https://raw.githubusercontent.com/example/repo/SRX-R/update.json",
            repository = "example/repo",
            currentVersionName = "1.2.56",
            channel = UpdateChannel.Stable,
        )

    requireNotNull(update)
    assertEquals("1.2.57", update.versionName)
    assertEquals(
        listOf(
            "https://raw.githubusercontent.com/example/repo/SRX-R/update.json",
            "https://cdn.jsdelivr.net/gh/example/repo@SRX-R/update.json",
        ),
        requests,
    )
  }

  @Test
  fun reportsFailureOnlyAfterBothManifestSourcesFail() = runBlocking {
    val requests = mutableListOf<String>()
    val fallbackChecker =
        UpdateChecker("SRX-Manager/test") { url ->
          requests += url
          throw IOException("HTTP 404")
        }

    try {
      fallbackChecker.check(
          manifestUrl = "https://raw.githubusercontent.com/example/repo/SRX-R/update.json",
          repository = "example/repo",
          currentVersionName = "1.2.56",
          channel = UpdateChannel.Stable,
      )
      fail("Expected both manifest sources to fail")
    } catch (error: IOException) {
      assertTrue(error.message.orEmpty().contains("首选源与备用镜像均不可用"))
      assertFalse(error.message.orEmpty().contains("请确认仓库分支已提交"))
    }
    assertEquals(2, requests.size)
  }

  @Test
  fun customManifestUrlIsNotRewritten() {
    assertEquals(
        listOf("https://updates.example.com/update.json"),
        updateManifestUrls("https://updates.example.com/update.json"),
    )
  }

  private companion object {
    val manifest =
        """
        {
          "schema": 1,
          "repository": "example/repo",
          "stable": {
            "version": "1.2.57",
            "tag": "v1.2.57",
            "title": "Storage Redirect X v1.2.57",
            "releaseNotes": "## 模块更新\n- 修复模块。"
          },
          "beta": {
            "version": "1.2.58-ci.123",
            "tag": "ci-build-123",
            "title": "CI Build 1.2.58-ci.123",
            "downloadUrl": "1.2.58-ci.123.apk"
          },
          "releases": []
        }
        """
            .trimIndent()
  }
}
