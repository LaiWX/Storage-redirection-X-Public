package org.srx.manager.ui.screen

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import org.srx.manager.data.LogEntry
import org.srx.manager.data.parseMonitorLogEntries

class LogListItemsTest {
  private fun line(flags: String) =
      "2026-10-13 14:33:27|com.android.providers.media.module|-|OPEN|" +
          "/storage/emulated/0/srx-icon-after.mp4|errno=0|event_role=request|" +
          "flags=$flags|identify_method=unknown|identify_reliability=none|" +
          "op=open|op_filter=open:create|ret=195"

  private fun entry(): LogEntry = parseMonitorLogEntries(line("0x20042")).single()

  @Test
  fun differentFlagsKeepEqualEntriesWithUniqueKeys() {
    val entries = parseMonitorLogEntries(line("0x20042") + "\n" + line("0x20242"))
    assertEquals(2, entries.size)
    assertEquals(entries[0], entries[1])
    val items = buildLogListItems(entries)
    assertEquals(entries, items.map { it.entry })
    assertNotEquals(items[0].key, items[1].key)
  }

  @Test
  fun identicalRecordsAndReferencesStaySeparate() {
    val entry = entry()
    val entries = listOf(entry, entry, entry.copy())
    val items = buildLogListItems(entries)
    assertEquals(entries, items.map { it.entry })
    assertEquals(3, items.map { it.key }.toSet().size)
  }

  @Test
  fun matchingFingerprintsKeepDistinctMetadata() {
    val entries = listOf(entry().copy(source = "new"), entry().copy(source = "old"))
    val items = buildLogListItems(entries)
    assertEquals(entries, items.map { it.entry })
    assertNotEquals(items[0].key, items[1].key)
  }

  @Test
  fun prependingOrdinaryRecordKeepsExistingKeys() {
    val entries = listOf(entry(), entry().copy())
    val before = buildLogListItems(entries)
    val after = buildLogListItems(listOf(entry().copy(timestamp = "2026-10-13 14:34:00")) + entries)
    assertEquals(before, after.drop(1))
  }

  @Test
  fun prependingDuplicateKeepsExistingKeys() {
    val entries = listOf(entry(), entry().copy())
    val before = buildLogListItems(entries)
    val after = buildLogListItems(listOf(entry().copy()) + entries)
    assertEquals(before, after.drop(1))
    assertEquals(3, after.map { it.key }.toSet().size)
  }

  @Test
  fun searchKeepsOriginalKeysAndDuplicateRecords() {
    val items = buildLogListItems(listOf(entry().copy(action = "new-action"), entry()))
    assertEquals(listOf(items[0]), filterLogListItems(items, " NEW-ACTION ", emptyMap()))
    assertEquals(items, filterLogListItems(items, "srx-icon-after", emptyMap()))
    assertEquals(items, filterLogListItems(items, " ", emptyMap()))
    assertEquals(emptyList<LogListItem>(), filterLogListItems(items, "missing", emptyMap()))
  }

  @Test
  fun repeatedRefreshKeepsKeysAndEmptyListIsValid() {
    val entries = listOf(entry(), entry().copy())
    assertEquals(buildLogListItems(entries), buildLogListItems(entries.map { it.copy() }))
    assertEquals(emptyList<LogListItem>(), buildLogListItems(emptyList()))
  }
}
