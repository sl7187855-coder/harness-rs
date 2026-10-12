//! Linux child-process sandbox using Landlock, seccomp and resource limits.
//! Setup errors are fatal: callers must not fall back to an unsandboxed shell.
#![cfg(target_os = "linux")]

use std::{
    ffi::CString,
    fs, io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::process::CommandExt,
    },
    path::{Path, PathBuf},
    sync::Arc,
};

const LANDLOCK_CREATE_RULESET: libc::c_long = 444;
const LANDLOCK_ADD_RULE: libc::c_long = 445;
const LANDLOCK_RESTRICT_SELF: libc::c_long = 446;
const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;
const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;

const ACCESS_EXECUTE: u64 = 1 << 0;
const ACCESS_WRITE_FILE: u64 = 1 << 1;
const ACCESS_READ_FILE: u64 = 1 << 2;
const ACCESS_READ_DIR: u64 = 1 << 3;
const ACCESS_REMOVE_DIR: u64 = 1 << 4;
const ACCESS_REMOVE_FILE: u64 = 1 << 5;
const ACCESS_MAKE_CHAR: u64 = 1 << 6;
const ACCESS_MAKE_DIR: u64 = 1 << 7;
const ACCESS_MAKE_REG: u64 = 1 << 8;
const ACCESS_MAKE_SOCK: u64 = 1 << 9;
const ACCESS_MAKE_FIFO: u64 = 1 << 10;
const ACCESS_MAKE_BLOCK: u64 = 1 << 11;
const ACCESS_MAKE_SYM: u64 = 1 << 12;
const ACCESS_REFER: u64 = 1 << 13;
const ACCESS_TRUNCATE: u64 = 1 << 14;

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
}
#[repr(C)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

const BPF_LD_W_ABS: u16 = 0x20;
const BPF_ALU_AND_K: u16 = 0x54;
const BPF_JMP_JEQ_K: u16 = 0x15;
const BPF_RET_K: u16 = 0x06;
const SECCOMP_RET_KILL_PROCESS: u32 = 0x80000000;
const SECCOMP_RET_ALLOW: u32 = 0x7fff0000;
const SECCOMP_RET_ERRNO: u32 = 0x00050000;
const AUDIT_ARCH_X86_64: u32 = 0xc000003e;
const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
const PR_SET_SECCOMP: libc::c_int = 22;
const SECCOMP_MODE_FILTER: libc::c_ulong = 2;

#[repr(C)]
#[derive(Clone, Copy)]
struct SockFilter {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}
#[repr(C)]
struct SockFprog {
    len: u16,
    filter: *const SockFilter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    WorkspaceWrite,
    ReadOnly,
}

pub struct PreparedSandbox {
    ruleset_fd: OwnedFd,
    _path_fds: Vec<OwnedFd>,
    filters: Vec<SockFilter>,
    temp_dir: PathBuf,
}

