package org.srx.manager.ui.screen

import java.time.Duration
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter
import java.time.format.DateTimeParseException
import java.time.format.ResolverStyle
import java.util.Locale
import org.srx.manager.data.LogEntry

private val LogFullTimeFormat =
    DateTimeFormatter.ofPattern("uuuu-MM-dd HH:mm", Locale.ROOT)
        .withResolverStyle(ResolverStyle.STRICT)
private val LogMonthDayTimeFormat = DateTimeFormatter.ofPattern("MM-dd HH:mm", Locale.ROOT)
private val LogTimeFormat = DateTimeFormatter.ofPattern("HH:mm", Locale.ROOT)

internal fun formatLogEntryTime(
    entry: LogEntry,
    showFullTime: Boolean,
    today: LocalDate,
): String {
  val time =
      try {
        LocalDateTime.parse(entry.timestamp.replace('T', ' ').take(16), LogFullTimeFormat)
      } catch (_: DateTimeParseException) {
        return entry.timeText.ifBlank { "--:--" }
      }
  val format =
      when {
        showFullTime || time.year != today.year -> LogFullTimeFormat
        time.toLocalDate() != today -> LogMonthDayTimeFormat
        else -> LogTimeFormat
      }
  return time.format(format)
}

internal fun nextLogMidnightDelay(now: ZonedDateTime): Long =
    Duration.between(now, now.toLocalDate().plusDays(1).atStartOfDay(now.zone))
        .toMillis()
        .coerceAtLeast(1L)
