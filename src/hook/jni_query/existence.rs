//! 仅在本次 Cursor 过滤期间复用真实目录中的普通文件条目。
//! 隔离规则仍逐行执行；未命中及不支持的情况保留原有系统调用。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_DIRECTORIES: usize = 128;
const MAX_ENTRIES: usize = 65_536;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const ENTRY_OVERHEAD: usize = 96;
const DIRECTORY_OVERHEAD: usize = 256;

struct Snapshot {
    regular: HashSet<Vec<u8>>,
    entries: usize,
    bytes: usize,
}

struct Builder {
    snapshot: Snapshot,
    entries_left: usize,
    bytes_left: usize,
}

impl Builder {
    fn new(entries_left: usize, bytes_left: usize) -> Self {
        Self {
            snapshot: Snapshot {
                regular: HashSet::new(),
                entries: 0,
                bytes: 0,
            },
            entries_left,
            bytes_left,
        }
    }

    fn add(&mut self, name: &[u8], kind: u8) -> Option<()> {
        if name.is_empty() || name.contains(&b'/') || name.contains(&0) {
            return None;
        }
        if name == b"." || name == b".." {
            return Some(());
        }
        let cost = name.len().checked_add(ENTRY_OVERHEAD)?;
        if self.entries_left == 0 || cost > self.bytes_left {
            return None;
        }
        self.entries_left -= 1;
        self.bytes_left -= cost;
        self.snapshot.entries += 1;
        self.snapshot.bytes += cost;
        // Linux/Android 的 DT_REG 为 8；符号链接和未知类型必须实际检查。
        if kind == 8 {
            self.snapshot.regular.try_reserve(1).ok()?;
            let mut owned = Vec::new();
            owned.try_reserve_exact(name.len()).ok()?;
            owned.extend_from_slice(name);
            self.snapshot.regular.insert(owned);
        }
        Some(())
    }

    fn dirents(&mut self, buffer: &[u8]) -> Option<()> {
        let mut offset = 0;
        while offset < buffer.len() {
            let record = buffer.get(offset..)?;
            let length = u16::from_ne_bytes(record.get(16..18)?.try_into().ok()?) as usize;
            if length < 20 || !length.is_multiple_of(8) {
                return None;
            }
            let record = record.get(..length)?;
            let names = &record[19..];
            let end = names.iter().position(|&byte| byte == 0)?;
            self.add(&names[..end], record[18])?;
            offset = offset.checked_add(length)?;
        }
        Some(())
    }
}

trait Backend {
    fn scan(&mut self, parent: &str, entries_left: usize, bytes_left: usize) -> Option<Snapshot>;
}

enum Directory {
    Unavailable,
    Ready(Snapshot),
}

struct Scope {
    token: u64,
    _caller_uid: i32,
    eligible: bool,
    directories: HashMap<String, Directory>,
    entries: usize,
    bytes: usize,
}

impl Scope {
    fn new(token: u64, caller_uid: i32, eligible: bool) -> Self {
        Self {
            token,
            _caller_uid: caller_uid,
            eligible,
            directories: HashMap::new(),
            entries: 0,
            bytes: 0,
        }
    }

    fn exists(
        &mut self,
        path: &str,
        backend: &mut impl Backend,
        fallback: impl FnOnce() -> bool,
    ) -> bool {
        if !self.eligible {
            return fallback();
        }
        let Some((parent, name)) = split_path(path) else {
            return fallback();
        };
        if !self.directories.contains_key(parent) {
            let cost = parent.len().saturating_add(DIRECTORY_OVERHEAD);
            if self.directories.len() >= MAX_DIRECTORIES || cost > MAX_BYTES - self.bytes {
                return fallback();
            }
            self.bytes += cost;
            let directory =
                match backend.scan(parent, MAX_ENTRIES - self.entries, MAX_BYTES - self.bytes) {
                    Some(snapshot) => {
                        self.entries += snapshot.entries;
                        self.bytes += snapshot.bytes;
                        Directory::Ready(snapshot)
                    }
                    None => Directory::Unavailable,
                };
            self.directories.insert(parent.to_owned(), directory);
        }
        if let Some(Directory::Ready(snapshot)) = self.directories.get(parent)
            && snapshot.regular.contains(name.as_bytes())
        {
            return true;
        }
        fallback()
    }
}

fn split_path(path: &str) -> Option<(&str, &str)> {
    if !path.starts_with('/') || path.contains('\0') {
        return None;
    }
    let (parent, name) = path.rsplit_once('/')?;
    if parent.is_empty()
        || name.is_empty()
        || path.split('/').any(|part| part == "." || part == "..")
    {
        return None;
    }
    Some((parent, name))
}

fn credentials_eligible(status: &str) -> bool {
    let mut uid = None;
    let mut gid = None;
    let mut capabilities = None;
    for line in status.lines() {
        if let Some(value) = line.strip_prefix("Uid:") {
            uid = equal_ids(value);
        }
        if let Some(value) = line.strip_prefix("Gid:") {
            gid = equal_ids(value);
        }
        if let Some(value) = line.strip_prefix("CapEff:") {
            capabilities = u64::from_str_radix(value.trim(), 16).ok();
        }
    }
    uid.is_some_and(|id| id != 0) && gid.is_some() && capabilities == Some(0)
}

