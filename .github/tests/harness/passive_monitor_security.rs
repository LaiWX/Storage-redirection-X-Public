// quality-allow(lint-suppression): 独立测试桩只提供真实函数所需接口，允许未使用字段及 libc 类型别名。
#![allow(dead_code, unused_variables, non_camel_case_types)]

use std::ffi::{CStr, CString, c_char};
type c_int = i32;
const AT_FDCWD: i32 = -100;

#[derive(Clone, Copy)]
enum StorageBackendMode {
    Auto,
    Fuse,
    Namespace,
}
struct Request {
    mode: StorageBackendMode,
    monitor: bool,
    available: bool,
}
trait MountRequestFields {
    fn is_monitor_only(&self) -> bool;
    fn storage_backend_mode(&self) -> StorageBackendMode;
    fn package_name(&self) -> &str {
        "example.app"
    }
    fn uid(&self) -> i32 {
        10001
    }
    fn allowed_real_paths(&self) -> &[String] {
        &[]
    }
    fn excluded_real_paths(&self) -> &[String] {
        &[]
    }
    fn sandboxed_paths(&self) -> &[String] {
        &[]
    }
    fn read_only_paths(&self) -> &[String] {
        &[]
    }
    fn path_mappings(&self) -> &[String] {
        &[]
    }
    fn is_mapping_mode_only(&self) -> bool {
        false
    }
}
impl MountRequestFields for Request {
    fn is_monitor_only(&self) -> bool {
        self.monitor
    }
    fn storage_backend_mode(&self) -> StorageBackendMode {
        self.mode
    }
}
mod config {
    pub(crate) use super::StorageBackendMode;
}
mod platform {
    pub fn user_id_from_uid(uid: i32) -> i32 {
        uid / 100000
    }
    pub mod paths {
        pub fn storage_user_root_for_user(uid: i32) -> String {
            format!("/storage/emulated/{uid}")
        }
        pub fn contains_wildcards(path: &str) -> bool {
            path.contains('*')
        }
        pub fn normalize_syntax(path: &str) -> String {
            path.to_string()
        }
    }
}
use platform::paths;

use std::sync::atomic::{AtomicBool, Ordering};
static PROFILE_ENABLED: AtomicBool = AtomicBool::new(false);
static MONITOR_ENABLED: AtomicBool = AtomicBool::new(false);
struct SettingsHub;
impl SettingsHub {
    fn instance() -> &'static Self {
        &Self
    }
    fn is_file_monitor_enabled(&self) -> bool {
        MONITOR_ENABLED.load(Ordering::Relaxed)
    }
    fn is_user_profile_enabled_in_memory(&self, _: &str, _: i32) -> bool {
        PROFILE_ENABLED.load(Ordering::Relaxed)
    }
}
// __SQLITE_OWNER__

#[test]
fn sqlite_exception_depends_on_redirect_configuration_only() {
    for monitoring in [false, true] {
        MONITOR_ENABLED.store(monitoring, Ordering::Relaxed);
        for configured in [false, true] {
            PROFILE_ENABLED.store(configured, Ordering::Relaxed);
            assert_eq!(
                should_allow_private_owner_sqlite_for_owner("other.app", 0),
                configured
            );
        }
    }
}
fn scoped_mount_allowed_for_scope(_: &str, _: StorageBackendMode) -> bool {
    panic!("monitor-only queried FUSE capability")
}
fn scoped_mount_roots_for_hybrid_rules(
    _: i32,
    _: &[String],
    _: &[String],
    _: &[String],
    _: &[String],
    _: &[String],
    _: bool,
) -> Vec<String> {
    panic!("monitor-only planned hybrid mounts")
}
// __ROOTS__

struct MountRequest {
    is_monitor_only: bool,
    storage_backend_mode: StorageBackendMode,
    uid: i32,
}
type CompanionMountRequest = MountRequest;
// __CANDIDATES__

#[test]
fn monitor_cannot_attach_shared_host_but_redirect_auto_can() {
    for mode in [
        StorageBackendMode::Auto,
        StorageBackendMode::Fuse,
        StorageBackendMode::Namespace,
    ] {
        let mut request = MountRequest {
            is_monitor_only: true,
            storage_backend_mode: mode,
            uid: 10001,
        };
        let roots = vec!["/storage/emulated/0".to_string()];
        assert!(!daemon_candidate(&request, &roots));
        assert!(!companion_candidate(&request, &roots));
        request.is_monitor_only = false;
        assert_eq!(
            daemon_candidate(&request, &roots),
            matches!(mode, StorageBackendMode::Auto)
        );
        assert_eq!(
            companion_candidate(&request, &roots),
            matches!(mode, StorageBackendMode::Auto)
        );
    }
}

