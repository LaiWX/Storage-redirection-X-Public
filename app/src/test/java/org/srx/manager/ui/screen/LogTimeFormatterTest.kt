package org.srx.manager.ui.screen

import java.time.LocalDate
import java.time.ZonedDateTime
import org.junit.Assert.assertEquals
import org.junit.Test
import org.srx.manager.data.LogEntry

class LogTimeFormatterTest {
  private val today = LocalDate.of(2026, 10, 13)

  private fun entry(timestamp: String, timeText: String = "14:35") =
      LogEntry(
          timestamp,
          timeText,
          "com.app",
          "com.app",
          "com.app",
          "应用",
          "open",
          "读取",
          "/path",
          true,
      )

  @Test
  fun todayShowsTimeAndOtherDatesShowTheirDate() {
    assertEquals("14:35", formatLogEntryTime(entry("2026-10-13 14:35:12"), false, today))
    assertEquals("10-12 14:35", formatLogEntryTime(entry("2026-10-12 14:35:12"), false, today))
    assertEquals("10-14 14:35", formatLogEntryTime(entry("2026-10-14T14:35:12"), false, today))
    assertEquals("2025-12-31 14:35", formatLogEntryTime(entry("2025-12-31 14:35:12"), false, today))
  }

  @Test
  fun fullModeAlwaysShowsTheYear() {
    assertEquals("2026-10-13 14:35", formatLogEntryTime(entry("2026-10-13 14:35:12"), true, today))
    assertEquals("2026-10-12 14:35", formatLogEntryTime(entry("2026-10-12 14:35:12"), true, today))
  }

  @Test
  fun invalidTimesUseExistingTextOrPlaceholder() {
    assertEquals("14:35", formatLogEntryTime(entry("无效时间"), false, today))
    assertEquals("--:--", formatLogEntryTime(entry("", ""), false, today))
    assertEquals("14:35", formatLogEntryTime(entry("2026-02-30 14:35"), false, today))
  }

  @Test
  fun crossingMidnightChangesTheSameEntryToADatedTime() {
    val log = entry("2026-10-13 14:35:12")
    assertEquals("14:35", formatLogEntryTime(log, false, today))
    assertEquals("10-13 14:35", formatLogEntryTime(log, false, today.plusDays(1)))
    assertEquals("2026-10-13 14:35", formatLogEntryTime(log, false, LocalDate.of(2027, 1, 1)))
  }

  @Test
  fun midnightTimerHandlesLocalZoneAndDst() {
    assertEquals(
        1000L,
        nextLogMidnightDelay(ZonedDateTime.parse("2026-10-13T23:59:59+08:00[Asia/Shanghai]")),
    )
    assertEquals(
        23L * 60 * 60 * 1000,
        nextLogMidnightDelay(ZonedDateTime.parse("2026-03-08T00:00:00-05:00[America/New_York]")),
    )
  }
}
