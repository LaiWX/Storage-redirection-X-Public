use super::MonitorEvent;
use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const RECORD_LIMIT: usize = 1000;
const STORAGE_FORMAT: &str = "final-records-v1\n";

pub(crate) struct MonitorJournal {
    path: PathBuf,
    records: VecDeque<MonitorEvent>,
    dirty: bool,
    // 淘汰关联 ID 保持有界，避免迟到观察重新显示已淘汰的请求。
    retired: VecDeque<String>,
}

impl MonitorJournal {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let marker = path.with_extension("records-format");
        let mut journal = Self {
            path,
            records: VecDeque::new(),
            dirty: false,
            retired: VecDeque::new(),
        };
        if fs::read_to_string(&marker).ok().as_deref() != Some(STORAGE_FORMAT) {
            journal.dirty = true;
            journal.flush()?;
            remove_backups(&journal.path)?;
            atomic_write(&marker, STORAGE_FORMAT.as_bytes())?;
        } else {
            match fs::read_to_string(&journal.path) {
                Ok(content) => {
                    for line in content.lines() {
                        if let Some(event) = MonitorEvent::parse(line) {
                            journal.records.push_back(event);
                            if journal.records.len() > RECORD_LIMIT {
                                journal.records.pop_front();
                                journal.dirty = true;
                            }
                        } else {
                            journal.dirty = true;
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    journal.dirty = true;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(journal)
    }
    pub fn ingest(&mut self, event: MonitorEvent) {
        if event.is_internal() {
            return;
        }
        let correlation = event.get("correlation_id");
        if !correlation.is_empty() && self.retired.iter().any(|v| v == correlation) {
            return;
        }
        let candidates: Vec<_> = self
            .records
            .iter()
            .enumerate()
            .filter_map(|(i, existing)| existing.merge_kind(&event).map(|kind| (i, kind)))
            .collect();
        let explicit: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|(i, _)| {
                !correlation.is_empty() && self.records[*i].get("correlation_id") == correlation
            })
            .collect();
        let selected = if explicit.len() == 1 {
            explicit.first().copied()
        } else if explicit.is_empty() && candidates.len() == 1 {
            candidates.first().copied()
        } else {
            None
        };
        if let Some((i, kind)) = selected {
            let mut merged = self.records[i].clone();
            merged.merge(event.clone(), kind);
            if MonitorEvent::parse(&merged.line()).is_some() {
                self.records[i] = merged;
            } else {
                // 关联字段扩展超过单行协议上限时，保留两条合法记录。
                self.append(event);
            }
        } else {
            self.append(event);
        }
        self.dirty = true;
    }
    fn append(&mut self, event: MonitorEvent) {
        self.records.push_back(event);
        if self.records.len() > RECORD_LIMIT
            && let Some(old) = self.records.pop_front()
        {
            if !old.get("correlation_id").is_empty() {
                self.retired
                    .push_back(old.get("correlation_id").to_string());
                if self.retired.len() > RECORD_LIMIT {
                    self.retired.pop_front();
                }
            }
        }
    }
    pub fn flush(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let mut content = String::new();
        for event in &self.records {
            content.push_str(&event.line());
            content.push('\n');
        }
        atomic_write(&self.path, content.as_bytes())?;
        self.dirty = false;
        Ok(())
    }
    pub fn clear(&mut self) -> io::Result<()> {
        // 只有空快照已保存才清内存；失败保留历史与待写状态。
        atomic_write(&self.path, b"")?;
        self.records.clear();
        self.retired.clear();
        self.dirty = false;
        Ok(())
    }
}

fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    let temp = PathBuf::from(format!("{}.tmp", path.display()));
    fs::write(&temp, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if path.extension().is_some_and(|v| v == "records-format") {
            0o644
        } else {
            0o666
        };
        fs::set_permissions(&temp, fs::Permissions::from_mode(mode))?;
    }
    fs::rename(temp, path)
}

fn remove_backups(path: &Path) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let prefix = format!(
        "{}.",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(suffix) = name.strip_prefix(&prefix)
            && !suffix.is_empty()
            && suffix.bytes().all(|b| b.is_ascii_digit())
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