struct RuntimeFlow {
    is_file_monitor_ui: bool,
    should_monitor: bool,
    should_redirect: bool,
    package_name: String,
}
mod policy {
    pub fn is_system_writer_package(name: &str) -> bool {
        name == "media.provider"
    }
    pub fn is_saf_native_monitor_bridge_package(name: &str) -> bool {
        name == "saf.provider"
    }
}
// __POST__

struct InterceptHub;
impl InterceptHub {
    fn is_passive_monitor(&self) -> bool {
        true
    }
}
enum PreparedPath {
    Ready {
        path_for_decision: String,
        is_relative: bool,
    },
    Bypass,
}
unsafe fn prepare_relevant_path(
    _: &InterceptHub,
    _: &str,
    _: c_int,
    path: *const c_char,
    _: i32,
    _: bool,
) -> PreparedPath {
    // 路径观察会覆盖 errno，模拟实际路径解析的副作用。
    runtime::set_errno(99);
    let text = unsafe { CStr::from_ptr(path) }
        .to_str()
        .unwrap()
        .to_string();
    PreparedPath::Ready {
        is_relative: !text.starts_with('/'),
        path_for_decision: text,
    }
}
mod runtime {
    use std::cell::Cell;
    thread_local! { static ERRNO: Cell<i32> = const { Cell::new(0) }; }
    pub fn current_errno() -> i32 {
        ERRNO.with(Cell::get)
    }
    pub fn set_errno(value: i32) {
        ERRNO.with(|errno| errno.set(value));
    }
}
mod monitor {
    use super::*;
    pub struct OpenResultRecord<'a> {
        pub op_name: &'a str,
        pub flags: i32,
        pub pathname: &'a str,
        pub original_pathname: &'a str,
        pub is_mapping: bool,
        pub result: i32,
        pub error_no: i32,
        pub existed_before: bool,
    }
    pub fn record_open_result(_: &InterceptHub, record: OpenResultRecord<'_>) {
        assert!(!record.is_mapping);
        assert_eq!(record.pathname, record.original_pathname);
        assert_eq!(record.error_no, if record.result < 0 { 13 } else { 0 });
        runtime::set_errno(88);
    }
}
// __OPENS__

#[test]
fn monitor_roots_are_empty_for_every_backend_and_capability() {
    for mode in [
        StorageBackendMode::Auto,
        StorageBackendMode::Fuse,
        StorageBackendMode::Namespace,
    ] {
        for available in [false, true] {
            let request = Request {
                mode,
                available,
                monitor: true,
            };
            assert!(scoped_fuse_mount_roots_for_request(&request).is_empty());
        }
    }
}

#[test]
fn ordinary_monitor_installs_hook_but_manager_and_unmonitored_apps_do_not() {
    let mut flow = RuntimeFlow {
        is_file_monitor_ui: false,
        should_monitor: true,
        should_redirect: false,
        package_name: "example.app".into(),
    };
    assert!(should_install_process_plt_hook(&flow, false));
    flow.is_file_monitor_ui = true;
    assert!(!should_install_process_plt_hook(&flow, false));
    flow.is_file_monitor_ui = false;
    flow.should_monitor = false;
    assert!(!should_install_process_plt_hook(&flow, false));
    flow.should_monitor = true;
    flow.should_redirect = true;
    assert!(!should_install_process_plt_hook(&flow, false));
}

#[test]
fn passive_open_preserves_arguments_result_errno_and_calls_once() {
    let hub = InterceptHub;
    for text in [
        "relative.db-shm",
        "/storage/emulated/0/Android/data/other/private.db",
    ] {
        let path = CString::new(text).unwrap();
        for flags in [0, 1, 2, 0x40 | 2, 0x200 | 2] {
            for result in [-1, 7] {
                runtime::set_errno(71);
                let mut calls = 0;
                let actual =
                    handle_open_like(&hub, "open", path.as_ptr(), flags, false, |target| {
                        calls += 1;
                        assert_eq!(target, path.as_ptr());
                        assert_eq!(runtime::current_errno(), 71);
                        if result < 0 {
                            runtime::set_errno(13);
                        }
                        result
                    });
                assert_eq!(actual, result);
                assert_eq!(calls, 1);
                assert_eq!(runtime::current_errno(), if result < 0 { 13 } else { 71 });
                for dirfd in [AT_FDCWD, 41] {
                    runtime::set_errno(71);
                    let mut calls = 0;
                    let actual = handle_openat_like(
                        &hub,
                        "openat2",
                        dirfd,
                        path.as_ptr(),
                        flags,
                        |target_fd, target| {
                            calls += 1;
                            assert_eq!(target_fd, dirfd);
                            assert_eq!(target, path.as_ptr());
                            assert_eq!(runtime::current_errno(), 71);
                            if result < 0 {
                                runtime::set_errno(13);
                            }
                            result
                        },
                    );
                    assert_eq!(actual, result);
                    assert_eq!(calls, 1);
                    assert_eq!(runtime::current_errno(), if result < 0 { 13 } else { 71 });
                }
            }
        }
    }
}
