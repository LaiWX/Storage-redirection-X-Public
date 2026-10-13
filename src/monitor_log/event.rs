use crate::platform::paths;
use std::collections::BTreeMap;

pub(crate) const MAX_EVENT_BYTES: usize = 16 * 1024 - 32;

#[derive(Clone, Debug)]
pub(crate) struct MonitorEvent {
    pub fields: Vec<String>,
    pub extra: BTreeMap<String, String>,
    pub time_ms: i64,
    identity: (String, String, Option<u32>),
    category: String,
}

impl MonitorEvent {
    pub fn parse(line: &str) -> Option<Self> {
        if line.is_empty()
            || line.len() > MAX_EVENT_BYTES
            || line.contains(['\n', '\r', '\0', '\t'])
        {
            return None;
        }
        let parts: Vec<_> = line.split('|').collect();
        if parts.len() < 5 || parts[1].is_empty() || parts[4].is_empty() {
            return None;
        }
        let time_ms = timestamp_ms(parts[0])?;
        let mut extra = BTreeMap::new();
        for part in &parts[5..] {
            let (key, value) = part.split_once('=')?;
            if key.is_empty() {
                return None;
            }
            extra.insert(key.to_string(), value.to_string());
        }
        let mut event = Self {
            fields: parts[..5].iter().map(|v| v.to_string()).collect(),
            extra,
            time_ms,
            identity: (String::new(), String::new(), None),
            category: String::new(),
        };
        event.fields[4] = event.fields[4]
            .strip_prefix("file://")
            .unwrap_or(&event.fields[4])
            .to_string();
        if event.get("event_role").is_empty() {
            let role = if matches!(
                event.get("identify_method"),
                "daemon_inotify" | "media_provider_fallback"
            ) {
                "observation"
            } else {
                "request"
            };
            event.extra.insert("event_role".into(), role.into());
        }
        if !matches!(event.get("event_role"), "request" | "observation")
            || event.line().len() > MAX_EVENT_BYTES
        {
            return None;
        }
        // 匹配键只解析一次，避免每个输入与 1000 条历史比较时反复分配路径与操作名。
        let (package, path, user) = event.resolve_identity();
        event.identity = (package.to_string(), path, user);
        event.category = event.resolve_category();
        Some(event)
    }

