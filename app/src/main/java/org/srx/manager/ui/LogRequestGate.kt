package org.srx.manager.ui

/** 在主线程串行调用，使旧刷新及清空前的应用信息查询无法回填日志状态。 */
internal class LogRequestGate {
  private var generation = 0L
  private var clearing = false

  fun beginRefresh(): Long? {
    if (clearing) return null
    return ++generation
  }

  fun beginClear(): Long? {
    if (clearing) return null
    clearing = true
    return ++generation
  }

  fun isCurrent(request: Long): Boolean = !clearing && request == generation

  fun finishClear(request: Long): Boolean {
    if (!clearing || request != generation) return false
    clearing = false
    return true
  }
}