fn equal_ids(value: &str) -> Option<u32> {
    let ids: Vec<u32> = value
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if ids.len() == 4 && ids.iter().all(|&id| id == ids[0]) {
        Some(ids[0])
    } else {
        None
    }
}

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
thread_local! {
    static SCOPES: RefCell<Vec<Scope>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn begin(caller_uid: i32, use_snapshot: bool) -> u64 {
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    // 单条 URI 仍压入独立作用域以屏蔽父查询，同时跳过凭据读取。
    let eligible = use_snapshot && platform::eligible();
    SCOPES.with(|scopes| {
        scopes
            .borrow_mut()
            .push(Scope::new(token, caller_uid, eligible))
    });
    token
}

pub(crate) fn end(token: u64) {
    SCOPES.with(|scopes| {
        let mut scopes = scopes.borrow_mut();
        if let Some(index) = scopes.iter().rposition(|scope| scope.token == token) {
            // Java 异常跨过嵌套作用域时，同时释放未正常结束的子查询。
            scopes.truncate(index);
        }
    });
}

pub(super) fn exists(path: &str, fallback: impl FnOnce() -> bool) -> bool {
    SCOPES.with(|scopes| {
        let Ok(mut scopes) = scopes.try_borrow_mut() else {
            return fallback();
        };
        match scopes.last_mut() {
            Some(scope) => scope.exists(path, &mut platform::RawBackend, fallback),
            None => fallback(),
        }
    })
}

#[cfg(all(
    any(target_os = "android", target_os = "linux"),
    target_pointer_width = "64"
))]
mod platform {
    use super::{Backend, Builder, Snapshot, credentials_eligible};
    use std::ffi::CString;

    // libc 的 Android arm64 模块未导出此常量；数值对应 NDK asm-generic/unistd.h。
    #[cfg(all(target_os = "android", target_arch = "aarch64"))]
    const FSTATFS_SYSCALL: libc::c_long = 44;
    #[cfg(not(all(target_os = "android", target_arch = "aarch64")))]
    const FSTATFS_SYSCALL: libc::c_long = libc::SYS_fstatfs;

    struct Fd(libc::c_int);
    impl Drop for Fd {
        fn drop(&mut self) {
            // SAFETY: 描述符由本模块打开且独占持有，释放后不再用于其他调用。
            unsafe {
                libc::syscall(libc::SYS_close, self.0);
            }
        }
    }

    fn open(path: &str, flags: libc::c_int) -> Option<Fd> {
        let path = CString::new(path).ok()?;
        // SAFETY: CString 在调用期间有效；只使用无需 mode 参数的打开标志。
        let fd =
            unsafe { libc::syscall(libc::SYS_openat, libc::AT_FDCWD, path.as_ptr(), flags, 0) };
        if fd < 0 {
            None
        } else {
            Some(Fd(fd as libc::c_int))
        }
    }

    fn interrupted() -> bool {
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
    }

    pub(super) fn eligible() -> bool {
        let Some(fd) = open("/proc/thread-self/status", libc::O_RDONLY | libc::O_CLOEXEC) else {
            return false;
        };
        let mut status = [0u8; 16 * 1024];
        let mut used = 0;
        loop {
            if used == status.len() {
                return false;
            }
            // SAFETY: used 小于缓冲区长度，可写范围与传入的长度一致。
            let count = unsafe {
                libc::syscall(
                    libc::SYS_read,
                    fd.0,
                    status[used..].as_mut_ptr(),
                    status.len() - used,
                )
            };
            if count < 0 {
                if interrupted() {
                    continue;
                }
                return false;
            }
            if count == 0 {
                break;
            }
            used += count as usize;
        }
        std::str::from_utf8(&status[..used]).is_ok_and(credentials_eligible)
    }

    pub(super) struct RawBackend;
    impl Backend for RawBackend {
        fn scan(
            &mut self,
            parent: &str,
            entries_left: usize,
            bytes_left: usize,
        ) -> Option<Snapshot> {
            let path = CString::new(parent).ok()?;
            // SAFETY: CString 在调用期间有效；只查询路径权限，不修改文件系统。
            if unsafe {
                libc::syscall(
                    libc::SYS_faccessat,
                    libc::AT_FDCWD,
                    path.as_ptr(),
                    libc::X_OK,
                )
            } != 0
            {
                return None;
            }
            let fd = open(parent, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC)?;
            let mut fs = std::mem::MaybeUninit::<libc::statfs>::uninit();
            // SAFETY: 仅启用 64 位平台，libc statfs 与 NDK/内核结构布局一致。
            if unsafe { libc::syscall(FSTATFS_SYSCALL, fd.0, fs.as_mut_ptr()) } != 0 {
                return None;
            }
            // SAFETY: 上方系统调用成功，内核已初始化 statfs 的字段。
            let kind = unsafe { fs.assume_init() }.f_type;
            if kind != 0xf2f5_2010 && kind != 0xef53 {
                return None;
            }
            let mut builder = Builder::new(entries_left, bytes_left);
            let mut buffer = [0u8; 32 * 1024];
            loop {
                // SAFETY: 描述符由 fd 持有，写入范围限于给定的缓冲区长度。
                let count = unsafe {
                    libc::syscall(
                        libc::SYS_getdents64,
                        fd.0,
                        buffer.as_mut_ptr(),
                        buffer.len(),
                    )
                };
                if count < 0 {
                    if interrupted() {
                        continue;
                    }
                    return None;
                }
                if count == 0 {
                    return Some(builder.snapshot);
                }
                builder.dirents(buffer.get(..count as usize)?)?;
            }
        }
    }
}

#[cfg(not(all(
    any(target_os = "android", target_os = "linux"),
    target_pointer_width = "64"
)))]
mod platform {
    use super::{Backend, Snapshot};
    pub(super) fn eligible() -> bool {
        false
    }
    pub(super) struct RawBackend;
    impl Backend for RawBackend {
        fn scan(&mut self, _: &str, _: usize, _: usize) -> Option<Snapshot> {
            None
        }
    }
}