impl PreparedSandbox {
    pub fn prepare(workspace: &Path, profile: Profile) -> io::Result<Arc<Self>> {
        let workspace = fs::canonicalize(workspace)?;
        if !workspace.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "sandbox workspace is not a directory",
            ));
        }
        let temp_dir = std::env::temp_dir().join(format!(
            "harness-sandbox-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir(&temp_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temp_dir, fs::Permissions::from_mode(0o700))?;
        }

        let result = Self::prepare_inner(&workspace, &temp_dir, profile);
        match result {
            Ok((ruleset_fd, path_fds, filters)) => Ok(Arc::new(Self {
                ruleset_fd,
                _path_fds: path_fds,
                filters,
                temp_dir,
            })),
            Err(error) => {
                let _ = fs::remove_dir_all(&temp_dir);
                Err(error)
            }
        }
    }

    fn prepare_inner(
        workspace: &Path,
        temp_dir: &Path,
        profile: Profile,
    ) -> io::Result<(OwnedFd, Vec<OwnedFd>, Vec<SockFilter>)> {
        let abi = unsafe {
            libc::syscall(
                LANDLOCK_CREATE_RULESET,
                std::ptr::null::<RulesetAttr>(),
                0usize,
                LANDLOCK_CREATE_RULESET_VERSION,
            )
        };
        if abi < 1 {
            return Err(io::Error::last_os_error());
        }
        let mut handled = ACCESS_EXECUTE
            | ACCESS_WRITE_FILE
            | ACCESS_READ_FILE
            | ACCESS_READ_DIR
            | ACCESS_REMOVE_DIR
            | ACCESS_REMOVE_FILE
            | ACCESS_MAKE_CHAR
            | ACCESS_MAKE_DIR
            | ACCESS_MAKE_REG
            | ACCESS_MAKE_SOCK
            | ACCESS_MAKE_FIFO
            | ACCESS_MAKE_BLOCK
            | ACCESS_MAKE_SYM;
        if abi >= 2 {
            handled |= ACCESS_REFER;
        }
        if abi >= 3 {
            handled |= ACCESS_TRUNCATE;
        }

        let attr = RulesetAttr {
            handled_access_fs: handled,
        };
        let ruleset = unsafe {
            libc::syscall(
                LANDLOCK_CREATE_RULESET,
                &attr,
                std::mem::size_of::<RulesetAttr>(),
                0u32,
            )
        };
        if ruleset < 0 {
            return Err(io::Error::last_os_error());
        }
        let ruleset_fd = unsafe { OwnedFd::from_raw_fd(ruleset as i32) };
        let mut path_fds = Vec::new();

        // The filesystem root is searchable/listable only; it grants no file reads.
        add_rule(
            &ruleset_fd,
            &mut path_fds,
            Path::new("/"),
            ACCESS_EXECUTE & handled,
        )?;
        for root in [
            "/usr",
            "/bin",
            "/sbin",
            "/lib",
            "/lib64",
            "/etc",
            "/usr/local",
            "/opt/node-24.21.0",
        ] {
            let path = Path::new(root);
            if path.exists() {
                add_rule(
                    &ruleset_fd,
                    &mut path_fds,
                    path,
                    (ACCESS_EXECUTE | ACCESS_READ_FILE | ACCESS_READ_DIR) & handled,
                )?;
            }
        }
        // /dev is read-only (so common read-only streams work), and /tmp itself
        // can be enumerated but only this execution's private child is writable.
        for root in ["/dev"] {
            let path = Path::new(root);
            if path.exists() {
                add_rule(
                    &ruleset_fd,
                    &mut path_fds,
                    path,
                    (ACCESS_READ_FILE | ACCESS_READ_DIR) & handled,
                )?;
            }
        }
        if Path::new("/tmp").exists() {
            add_rule(
                &ruleset_fd,
                &mut path_fds,
                Path::new("/tmp"),
                ACCESS_EXECUTE & handled,
            )?;
        }

        let ro_rights = (ACCESS_EXECUTE | ACCESS_READ_FILE | ACCESS_READ_DIR) & handled;
        add_rule(
            &ruleset_fd,
            &mut path_fds,
            workspace,
            if profile == Profile::WorkspaceWrite {
                handled
            } else {
                ro_rights
            },
        )?;
        add_rule(&ruleset_fd, &mut path_fds, temp_dir, handled)?;

        let filters = seccomp_filters();
        Ok((ruleset_fd, path_fds, filters))
    }

    /// Attach the prebuilt sandbox setup to a tokio process before spawn.
    /// The child closure performs only syscalls: no allocation, locks or I/O.
    pub fn attach(&self, command: &mut tokio::process::Command) {
        let ruleset_fd = self.ruleset_fd.as_raw_fd();
        let filters_addr = self.filters.as_ptr() as usize;
        let filters_len = self.filters.len() as u16;
        unsafe {
            command.as_std_mut().pre_exec(move || {
                if libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::syscall(LANDLOCK_RESTRICT_SELF, ruleset_fd, 0u32) != 0 {
                    return Err(io::Error::last_os_error());
                }
                let prog = SockFprog {
                    len: filters_len,
                    filter: filters_addr as *const SockFilter,
                };
                if libc::prctl(
                    PR_SET_SECCOMP,
                    SECCOMP_MODE_FILTER,
                    &prog as *const SockFprog,
                ) != 0
                {
                    return Err(io::Error::last_os_error());
                }
                set_limit(libc::RLIMIT_CPU, 120, 120)?;
                set_limit(libc::RLIMIT_NOFILE, 256, 256)?;
                set_limit(libc::RLIMIT_NPROC, 64, 64)?;
                set_limit(libc::RLIMIT_FSIZE, 1_000_000_000, 1_000_000_000)?;
                set_limit(libc::RLIMIT_CORE, 0, 0)?;
                Ok(())
            });
        }
    }

    pub fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }
}

impl Drop for PreparedSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temp_dir);
    }
}

