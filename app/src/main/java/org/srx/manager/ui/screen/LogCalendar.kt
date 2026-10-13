package org.srx.manager.ui.screen

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import java.time.LocalDate
import java.time.ZonedDateTime
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive

/** 每个日志列表共用一份本地日期，恢复前台、跨午夜或系统时钟变化时更新。 */
@Composable
internal fun rememberLogToday(): LocalDate {
  val context = LocalContext.current.applicationContext
  val lifecycle = LocalLifecycleOwner.current.lifecycle
  var today by remember { mutableStateOf(LocalDate.now()) }
  var clockRevision by remember { mutableLongStateOf(0L) }
  DisposableEffect(context) {
    val receiver =
        object : BroadcastReceiver() {
          override fun onReceive(context: Context?, intent: Intent?) {
            today = LocalDate.now()
            clockRevision++
          }
        }
    val filter =
        IntentFilter().apply {
          addAction(Intent.ACTION_DATE_CHANGED)
          addAction(Intent.ACTION_TIME_CHANGED)
          addAction(Intent.ACTION_TIMEZONE_CHANGED)
        }
    ContextCompat.registerReceiver(context, receiver, filter, ContextCompat.RECEIVER_NOT_EXPORTED)
    onDispose { context.unregisterReceiver(receiver) }
  }
  LaunchedEffect(lifecycle, clockRevision) {
    lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
      while (isActive) {
        val now = ZonedDateTime.now()
        today = now.toLocalDate()
        delay(nextLogMidnightDelay(now))
      }
    }
  }
  return today
}
