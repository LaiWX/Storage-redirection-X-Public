package org.srx.manager.ui

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LogRequestGateTest {
  @Test
  fun lateRefreshCannotReplaceNewerSnapshot() {
    val gate = LogRequestGate()
    val old = gate.beginRefresh()!!
    val current = gate.beginRefresh()!!
    assertFalse(gate.isCurrent(old))
    assertTrue(gate.isCurrent(current))
  }

  @Test
  fun clearInvalidatesRefreshAndPackageLookups() {
    val gate = LogRequestGate()
    val refresh = gate.beginRefresh()!!
    val clear = gate.beginClear()!!
    assertFalse(gate.isCurrent(refresh))
    assertNull(gate.beginRefresh())
    assertNull(gate.beginClear())
    assertTrue(gate.finishClear(clear))
    assertFalse(gate.isCurrent(refresh))
    assertNotNull(gate.beginRefresh())
  }

  @Test
  fun failedClearStillRejectsOldRefresh() {
    val gate = LogRequestGate()
    val refresh = gate.beginRefresh()!!
    val clear = gate.beginClear()!!
    assertFalse(gate.finishClear(refresh))
    assertTrue(gate.finishClear(clear))
    assertFalse(gate.isCurrent(refresh))
    assertTrue(gate.isCurrent(gate.beginRefresh()!!))
  }
}
