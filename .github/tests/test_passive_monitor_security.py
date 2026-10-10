"""Security guards plus executable tests of the actual Rust decision paths."""

import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(path):
    return (ROOT / path).read_text(encoding="utf-8")


def function(source, name):
    start = source.index("fn " + name)
    brace = source.index("{", start)
    depth = 0
    for i in range(brace, len(source)):
        depth += (source[i] == "{") - (source[i] == "}")
        if depth == 0:
            return source[start : i + 1]
    raise AssertionError(name)


class PassiveMonitorSecurityTest(unittest.TestCase):
    def test_mount_entries_reject_before_side_effects(self):
        entries = [
            ("src/daemon_mount.rs", "execute_mount_request", "mark_state"),
            ("src/daemon_mount.rs", "try_fast_update_shared_host", "let "),
            ("src/daemon_mount.rs", "pre_register_host_policy", "wait_for_host_session"),
            ("src/lifecycle/companion_mount.rs", "execute_companion_mount_request", "mark_state"),
            ("src/fuse_host_control.rs", "register_app_policy", "HOST_CONTROL_FD.load"),
            ("src/fuse_redirect/config.rs", "mount_blocking_with_ready", "create_directory"),
            ("src/fuse_redirect/config.rs", "mount_host_fuse", "create_directory"),
            ("src/fuse_redirect/policy.rs", "register", "registered_mount_namespace"),
            ("src/fuse_redirect/policy.rs", "new", "real_backend_root_for_config"),
        ]
        for path, name, effect in entries:
            with self.subTest(path=path, name=name):
                source = read(path)
                if path == "src/fuse_redirect/policy.rs" and name == "new":
                    source = source[source.index("impl RedirectPolicy"):]
                body = function(source, name)
                guard = body.index(".is_monitor_only")
                self.assertLess(guard, body.index(effect))
                self.assertIn("return ", body[guard : body.index(effect)])
        daemon = read("src/daemon.rs")
        start = daemon.index("let request = build_request(&proc, config_version, &config_snapshot)")
        self.assertIn("if request.is_monitor_only", daemon[start : daemon.index("plans.push", start)])
        self.assertIn("!has_mount_state(&request)", daemon[start : daemon.index("plans.push", start)])
        self.assertIn("continue;", daemon[start : daemon.index("plans.push", start)])
        request = daemon[daemon.index("None if is_monitor_only => ("):]
        self.assertLess(request.index("MountOperation::Disable"), request.index("None => ("))
        entry = function(read("src/daemon_mount.rs"), "execute_mount_request")
        self.assertIn("request.operation != MountOperation::Disable", entry)

    def test_monitoring_is_not_sqlite_authorization(self):
        source = read("src/hook/media_fuse.rs")
        owner = function(source, "should_allow_private_owner_sqlite_for_owner")
        self.assertNotIn("is_file_monitor_enabled", owner)
        self.assertIn("is_user_profile_enabled_in_memory", owner)
        access = function(source, "should_allow_private_owner_sqlite_access_for_caller")
        self.assertLess(access.index("if !allow_by_owner"), access.index("let allow_by_path_token"))
        for name in ("has_recent_private_owner_sqlite_access_for_caller", "has_recent_private_owner_sqlite_access("):
            self.assertIn("!should_allow_private_owner_sqlite_for_owner", function(source, name))

    def test_passive_hooks_return_before_repairs_and_keep_original_arguments(self):
        source = read("src/hook/ops/open.rs")
        for name, original in (("handle_open_like", "call_original(pathname)"),
                               ("handle_openat_like", "call_original(dirfd, pathname)")):
            body = function(source, name)
            passive = body[body.index("if hub.is_passive_monitor()") : body.index("retry_fuse_fix_for_media_provider")]
            self.assertIn(original, passive)
            self.assertIn("runtime::set_errno(saved_errno)", passive)
            self.assertIn("return result", passive)
        rename = function(read("src/hook/ops/rename.rs"), "handle_rename_like")
        passive = rename[rename.index("if hub.is_passive_monitor()") : rename.index("let should_apply_policy")]
        self.assertIn("call_original(request.oldpath, request.newpath)", passive)
        self.assertIn("runtime::set_errno(saved_errno)", passive)
        metadata = read("src/hook/ops/mutation/meta.rs")
        for name in ("handle_ftruncate_fd_audit", "handle_futimens_fd_audit"):
            body = function(metadata, name)
            self.assertLess(body.index("hub.is_passive_monitor()"), body.index("resolve_dirfd_path"))
            self.assertIn("return handle_fd_path_audit", body)

    def test_actual_rust_paths_execute(self):
        rustc = shutil.which("rustc")
        if not rustc:
            self.skipTest("rustc is required for executable Rust guards")
        template = read(".github/tests/harness/passive_monitor_security.rs")
        roots = function(read("src/fuse_redirect/config.rs"), "scoped_fuse_mount_roots_for_request")
        post = function(read("src/lifecycle/specialize_post.rs"), "should_install_process_plt_hook")
        open_source = read("src/hook/ops/open.rs")
        # 注入真实被动分支；后续修复与重定向路径用 panic 替代，确认执行不会越过安全边界。
        opens = []
        for name in ("handle_open_like", "handle_openat_like"):
            body = function(open_source, name)
            opens.append(body[:body.index("    retry_fuse_fix_for_media_provider(hub);")]
                         + '    panic!("entered active I/O path");\n}')
        source = template.replace("// __ROOTS__", roots).replace("// __POST__", post)
        candidates = []
        for path, label in (("src/daemon_mount.rs", "daemon"), ("src/lifecycle/companion_mount.rs", "companion")):
            candidate = function(read(path), "shared_host_preattach_candidate")
            candidates.append(candidate.replace("fn shared_host_preattach_candidate", "fn " + label + "_candidate"))
        source = source.replace("// __CANDIDATES__", "\n".join(candidates))
        source = source.replace("// __SQLITE_OWNER__", function(
            read("src/hook/media_fuse.rs"), "should_allow_private_owner_sqlite_for_owner"))
        source = source.replace("// __OPENS__", "\n".join(opens))
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            harness = directory / "security.rs"
            binary = directory / "security.exe"
            harness.write_text(source, encoding="utf-8")
            build = subprocess.run([rustc, "--edition=2024", "--test", str(harness), "-o", str(binary)],
                                   capture_output=True, text=True, encoding="utf-8")
            self.assertEqual(0, build.returncode, build.stdout + build.stderr)
            run = subprocess.run([str(binary)], capture_output=True, text=True, encoding="utf-8")
            self.assertEqual(0, run.returncode, run.stdout + run.stderr)
            # 反向验证：移除真实被动分支后，可执行测试必须失败。
            broken = source.replace("if hub.is_passive_monitor()", "if false")
            harness.write_text(broken, encoding="utf-8")
            build = subprocess.run([rustc, "--edition=2024", "--test", str(harness), "-o", str(binary)],
                                   capture_output=True, text=True, encoding="utf-8")
            self.assertEqual(0, build.returncode, build.stdout + build.stderr)
            run = subprocess.run([str(binary)], capture_output=True, text=True, encoding="utf-8")
            self.assertNotEqual(0, run.returncode, "Broken passive guards must fail")


if __name__ == "__main__":
    unittest.main()
