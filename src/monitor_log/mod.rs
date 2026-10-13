// 文件监视日志的最终记录处理与周期快照存储。
mod event;
mod store;

pub(crate) use event::MonitorEvent;
pub(crate) use store::MonitorJournal;

pub(crate) fn remaining_flush_ms(elapsed: std::time::Duration, interval_ms: i32) -> i32 {
    (interval_ms as u128).saturating_sub(elapsed.as_millis()) as i32
}
