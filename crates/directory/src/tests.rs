//! Docker CLI 文件执行，attempt 的持久封印与命名环境共同兑现 retire。
//! 参考：docs/Directory/README.md（文件执行、隔离与恢复）。

use super::*;
use croon_port::store::{Budgets, ContentStore};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

/// 目录测试用的临时目录，与 Fixture 的 state 目录分开。
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "croon-directory-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn content_at(path: &Path) -> Content {
    Content::from_inputs(
        Rc::new(ContentStore::open(path).unwrap()),
        &[],
        Budgets {
            nodes: 10000,
            read_bytes: 1000000,
            write_bytes: 1000000,
        },
    )
    .unwrap()
}
struct Fixture {
    docker: Docker,
    root: PathBuf,
    content: Rc<Content>,
    snapshot: String,
}
impl Fixture {
    fn new(image: &str) -> Self {
        let unique = format!(
            "croon-execution-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let root = std::env::temp_dir().join(&unique);
        fs::create_dir(&root).unwrap();
        let store = Rc::new(ContentStore::open(root.join("content")).unwrap());
        let content = Rc::new(
            Content::from_inputs(
                store,
                &[],
                Budgets {
                    nodes: 100000,
                    read_bytes: 64 << 20,
                    write_bytes: 64 << 20,
                },
            )
            .unwrap(),
        );
        let raw = content.put_bytes(b"delivered").unwrap();
        let file = content.pack(&raw, &[]).unwrap();
        let raw = content
            .put_json(&json!({"input":{"file":file,"x":false}}))
            .unwrap();
        let snapshot = content.pack(&raw, &[file]).unwrap();
        let docker = Docker::new(Config {
            identity: unique,
            state: root.join("state"),
            docker: "/usr/bin/docker".into(),
            python: "/usr/bin/python3".into(),
            image: image.into(),
            collector_image: "python:3.12-slim".into(),
            platform: "linux/amd64".into(),
            cpus: "0.5".into(),
            memory_bytes: 128 << 20,
            work_bytes: 8 << 20,
            temporary_bytes: 4 << 20,
            pids: 32,
            output_bytes: 128,
            nodes: 1000,
            archive_bytes: 12 << 20,
            cleanup_timeout: Duration::from_secs(10),
        })
        .unwrap();
        Self {
            docker,
            root,
            content,
            snapshot,
        }
    }
    fn call(&self, attempt: &str, value: &Json) -> Result<Box<dyn Execution>, HostError> {
        self.docker.start(&Call {
            instance: "test",
            generation: 1,
            key: attempt,
            source: "test",
            attempt,
            value,
            dependencies: std::slice::from_ref(&self.snapshot),
            content: self.content.clone(),
        })
    }
    fn request(&self, argv: &[&str], timeout: u64) -> Json {
        json!({"snapshot":self.snapshot,"mounts":{"readonly":self.snapshot},"argv":argv,"stdin":"","timeout":timeout})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // All test CLI requests have completed before teardown. Restrict removal to this unique provider.
        if let Ok(bytes) = self.docker.run(&[
            "ps",
            "-aq",
            "--filter",
            &format!("label=croon.provider={}", self.docker.prefix),
        ]) {
            for id in String::from_utf8_lossy(&bytes).lines() {
                let _ = self.docker.run(&["rm", "-f", id]);
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn finish(run: &mut dyn Execution) -> HostDone {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(done) = run.collect().unwrap() {
            return done;
        }
        assert!(Instant::now() < deadline, "collection deadline");
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[test]
#[ignore = "requires local Docker and preloaded alpine:latest; creates only unique test containers"]
fn docker_isolates_inputs_and_returns_nonzero_stable_artifacts() {
    let f = Fixture::new("alpine:latest");
    let value = f.request(&["/bin/sh","-c", "set -e; test $(cat /work/input) = delivered; test $(cat /mnt/readonly/input) = delivered; test ! -e /var/run/docker.sock; test ! -e /home/thooer; (touch /etc/croon-forbidden) 2>/dev/null && exit 98; echo changed > /work/input; (echo bad > /mnt/readonly/input) 2>/dev/null && exit 99; printf '%0300d' 1; echo artifact > /work/new; ln -s /etc/passwd /work/link; exit 7"],5);
    let mut run = f.call("nonzero", &value).unwrap();
    let mut duplicate = f.call("nonzero", &value).unwrap();
    let done = finish(run.as_mut());
    assert_eq!(done.outcome["value"]["exit"], 7);
    assert_eq!(done.outcome["value"]["truncated"], true);
    assert_eq!(duplicate.collect().unwrap().unwrap().outcome, done.outcome);
    let result = done.outcome["value"]["snapshot"].as_str().unwrap();
    let tree = read(&f.content, result).unwrap();
    assert!(
        tree.contains_key("new"),
        "tree={tree:?} outcome={}",
        done.outcome
    );
    assert!(matches!(tree["link"], croon_tree::Entry::Link { .. }));
    assert_ne!(result, f.snapshot);
    assert!(read(&f.content, &f.snapshot).unwrap().len() == 1);
    f.docker.retire("nonzero").unwrap();
    f.docker.retire("nonzero").unwrap();
    assert!(f.call("nonzero", &value).is_err());

    // Archive overflow must not retain a running helper or buffer the whole output in memory.
    let mut config = (*f.docker.config).clone();
    config.archive_bytes = 1024;
    let limited = Docker::new(config).unwrap();
    let limited_value = f.request(
        &["sh", "-c", "dd if=/dev/zero of=/work/large bs=4096 count=1"],
        2,
    );
    let mut run = limited
        .start(&Call {
            instance: "test",
            generation: 1,
            key: "archive-limit",
            source: "test",
            attempt: "archive-limit",
            value: &limited_value,
            dependencies: std::slice::from_ref(&f.snapshot),
            content: f.content.clone(),
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match run.collect() {
            Err(error) => {
                assert!(error.0.contains("budget"), "{error}");
                break;
            }
            Ok(None) => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(Some(_)) => panic!("archive budget was ignored"),
        }
    }
    let collector = format!("{}-collector", limited.name("archive-limit"));
    assert!(
        limited
            .run(&[
                "container",
                "ls",
                "-a",
                "--filter",
                &format!("name=^{collector}$"),
                "--format",
                "{{.Names}}"
            ])
            .unwrap()
            .is_empty()
    );
    assert!(
        fs::metadata(limited.path("archive-limit").join("archive.tmp"))
            .unwrap()
            .len()
            <= 1024
    );
    limited.retire("archive-limit").unwrap();
}
#[test]
#[ignore = "requires local Docker and preloaded python:3.12-slim"]
fn docker_timeout_kills_double_fork_setsid_and_preserves_last_files() {
    let f = Fixture::new("python:3.12-slim");
    let script = "import os,time\nif os.fork()==0:\n os.setsid()\n if os.fork()==0:\n  while True:\n   open('/work/heartbeat','w').write(str(time.time()))\n   time.sleep(.02)\n os._exit(0)\nwhile True: time.sleep(1)";
    let value = f.request(&["python3", "-c", script], 1);
    let mut run = f.call("timeout", &value).unwrap();
    // The provider deadline applies even before Kernel polls collect.
    std::thread::sleep(Duration::from_millis(1400));
    assert_eq!(
        f.docker
            .run(&[
                "inspect",
                "--format",
                "{{.State.Paused}}",
                &f.docker.name("timeout")
            ])
            .unwrap(),
        b"true\n"
    );
    let done = finish(run.as_mut());
    assert_eq!(done.outcome["value"]["timed_out"], true);
    assert!(
        read(
            &f.content,
            done.outcome["value"]["snapshot"].as_str().unwrap()
        )
        .unwrap()
        .contains_key("heartbeat"),
        "tree={:?} outcome={}",
        read(
            &f.content,
            done.outcome["value"]["snapshot"].as_str().unwrap()
        ),
        done.outcome
    );
    let name = f.docker.name("timeout");
    assert!(
        f.docker
            .run(&[
                "container",
                "ls",
                "-a",
                "--filter",
                &format!("name=^{name}$"),
                "--format",
                "{{.Names}}"
            ])
            .unwrap()
            .is_empty()
    );
    f.docker.retire("timeout").unwrap();
    assert!(f.call("timeout", &value).is_err());
}
#[test]
#[ignore = "requires local Docker and preloaded alpine:latest"]
fn docker_recovery_retires_uncollected_environment_and_fences_late_start() {
    let f = Fixture::new("alpine:latest");
    let value = f.request(&["sh", "-c", "echo crash > /work/crash; sleep 120"], 10);
    let run = f.call("crash", &value).unwrap();
    drop(run);
    // Reopen the same identity/state without the original in-memory execution map.
    let recovered = Docker::new((*f.docker.config).clone()).unwrap();
    recovered.retire("crash").unwrap();
    recovered.retire("crash").unwrap();
    assert!(
        recovered
            .start(&Call {
                instance: "test",
                generation: 1,
                key: "crash",
                source: "test",
                attempt: "crash",
                value: &value,
                dependencies: std::slice::from_ref(&f.snapshot),
                content: f.content.clone()
            })
            .is_err()
    );
    recovered.tombstone(&recovered.name("crash")).unwrap();
    let clean = f.request(
        &[
            "sh",
            "-c",
            "test ! -e /work/crash; echo recovered > /work/recovered",
        ],
        2,
    );
    let mut run = f.call("replacement", &clean).unwrap();
    let done = finish(run.as_mut());
    assert_eq!(done.outcome["value"]["exit"], 0);
    recovered.retire("replacement").unwrap();
    // A late name-based start can only launch an inert tombstone, never the old workload.
    let retired = recovered.name("crash");
    recovered.run(&["start", &retired]).unwrap();
    recovered.tombstone(&retired).unwrap();
    let mounts = recovered
        .run(&["inspect", "--format", "{{json .Mounts}}", &retired])
        .unwrap();
    assert_eq!(mounts, b"[]\n");

    // Fail the local CLI spawn only for the actual task exec, after create/start/seed succeeded.
    let wrapper = f.root.join("fail-exec");
    fs::write(
        &wrapper,
        r#"#!/bin/sh
if [ "$1" = exec ]; then
 /usr/bin/docker "$@"
 result=$?
 rm -- "$0"
 exit "$result"
fi
exec /usr/bin/docker "$@"
"#,
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = (*f.docker.config).clone();
    config.docker = wrapper;
    let failing = Docker::new(config).unwrap();
    let call = Call {
        instance: "test",
        generation: 1,
        key: "failed-exec",
        source: "test",
        attempt: "failed-exec",
        value: &value,
        dependencies: std::slice::from_ref(&f.snapshot),
        content: f.content.clone(),
    };
    // The seed copy succeeds, then the CLI executable disappears before task spawn.
    assert!(failing.start(&call).is_err());
    assert!(failing.path("failed-exec").join("retired").exists());
    recovered.retire("failed-exec").unwrap();
    assert!(f.call("failed-exec", &value).is_err());
}
#[test]
fn archive_parser_rejects_traversal_hardlinks_and_link_parents() {
    let root = std::env::temp_dir().join(format!(
        "croon-archive-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    for kind in ["traversal", "hardlink", "link_parent", "oversize"] {
        let archive = root.join("bad.tar");
        let output = root.join("out");
        fs::create_dir(&output).unwrap();
        let script = "import tarfile,io,sys\np,k=sys.argv[1:]\nwith tarfile.open(p,'w') as t:\n m=tarfile.TarInfo('../escape' if k=='traversal' else 'a')\n if k=='hardlink': m.type=tarfile.LNKTYPE;m.linkname='other'\n if k=='link_parent': m.type=tarfile.SYMTYPE;m.linkname='/tmp'\n if k=='oversize': m.size=1024\n t.addfile(m,io.BytesIO(b'x'*1024))\n if k=='link_parent': t.addfile(tarfile.TarInfo('a/escape'))";
        let mut create = Command::new("/usr/bin/python3");
        create.args(["-I", "-c", script]).arg(&archive).arg(kind);
        bounded(create, Duration::from_secs(5), 1024).unwrap();
        let mut parser = Command::new("/usr/bin/python3");
        parser
            .args(["-I", "-c", include_str!("archive.py")])
            .arg(&archive)
            .arg(&output)
            .args(["10", "10"]);
        assert!(
            bounded(parser, Duration::from_secs(5), 1024).is_err(),
            "{kind}"
        );
        assert!(!root.join("escape").exists());
        fs::remove_dir_all(&output).unwrap();
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lstat_import_preserves_links_without_reading_target_and_round_trips_modes() {
    let temp = Temp::new();
    let content = content_at(&temp.0.join("store"));
    let source = temp.0.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("script"), b"hello").unwrap();
    std::fs::set_permissions(
        source.join("script"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    std::os::unix::fs::symlink("/not-delivered/secret", source.join("link")).unwrap();
    let limits = Limits {
        nodes: 100,
        bytes: 1000,
    };
    let id = import(&content, &source, limits).unwrap();
    let out = temp.0.join("out");
    std::fs::create_dir(&out).unwrap();
    materialize(&content, &id, &out, limits).unwrap();
    assert_eq!(
        std::fs::read_link(out.join("link")).unwrap(),
        Path::new("/not-delivered/secret")
    );
    assert_eq!(import(&content, &out, limits).unwrap(), id);
    assert!(materialize(&content, &id, &out, limits).is_err());
    assert!(import(&content, &source, Limits { nodes: 1, bytes: 1 }).is_err());
}
#[test]
fn materialized_modes_follow_the_documented_ones_not_the_umask() {
    // docs/Directory/README.md：目录 0755，普通文件 0644，可执行文件 0755，与 umask 无关。
    let temp = Temp::new();
    let content = content_at(&temp.0.join("store"));
    let source = temp.0.join("source");
    std::fs::create_dir_all(source.join("sub/deeper")).unwrap();
    std::fs::write(source.join("plain"), b"p").unwrap();
    std::fs::write(source.join("sub/run.sh"), b"#!/bin/sh\n").unwrap();
    std::fs::write(source.join("sub/deeper/note"), b"n").unwrap();
    std::fs::set_permissions(
        source.join("sub/run.sh"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let limits = Limits {
        nodes: 100,
        bytes: 1000,
    };
    let id = import(&content, &source, limits).unwrap();
    for umask in [0o022, 0o077] {
        let out = temp.0.join(format!("out-{umask:o}"));
        std::fs::create_dir(&out).unwrap();
        let old = unsafe { libc::umask(umask) };
        let done = materialize(&content, &id, &out, limits);
        unsafe { libc::umask(old) };
        done.unwrap();
        let mode = |p: &str| std::fs::metadata(out.join(p)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode("sub"), 0o755, "umask {umask:o}");
        assert_eq!(mode("sub/deeper"), 0o755, "umask {umask:o}");
        assert_eq!(mode("plain"), 0o644, "umask {umask:o}");
        assert_eq!(mode("sub/run.sh"), 0o755, "umask {umask:o}");
    }
}
#[test]
#[ignore = "requires local Docker and preloaded alpine:latest; creates only unique test containers"]
fn a_snapshot_with_subdirectories_is_readable_by_the_next_execution() {
    // 第一次执行在 /work 里建出子目录并交回；第二次执行以那份快照为输入，第一条 cp 要读得到它。
    // 容器去掉了越权读的 capability，所以输入目录自己的权限必须让它读得到。
    let f = Fixture::new("alpine:latest");
    let first = f.request(
        &["/bin/sh", "-c", "mkdir -p /work/__pycache__ && echo x > /work/__pycache__/a.pyc && echo y > /work/b"],
        5,
    );
    let mut run = f.call("first", &first).unwrap();
    let done = finish(run.as_mut());
    let produced = done.outcome["value"]["snapshot"].as_str().unwrap().to_owned();
    assert!(read(&f.content, &produced).unwrap().contains_key("__pycache__"));

    let second = json!({"snapshot": produced, "mounts": {}, "argv": ["/bin/sh", "-c", "cat /work/__pycache__/a.pyc"], "stdin": "", "timeout": 5});
    let mut run = f
        .docker
        .start(&Call {
            instance: "test",
            generation: 1,
            key: "second",
            source: "test",
            attempt: "second",
            value: &second,
            dependencies: std::slice::from_ref(&produced),
            content: f.content.clone(),
        })
        .expect("子目录里有内容的快照，下一次执行要起得来");
    let done = finish(run.as_mut());
    assert_eq!(done.outcome["value"]["exit"], 0, "{}", done.outcome);
    assert_eq!(done.outcome["value"]["stdout"], "x\n");
}
#[test]
fn materialization_rejects_link_destination_and_file_dependency_smuggling() {
    let temp = Temp::new();
    let content = content_at(&temp.0.join("store"));
    let raw = content.put_bytes(b"file").unwrap();
    let file = content.pack(&raw, &[]).unwrap();
    let fake_file = content.pack(&raw, &[file]).unwrap();
    let tree = content
        .put_json(&json!({"f":{"file":fake_file,"x":false}}))
        .unwrap();
    let id = content.pack(&tree, &[fake_file]).unwrap();
    let out = temp.0.join("out");
    std::fs::create_dir(&out).unwrap();
    assert!(
        materialize(
            &content,
            &id,
            &out,
            Limits {
                nodes: 10,
                bytes: 100
            }
        )
        .is_err()
    );
    let link = temp.0.join("linked");
    std::os::unix::fs::symlink(&out, &link).unwrap();
    assert!(
        materialize(
            &content,
            &id,
            &link,
            Limits {
                nodes: 10,
                bytes: 100
            }
        )
        .is_err()
    );
    assert!(std::fs::read_dir(out).unwrap().next().is_none());
}