fn set_limit(resource: libc::__rlimit_resource_t, soft: u64, hard: u64) -> io::Result<()> {
    let value = libc::rlimit {
        rlim_cur: soft as libc::rlim_t,
        rlim_max: hard as libc::rlim_t,
    };
    if unsafe { libc::setrlimit(resource, &value) } != 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn add_rule(ruleset: &OwnedFd, fds: &mut Vec<OwnedFd>, path: &Path, rights: u64) -> io::Result<()> {
    let canonical = fs::canonicalize(path)?;
    let text = canonical
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "non UTF-8 sandbox path"))?;
    let c_path = CString::new(text)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in sandbox path"))?;
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let rule = PathBeneathAttr {
        allowed_access: rights,
        parent_fd: fd.as_raw_fd(),
    };
    let result = unsafe {
        libc::syscall(
            LANDLOCK_ADD_RULE,
            ruleset.as_raw_fd(),
            LANDLOCK_RULE_PATH_BENEATH,
            &rule,
            0u32,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    fds.push(fd);
    Ok(())
}

fn seccomp_filters() -> Vec<SockFilter> {
    let mut blocked = vec![
        libc::SYS_socket,
        libc::SYS_socketpair,
        libc::SYS_connect,
        libc::SYS_bind,
        libc::SYS_listen,
        libc::SYS_accept,
        libc::SYS_accept4,
        libc::SYS_sendto,
        libc::SYS_recvfrom,
        libc::SYS_sendmsg,
        libc::SYS_recvmsg,
        libc::SYS_ptrace,
        libc::SYS_mount,
        libc::SYS_umount2,
        libc::SYS_kexec_load,
        libc::SYS_bpf,
        libc::SYS_perf_event_open,
        libc::SYS_keyctl,
        libc::SYS_unshare,
        libc::SYS_setns,
        libc::SYS_pivot_root,
        libc::SYS_init_module,
        libc::SYS_finit_module,
        libc::SYS_delete_module,
        libc::SYS_open_by_handle_at,
        libc::SYS_io_uring_setup,
        libc::SYS_io_uring_enter,
        libc::SYS_io_uring_register,
        // io_uring can issue socket/connect operations without invoking the
        // corresponding individual syscalls after the ring has been created.
        425, // io_uring_setup
        426, // io_uring_enter
        427, // io_uring_register
    ];
    blocked.sort_unstable();
    blocked.dedup();
    let mut filters = vec![
        SockFilter {
            code: BPF_LD_W_ABS,
            jt: 0,
            jf: 0,
            k: 4,
        },
        SockFilter {
            code: BPF_JMP_JEQ_K,
            jt: 1,
            jf: 0,
            k: AUDIT_ARCH_X86_64,
        },
        SockFilter {
            code: BPF_RET_K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_KILL_PROCESS,
        },
        SockFilter {
            code: BPF_LD_W_ABS,
            jt: 0,
            jf: 0,
            k: 0,
        },
        // Reject the x32 syscall ABI: it shares AUDIT_ARCH_X86_64 but sets
        // __X32_SYSCALL_BIT, which would otherwise evade the native syscall list.
        SockFilter {
            code: BPF_ALU_AND_K,
            jt: 0,
            jf: 0,
            k: 0x40000000,
        },
        SockFilter {
            code: BPF_JMP_JEQ_K,
            jt: 0,
            jf: 1,
            k: 0x40000000,
        },
        SockFilter {
            code: BPF_RET_K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_KILL_PROCESS,
        },
        SockFilter {
            code: BPF_LD_W_ABS,
            jt: 0,
            jf: 0,
            k: 0,
        },
    ];
    for nr in blocked {
        filters.push(SockFilter {
            code: BPF_JMP_JEQ_K,
            jt: 0,
            jf: 1,
            k: nr as u32,
        });
        filters.push(SockFilter {
            code: BPF_RET_K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_ERRNO | libc::EPERM as u32,
        });
    }
    filters.push(SockFilter {
        code: BPF_RET_K,
        jt: 0,
        jf: 0,
        k: SECCOMP_RET_ALLOW,
    });
    filters
}

#[cfg(not(target_arch = "x86_64"))]
compile_error!("Landlock/seccomp syscall filter currently requires x86_64");

#[cfg(test)]
mod tests {
    use super::*;
    use std::{process::Stdio, time::Duration};
    use tokio::{io::AsyncReadExt, process::Command, time::timeout};

    async fn run(
        workspace: &Path,
        command_text: &str,
        profile: Profile,
    ) -> io::Result<(i32, String, String)> {
        let sandbox = PreparedSandbox::prepare(workspace, profile)?;
        let mut command = Command::new("/bin/bash");
        command
            .args(["--noprofile", "--norc", "-lc", command_text])
            .current_dir(workspace)
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("HOME", sandbox.temp_dir())
            .env("TMPDIR", sandbox.temp_dir())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        sandbox.attach(&mut command);
        let mut child = command.spawn()?;
        let mut out = child.stdout.take().unwrap();
        let mut err = child.stderr.take().unwrap();
        let wait = timeout(Duration::from_secs(10), async {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let a = out.read_to_end(&mut stdout).await;
            let b = err.read_to_end(&mut stderr).await;
            if a.is_err() || b.is_err() {
                return Err(io::Error::other("pipe read failed"));
            }
            let status = child.wait().await?;
            Ok((
                status.code().unwrap_or(-1),
                String::from_utf8_lossy(&stdout).to_string(),
                String::from_utf8_lossy(&stderr).to_string(),
            ))
        })
        .await;
        match wait {
            Ok(r) => r,
            Err(_) => {
                let _ = child.kill().await;
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "sandbox process timed out",
                ))
            }
        }
    }

    #[tokio::test]
    async fn sandbox_allows_workspace_write_and_blocks_outside_read() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let outside = temp.path().join("secret.txt");
        fs::write(&outside, "TOP-SECRET").unwrap();
        let (code, out, _) = run(
            &workspace,
            "printf ok > good.txt; cat good.txt; cat ../secret.txt",
            Profile::WorkspaceWrite,
        )
        .await
        .unwrap();
        assert_eq!(out, "ok");
        assert_ne!(code, 0);
        assert_eq!(
            fs::read_to_string(workspace.join("good.txt")).unwrap(),
            "ok"
        );
    }

    #[tokio::test]
    async fn sandbox_denies_network_syscalls() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let (code, _, err) = run(
            &workspace,
            "python3 -c 'import socket; socket.socket()'",
            Profile::WorkspaceWrite,
        )
        .await
        .unwrap();
        assert_ne!(code, 0);
        assert!(!err.is_empty());
    }

    #[tokio::test]
    async fn sandbox_denies_io_uring_network_bypass() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let command = "python3 -c 'import ctypes; libc=ctypes.CDLL(None,use_errno=True); result=libc.syscall(425,1,0); print(result,ctypes.get_errno())'";
        let (code, out, err) = run(&workspace, command, Profile::ReadOnly).await.unwrap();
        assert_eq!(code, 0, "sandbox test process should survive: {err}");
        assert_eq!(
            out.trim(),
            "-1 1",
            "io_uring_setup must be denied with EPERM"
        );
    }

    #[tokio::test]
    async fn sandbox_kills_x32_abi_syscall_escape() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let command = "python3 -c 'import ctypes; ctypes.CDLL(None).syscall(0x40000029,0,0,0,0,0,0); print(\"X32_SYSCALL_SURVIVED\")'";
        let (code, out, _) = run(&workspace, command, Profile::ReadOnly).await.unwrap();
        assert_ne!(
            code, 0,
            "the x32 ABI syscall should kill only the sandboxed child"
        );
        assert!(!out.contains("X32_SYSCALL_SURVIVED"));
    }

    #[tokio::test]
    async fn sandbox_read_only_profile_blocks_workspace_write() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let (code, _, _) = run(
            &workspace,
            "printf blocked > forbidden.txt",
            Profile::ReadOnly,
        )
        .await
        .unwrap();

        assert_ne!(code, 0, "read-only workspace writes must fail");
        assert!(
            !workspace.join("forbidden.txt").exists(),
            "read-only sandbox must not create the target file"
        );
    }

    #[tokio::test]
    async fn sandbox_blocks_symlink_escape_to_outside_secret() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let outside = temp.path().join("secret.txt");
        fs::write(&outside, "SYMLINK-ESCAPE-SECRET").unwrap();
        symlink(&outside, workspace.join("leak.txt")).unwrap();

        let (code, out, _) = run(&workspace, "cat leak.txt", Profile::WorkspaceWrite)
            .await
            .unwrap();

        assert_ne!(
            code, 0,
            "workspace symlink must not grant outside read access"
        );
        assert!(
            !out.contains("SYMLINK-ESCAPE-SECRET"),
            "outside secret must not be exposed through symlink"
        );
    }
}
