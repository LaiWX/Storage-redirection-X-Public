// 文件监视黑名单只决定后续事件是否写入日志。
use super::MonitorFilterConfig;
use crate::platform::lru_cache::LruCache;
use once_cell::sync::Lazy;
use std::cell::RefCell;
use std::sync::Mutex;

// 优化：监控路径匹配缓存
const MONITOR_PATH_CACHE_SIZE: usize = 128;

struct MonitorPathMatchCache {
    config_version: u64,
    entries: LruCache<String, bool>,
}

impl MonitorPathMatchCache {
    fn new() -> Self {
        Self {
            config_version: 0,
            entries: LruCache::new(MONITOR_PATH_CACHE_SIZE),
        }
    }

    fn prepare_version(&mut self, config_version: u64) -> bool {
        if self.config_version == config_version {
            return true;
        }
        if config_version < self.config_version {
            return false;
        }
        self.config_version = config_version;
        self.entries.clear();
        true
    }

    /// 接收借用的键，命中已有键时只原地更新值，不再产生任何分配。
    fn insert(&mut self, key: &str, value: bool) {
        self.entries.insert(key.to_string(), value);
    }

    fn get(&mut self, key: &str) -> Option<bool> {
        self.entries.get(&key.to_string())
    }
}

thread_local! {
    // 事件热路径复用键缓冲，避免每条监控记录都重新分配缓存键。
    static MONITOR_CACHE_KEY_BUF: RefCell<String> = RefCell::new(String::new());
}

static MONITOR_PATH_MATCH_CACHE: Lazy<Mutex<MonitorPathMatchCache>> =
    Lazy::new(|| Mutex::new(MonitorPathMatchCache::new()));
pub(super) fn should_filter_monitor_record_locked_for_version(
    filters: &MonitorFilterConfig,
    config_version: u64,
    path: &str,
    operation: &str,
) -> bool {
    let normalized_path = crate::platform::paths::normalize(path);
    // 调用方只传入内部规范化的小写操作名，避免在事件热路径重复分配。
    let op = operation;

    MONITOR_CACHE_KEY_BUF.with(|buf| {
        let mut cache_key = buf.borrow_mut();
        cache_key.clear();
        cache_key.push_str(&normalized_path);
        cache_key.push('|');
        cache_key.push_str(op);

        // 优化：为高频路径和操作组合提供快速缓存查找
        if let Ok(mut cache) = MONITOR_PATH_MATCH_CACHE.try_lock()
            && cache.prepare_version(config_version)
            && let Some(cached_result) = cache.get(cache_key.as_str())
        {
            return cached_result;
        }

        let path_matched = filters
            .excluded_paths
            .iter()
            .any(|rule| monitor_path_filter_matches(rule, &normalized_path));

        let op_matched = filters
            .excluded_operations
            .iter()
            .any(|rule| monitor_operation_filter_matches(rule, op));

        let result = path_matched || op_matched;

        // 优化：缓存匹配结果
        if let Ok(mut cache) = MONITOR_PATH_MATCH_CACHE.try_lock()
            && cache.prepare_version(config_version)
        {
            cache.insert(cache_key.as_str(), result);
        }

        result
    })
}

fn monitor_path_filter_matches(rule: &str, path: &str) -> bool {
    // 配置解析器将相对规则编译为跨用户的绝对模式；两侧都取用户相对路径。
    let Some(rule) = rule.strip_prefix("/storage/emulated/*/") else {
        return false;
    };
    if rule.is_empty() || path.is_empty() {
        return false;
    }
    let Some(rest) = path.strip_prefix(crate::platform::paths::STORAGE_EMULATED_PREFIX) else {
        return false;
    };
    let Some((user_id, path)) = rest.split_once('/') else {
        return false;
    };
    if user_id.is_empty() || !user_id.bytes().all(|ch| ch.is_ascii_digit()) || path.is_empty() {
        return false;
    }
    if crate::platform::paths::has_unsafe_segments(path) {
        return false;
    }
    if !has_monitor_path_wildcard(rule) {
        return crate::platform::paths::is_same_or_child(path, rule);
    }
    if crate::platform::paths::matches(rule, path, true) {
        return true;
    }
    if let Some(base) = rule.strip_suffix("/**") {
        return crate::platform::paths::matches(base, path, true);
    }
    false
}

fn has_monitor_path_wildcard(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?')
}

fn monitor_operation_filter_matches(rule: &str, operation: &str) -> bool {
    if rule.is_empty() || operation.is_empty() {
        return false;
    }
    crate::platform::paths::matches(rule, operation, false)
}

#[cfg(test)]
#[path = "../../tests/native-config-fixtures/src/monitor_filter_tests.rs"]
mod tests;
