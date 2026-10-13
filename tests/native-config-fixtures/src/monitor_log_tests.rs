use crate::monitor_log::{MonitorEvent, MonitorJournal};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let base = std::env::temp_dir();
        let path = base.join(format!(
            "srx-monitor-test-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(path.starts_with(&base));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> PathBuf {
        self.0.join("file_monitor.log")
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        assert!(self.0.starts_with(std::env::temp_dir()));
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn event(time: &str, user: u32, pkg: &str, op: &str, role: &str, extra: &str) -> MonitorEvent {
    MonitorEvent::parse(&format!("2026-10-13 10:00:{time}|{pkg}|{pkg}|OPEN|/storage/emulated/{user}/Download/a.txt|ret=0|errno=0|op_filter={op}|event_role={role}{extra}")).unwrap()
}
fn content(journal: &mut MonitorJournal, path: &PathBuf) -> String {
    journal.flush().unwrap();
    fs::read_to_string(path).unwrap()
}
#[test]
fn initialization_clears_legacy_once_and_restart_keeps_records() {
    let dir = TestDir::new();
    let path = dir.path();
    fs::write(&path, "old log").unwrap();
    fs::write(path.with_extension("log.1"), "backup").unwrap();
    let mut journal = MonitorJournal::open(&path).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "");
    assert!(!path.with_extension("log.1").exists());
    journal.ingest(event("00", 0, "com.a", "open:write", "request", ""));
    let expected = content(&mut journal, &path);
    drop(journal);
    let mut restarted = MonitorJournal::open(&path).unwrap();
    assert_eq!(content(&mut restarted, &path), expected);
}
#[test]
fn periodic_snapshot_does_not_flush_on_line_count_and_keeps_latest_thousand() {
    let dir = TestDir::new();
    let path = dir.path();
    let mut journal = MonitorJournal::open(&path).unwrap();
    for i in 0..1001 {
        journal.ingest(event(
            "00",
            0,
            "com.a",
            "open:write",
            "request",
            &format!("|sequence={i}"),
        ));
        if i == 63 || i == 64 {
            assert_eq!(fs::read_to_string(&path).unwrap(), "");
        }
    }
    let text = content(&mut journal, &path);
    assert_eq!(text.lines().count(), 1000);
    assert!(!text.contains("|sequence=0\n"));
    assert!(text.contains("|sequence=1000\n"));
    assert!(!path.with_extension("log.1").exists());
}
#[test]
fn correlation_merges_across_flushes_and_in_reverse_order() {
    for reverse in [false, true] {
        let dir = TestDir::new();
        let path = dir.path();
        let mut journal = MonitorJournal::open(&path).unwrap();
        let request = event(
            "00",
            0,
            "com.a",
            "open:create",
            "request",
            "|correlation_id=c1",
        );
        let observation = event(
            "03",
            0,
            "com.a",
            "inotify",
            "observation",
            "|correlation_id=c1|observer_package=srx_daemon",
        );
        let (first, second) = if reverse {
            (observation, request)
        } else {
            (request, observation)
        };
        journal.ingest(first);
        journal.flush().unwrap();
        journal.ingest(second);
        let text = content(&mut journal, &path);
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("event_role=request"));
        assert!(text.contains("observation_count=1"));
    }
}
#[test]
fn independent_requests_and_ambiguous_observations_stay_separate() {
    let dir = TestDir::new();
    let path = dir.path();
    let mut journal = MonitorJournal::open(&path).unwrap();
    journal.ingest(event("00", 0, "com.a", "open:create", "request", ""));
    journal.ingest(event("00", 0, "com.a", "open:create", "request", ""));
    journal.ingest(event("01", 0, "com.a", "inotify", "observation", ""));
    assert_eq!(content(&mut journal, &path).lines().count(), 3);
}
#[test]
fn different_users_apps_results_and_expired_windows_do_not_merge() {
    let dir = TestDir::new();
    let path = dir.path();
    let mut journal = MonitorJournal::open(&path).unwrap();
    journal.ingest(event(
        "00",
        0,
        "com.a",
        "open:create",
        "request",
        "|correlation_id=c1",
    ));
    journal.ingest(event(
        "00",
        10,
        "com.a",
        "inotify",
        "observation",
        "|correlation_id=c1",
    ));
    journal.ingest(event(
        "00",
        0,
        "com.b",
        "inotify",
        "observation",
        "|correlation_id=c1",
    ));
    journal.ingest(event("03", 0, "com.a", "inotify", "observation", ""));
    let mut failed = event(
        "00",
        0,
        "com.a",
        "inotify",
        "observation",
        "|correlation_id=c1",
    );
    failed.extra.insert("ret".into(), "-1".into());
    failed.extra.insert("errno".into(), "13".into());
    journal.ingest(failed);
    assert_eq!(content(&mut journal, &path).lines().count(), 5);
}
#[test]
fn unique_short_window_and_fuse_create_write_pair_merge() {
    let dir = TestDir::new();
    let path = dir.path();
    let mut journal = MonitorJournal::open(&path).unwrap();
    journal.ingest(event("00.000", 0, "com.a", "open:create", "request", ""));
    journal.ingest(event(
        "01.500",
        0,
        "com.a",
        "inotify",
        "observation",
        "|observer_package=srx_daemon",
    ));
    assert_eq!(content(&mut journal, &path).lines().count(), 1);
    journal.clear().unwrap();
    journal.ingest(event(
        "00",
        0,
        "com.a",
        "fuse_write",
        "request",
        "|identify_method=fuse_redirect|source=fuse_redirect",
    ));
    journal.ingest(event(
        "01",
        0,
        "com.a",
        "fuse_create",
        "request",
        "|identify_method=fuse_redirect|source=fuse_redirect",
    ));
    assert!(content(&mut journal, &path).contains("combined_action=create_write"));
}
#[test]
fn late_observation_does_not_resurrect_evicted_request() {
    let dir = TestDir::new();
    let path = dir.path();
    let mut journal = MonitorJournal::open(&path).unwrap();
    journal.ingest(event(
        "00",
        0,
        "com.a",
        "open:create",
        "request",
        "|correlation_id=old",
    ));
    for i in 0..1000 {
        journal.ingest(event(
            "00",
            0,
            "com.a",
            "open:write",
            "request",
            &format!("|sequence={i}"),
        ));
    }
    journal.ingest(event(
        "00",
        0,
        "com.a",
        "inotify",
        "observation",
        "|correlation_id=old",
    ));
    let text = content(&mut journal, &path);
    assert_eq!(text.lines().count(), 1000);
    assert!(!text.contains("correlation_id=old"));
}
#[test]
fn failed_snapshot_and_clear_preserve_old_file_and_can_retry() {
    let dir = TestDir::new();
    let path = dir.path();
    let mut journal = MonitorJournal::open(&path).unwrap();
    journal.ingest(event("00", 0, "com.a", "open:read", "request", ""));
    let old = content(&mut journal, &path);
    let temp = PathBuf::from(format!("{}.tmp", path.display()));
    fs::create_dir(&temp).unwrap();
    journal.ingest(event("01", 0, "com.a", "open:write", "request", ""));
    assert!(journal.flush().is_err());
    assert!(journal.clear().is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), old);
    fs::remove_dir(&temp).unwrap();
    assert_eq!(content(&mut journal, &path).lines().count(), 2);
    journal.clear().unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "");
}
#[test]
fn only_known_internal_pending_records_are_removed() {
    let mut ordinary = event("00", 0, "com.a", "open:create", "request", "");
    ordinary.fields[4] = "/storage/emulated/0/Download/.pending-123-a.txt".into();
    assert!(!ordinary.is_internal());
    let mut internal = ordinary.clone();
    internal
        .extra
        .insert("source".into(), "media_provider_java".into());
    assert!(internal.is_internal());
    internal.fields[4] = "/storage/emulated/0/Download/a.txt".into();
    internal
        .extra
        .insert("from".into(), ordinary.fields[4].clone());
    internal
        .extra
        .insert("source".into(), "media_store_pending_commit".into());
    assert!(!internal.is_internal());
    assert!(MonitorEvent::parse("malformed").is_none());
    assert!(MonitorEvent::parse(&format!("{}\n", ordinary.line())).is_none());
}

#[test]
fn flush_deadline_is_not_reset_by_continuous_incoming_records() {
    use crate::monitor_log::remaining_flush_ms;
    use std::time::Duration;
    for elapsed in [0, 64, 1000, 1999] {
        assert_eq!(
            remaining_flush_ms(Duration::from_millis(elapsed), 2000),
            2000 - elapsed as i32
        );
    }
    assert_eq!(remaining_flush_ms(Duration::from_millis(2000), 2000), 0);
    assert_eq!(remaining_flush_ms(Duration::from_millis(2500), 2000), 0);
}

#[test]
fn correlation_deduplicates_requests_but_keeps_independent_operations() {
    let dir = TestDir::new();
    let path = dir.path();
    let mut journal = MonitorJournal::open(&path).unwrap();
    let request = event(
        "00",
        0,
        "com.a",
        "open:create",
        "request",
        "|correlation_id=c1",
    );
    journal.ingest(request.clone());
    journal.ingest(request);
    assert_eq!(content(&mut journal, &path).lines().count(), 1);
    journal.ingest(event(
        "00",
        0,
        "com.a",
        "open:create",
        "request",
        "|correlation_id=c2",
    ));
    assert_eq!(content(&mut journal, &path).lines().count(), 2);
}
