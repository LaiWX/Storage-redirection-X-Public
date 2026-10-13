use super::*;

fn parsed_filters(json: &str) -> MonitorFilterConfig {
    let mut state = super::super::SettingsState::new();
    assert!(super::super::ingest::parse_monitor_filter_config(
        &mut state, json
    ));
    state.monitor_filters
}

fn path_matches(rule: &str, path: &str) -> bool {
    let json = serde_json::json!({"excluded_paths": [rule]}).to_string();
    let filters = parsed_filters(&json);
    assert_eq!(filters.excluded_paths.len(), 1);
    monitor_path_filter_matches(
        &filters.excluded_paths[0],
        &crate::platform::paths::normalize(path),
    )
}

#[test]
fn shared_storage_aliases_and_users_match_relative_rules() {
    for path in [
        "/storage/emulated/0/Download/a.txt",
        "/storage/emulated/10/Download/a.txt",
        "/sdcard/Download/a.txt",
        "/storage/self/primary/Download/a.txt",
        "/data/media/10/Download/a.txt",
        "/mnt/runtime/write/emulated/10/Download/a.txt",
    ] {
        assert!(path_matches("Download", path), "{path}");
    }
    assert!(path_matches("download", "/storage/emulated/0/Download"));
    for path in [
        "/storage/emulated/0/Downloads/a.txt",
        "/storage/emulated/0/Documents/Download/a.txt",
        "/data/user/0/pkg/Download/a.txt",
        "/storage/emulated/invalid/Download/a.txt",
        "/storage/emulated/0/Download/../a.txt",
        "/storage/emulated/0",
    ] {
        assert!(!path_matches("Download", path), "{path}");
    }
}

#[test]
fn wildcard_rules_match_segments_and_subtrees() {
    assert!(path_matches(
        "Download/al**a/file.?xt",
        "/sdcard/Download/alpha/file.txt"
    ));
    assert!(path_matches(
        "Download/a*",
        "/sdcard/Download/Alpha/nested/file.txt"
    ));
    assert!(!path_matches(
        "Download/a*/file.txt",
        "/sdcard/Download/alpha/nested/file.txt"
    ));
    assert!(path_matches("Android/data/**", "/sdcard/Android/data"));
    assert!(path_matches(
        "Android/data/**",
        "/sdcard/Android/data/pkg/cache/file"
    ));
}

#[test]
fn operations_intents_and_paths_are_alternatives() {
    let filters = parsed_filters(
        r#"{"excluded_paths":["Download","Android/data"],"excluded_operations":["open*:read","*:create"]}"#,
    );
    // 单一版本使用同一配置，避免全局缓存污染其他测试。
    let matches = |path, op| should_filter_monitor_record_locked_for_version(&filters, 1, path, op);
    assert!(matches("/sdcard/Download/a.txt", "mkdir"));
    assert!(matches("/sdcard/Android/data/pkg/cache/a.txt", "mkdir"));
    assert!(matches("/sdcard/Documents/a.txt", "openat:read"));
    assert!(matches("/sdcard/Documents/a.txt", "provider_open:create"));
    assert!(!matches("/sdcard/Documents/a.txt", "open:write"));
    assert!(!matches("/sdcard/Documents/a.txt", "mkdir"));
    for intent in ["read", "write", "create"] {
        assert!(monitor_operation_filter_matches(
            &format!("*:{intent}"),
            &format!("openat:{intent}")
        ));
    }
    assert!(!monitor_operation_filter_matches("*:write", "open:create"));
    assert!(matches("/sdcard/Download/a.txt", ""));
    assert!(!matches("/sdcard/Documents/a.txt", ""));

    let cleared = parsed_filters(r#"{"excluded_paths":[],"excluded_operations":[]}"#);
    assert!(!should_filter_monitor_record_locked_for_version(
        &cleared,
        2,
        "/sdcard/Download/a.txt",
        "mkdir"
    ));
    assert!(!should_filter_monitor_record_locked_for_version(
        &cleared,
        2,
        "/sdcard/Documents/a.txt",
        "openat:read"
    ));
    assert!(!should_filter_monitor_record_locked_for_version(
        &cleared,
        2,
        "/sdcard/Android/data/pkg/cache/a.txt",
        "mkdir"
    ));
    assert!(should_filter_monitor_record_locked_for_version(
        &filters,
        3,
        "/sdcard/Download/a.txt",
        "mkdir"
    ));
}

#[test]
fn newer_configuration_invalidates_cached_decisions() {
    let mut cache = MonitorPathMatchCache::new();
    assert!(cache.prepare_version(1));
    cache.insert("path|open:read", true);
    assert_eq!(cache.get("path|open:read"), Some(true));
    assert!(cache.prepare_version(2));
    assert_eq!(cache.get("path|open:read"), None);
    cache.insert("path|open:read", false);
    assert!(cache.prepare_version(2));
    assert_eq!(cache.get("path|open:read"), Some(false));
    assert!(!cache.prepare_version(1));
}

#[test]
fn android_data_config_and_compiled_default_match() {
    let filters = parsed_filters(r#"{"excluded_paths":["Android/data"]}"#);
    assert_eq!(filters.excluded_paths, ["/storage/emulated/*/Android/data"]);
    // 与生产默认规则同形的已编译规则也必须生效。
    let default_rule = "/storage/emulated/*/Android/data";
    for path in [
        "/storage/emulated/0/Android/data",
        "/storage/emulated/10/Android/data/pkg/cache/file",
        "/sdcard/Android/data/pkg/file",
        "/storage/self/primary/Android/data/pkg/file",
        "/data/media/10/Android/data/pkg/file",
        "/mnt/runtime/write/emulated/10/Android/data/pkg/file",
        "/storage/emulated/0/android/DATA/pkg/file",
    ] {
        assert!(path_matches("Android/data", path), "{path}");
        assert!(
            monitor_path_filter_matches(default_rule, &crate::platform::paths::normalize(path),),
            "{path}"
        );
    }
    for path in [
        "/sdcard/Android/database/pkg/file",
        "/sdcard/Android/data_backup/pkg/file",
        "/sdcard/Documents/Android/data/pkg/file",
        "/data/user/0/pkg/Android/data/file",
    ] {
        assert!(!path_matches("Android/data", path), "{path}");
    }
}