    pub fn get(&self, key: &str) -> &str {
        self.extra.get(key).map(String::as_str).unwrap_or("")
    }
    pub fn operation(&self) -> String {
        let op = [self.get("op_filter"), self.get("op"), &self.fields[3]];
        op.into_iter()
            .find(|v| !v.is_empty())
            .unwrap_or("")
            .to_lowercase()
    }
    pub fn filter_operation(&self) -> String {
        if self.get("deny_reason") == "read_only_rule" {
            String::new()
        } else {
            self.operation()
        }
    }
    fn resolve_category(&self) -> String {
        let op = self.operation();
        if let Some((_, intent)) = op.rsplit_once(':') {
            return intent.to_string();
        }
        match op.as_str() {
            "fuse_create" | "fuse_monitor_create" | "create" | "inotify" => "create",
            "fuse_write" | "fuse_monitor_write" | "write" | "close_write" => "write",
            "mkdirat" => "mkdir",
            "mknodat" => "mknod",
            "unlinkat" | "delete" => "unlink",
            "renameat" | "renameat2" => "rename",
            _ => &op,
        }
        .to_string()
    }
    fn resolve_identity(&self) -> (&str, String, Option<u32>) {
        let package = [
            self.fields[2].as_str(),
            self.get("watch_package"),
            self.fields[1].as_str(),
        ]
        .into_iter()
        .find(|v| !v.is_empty() && *v != "-" && !v.contains(','))
        .unwrap_or("");
        let path = paths::normalize(if self.get("backend").is_empty() {
            &self.fields[4]
        } else {
            self.get("backend")
        });
        let user = path
            .strip_prefix(paths::STORAGE_EMULATED_PREFIX)
            .and_then(|v| v.split('/').next())
            .and_then(|v| v.parse().ok());
        (package, path, user)
    }
    pub fn category(&self) -> &str {
        &self.category
    }
    pub fn identity(&self) -> (&str, &str, Option<u32>) {
        (&self.identity.0, &self.identity.1, self.identity.2)
    }
    pub fn is_internal(&self) -> bool {
        if self.get("op") == "monitor_watch"
            || self.get("source") == "mount_prep"
            || self.get("identify_method") == "mount_prep"
            || (self.get("ret") == "-1" && self.get("errno") == "17")
        {
            return true;
        }
        let provider = matches!(
            self.get("source"),
            "media_store_pending_commit" | "media_provider_java" | "saf_provider"
        ) || matches!(
            self.get("identify_method"),
            "provider_open" | "media_provider_fallback" | "daemon_inotify"
        ) || self.fields[1].contains("providers.media");
        provider
            && [&self.fields[4], self.get("backend"), self.get("landing")]
                .into_iter()
                .any(|path| {
                    path.rsplit('/')
                        .next()
                        .and_then(|v| v.strip_prefix(".pending-"))
                        .and_then(|v| v.split_once('-'))
                        .is_some_and(|(number, name)| {
                            !number.is_empty()
                                && number.bytes().all(|v| v.is_ascii_digit())
                                && !name.is_empty()
                        })
                })
    }
    pub fn merge_kind(&self, other: &Self) -> Option<bool> {
        let a = self.identity();
        let b = other.identity();
        if a.0.is_empty() || a.0 != b.0 || a.2.is_none() || a.2 != b.2 || a.1 != b.1 {
            return None;
        }
        if self.result() != other.result() {
            return None;
        }
        let ca = self.get("correlation_id");
        let cb = other.get("correlation_id");
        let correlated = !ca.is_empty() && ca == cb;
        if !correlated
            && ((!ca.is_empty() && !cb.is_empty()) || (self.time_ms - other.time_ms).abs() > 1500)
        {
            return None;
        }
        let opposite_roles = self.get("event_role") != other.get("event_role");
        let same_category = self.category() == other.category();
        let create_write = matches!(
            (self.category(), other.category()),
            ("create", "write") | ("write", "create")
        );
        if opposite_roles && (same_category || (correlated && create_write)) {
            return Some(false);
        }
        let fuse = |e: &Self| {
            matches!(
                e.get("identify_method"),
                "fuse_redirect" | "fuse_monitor_only"
            )
        };
        if self.get("event_role") == "request"
            && other.get("event_role") == "request"
            && fuse(self)
            && fuse(other)
            && self.get("source") == other.get("source")
            && create_write
        {
            return Some(true);
        }
        // 相同关联 ID 的重复请求才可去重；普通独立请求保持独立。
        if correlated
            && self.get("event_role") == "request"
            && other.get("event_role") == "request"
            && same_category
        {
            return Some(false);
        }
        None
    }
    fn result(&self) -> (bool, &str) {
        let success = self
            .get("ret")
            .parse::<i32>()
            .map(|v| v >= 0)
            .unwrap_or(true);
        (success, if success { "" } else { self.get("errno") })
    }
    pub fn merge(&mut self, other: Self, create_write: bool) {
        let count = |e: &Self| {
            e.get("observation_count")
                .parse::<u64>()
                .unwrap_or(0)
                .max(u64::from(e.get("event_role") == "observation"))
        };
        let observations = count(self).saturating_add(count(&other));
        let observer = if !self.get("observer_package").is_empty() {
            self.get("observer_package").to_string()
        } else {
            other.get("observer_package").to_string()
        };
        if self.get("event_role") == "observation" || (create_write && self.category() == "write") {
            *self = other;
        }
        if create_write {
            self.extra
                .insert("combined_action".into(), "create_write".into());
        }
        if observations > 0 {
            self.extra
                .insert("observation_count".into(), observations.to_string());
        }
        if !observer.is_empty() {
            self.extra.insert("observer_package".into(), observer);
        }
    }
    pub fn line(&self) -> String {
        let mut line = self.fields.join("|");
        for (key, value) in &self.extra {
            line.push('|');
            line.push_str(key);
            line.push('=');
            line.push_str(value);
        }
        line
    }
}

fn timestamp_ms(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 19
        || [bytes[4], bytes[7], bytes[10], bytes[13], bytes[16]] != [b'-', b'-', b' ', b':', b':']
    {
        return None;
    }
    let number = |a, b| value.get(a..b)?.parse::<i64>().ok();
    let mut year = number(0, 4)?;
    let month = number(5, 7)?;
    let day = number(8, 10)?;
    let hour = number(11, 13)?;
    let minute = number(14, 16)?;
    let second = number(17, 19)?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return None;
    }
    year -= i64::from(month <= 2);
    let era = year.div_euclid(400);
    let y = year - era * 400;
    let m = month + if month > 2 { -3 } else { 9 };
    let days = era * 146097 + y * 365 + y / 4 - y / 100 + (153 * m + 2) / 5 + day - 1 - 719468;
    let milliseconds = if value.len() == 19 {
        0
    } else {
        let fraction = value.strip_prefix(&value[..19])?.strip_prefix('.')?;
        if fraction.is_empty()
            || fraction.len() > 3
            || !fraction.bytes().all(|v| v.is_ascii_digit())
        {
            return None;
        }
        fraction.parse::<i64>().ok()? * 10i64.pow(3 - fraction.len() as u32)
    };
    Some(((days * 24 + hour) * 60 * 60 + minute * 60 + second) * 1000 + milliseconds)
}
