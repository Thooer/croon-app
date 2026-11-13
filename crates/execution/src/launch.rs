//! Docker CLI 文件执行，attempt 的持久封印与命名环境共同兑现 retire。
//! 参考：docs/Directory/README.md（文件执行、隔离与恢复）。

use super::*;

impl Docker {
    pub(super) fn launch(&self, call: &Call, request: Request) -> Result<Running, HostError> {
        let path = self.path(call.attempt);
        let input = path.join("input");
        fs::create_dir(&input).map_err(host)?;
        let limits = Limits {
            nodes: self.config.nodes,
            bytes: self.config.work_bytes,
        };
        materialize(&call.content, &request.snapshot, &input, limits)?;
        let input_text = input
            .to_str()
            .ok_or_else(|| host("non-Unicode input path"))?;
        if input_text.contains(',') {
            return Err(host("comma in input path"));
        }
        let mut mount_paths = vec![format!(
            "type=bind,src={input_text},dst=/croon-input,readonly,bind-propagation=rprivate,bind-recursive=readonly"
        )];
        for (name, id) in request.mounts {
            let mount = path.join(format!(
                "mount-{}",
                kernel::value::ValueId::of_bytes(name.as_bytes()).hex()
            ));
            fs::create_dir(&mount).map_err(host)?;
            materialize(&call.content, &id, &mount, limits)?;
            let mount_text = mount
                .to_str()
                .ok_or_else(|| host("non-Unicode mount path"))?;
            // --mount uses comma as a delimiter; reject rather than reinterpret a host path.
            if mount_text.contains(',') || name.contains(',') {
                return Err(host("comma in mount path"));
            }
            mount_paths.push(format!("type=bind,src={mount_text},dst=/mnt/{name},readonly,bind-propagation=rprivate,bind-recursive=readonly"));
        }
        fs::write(path.join("stdin"), request.stdin.as_bytes()).map_err(host)?;
        let name = self.name(call.attempt);
        let mut args = vec![
            "create".to_owned(),
            "--pull=never".into(),
            "--name".into(),
            name.clone(),
            "--label".into(),
            format!("croon.provider={}", self.prefix),
            "--platform".into(),
            self.config.platform.clone(),
            "--network=none".into(),
            "--read-only".into(),
            "--user=0:0".into(),
            "--cap-drop=ALL".into(),
            "--security-opt=no-new-privileges".into(),
            "--pids-limit".into(),
            self.config.pids.to_string(),
            "--memory".into(),
            self.config.memory_bytes.to_string(),
            "--memory-swap".into(),
            self.config.memory_bytes.to_string(),
            "--cpus".into(),
            self.config.cpus.clone(),
            "--log-driver=none".into(),
            "--ipc=private".into(),
            "--shm-size=1048576".into(),
            "--tmpfs".into(),
            format!(
                "/work:rw,nosuid,nodev,size={},mode=0755",
                self.config.work_bytes
            ),
            "--tmpfs".into(),
            format!(
                "/tmp:rw,nosuid,nodev,noexec,size={},mode=1777",
                self.config.temporary_bytes
            ),
            "--workdir=/work".into(),
            "--entrypoint=/bin/sh".into(),
        ];
        for mount in mount_paths {
            args.push("--mount".into());
            args.push(mount);
        }
        args.extend([
            self.config.image.clone(),
            "-c".into(),
            "while :; do sleep 3600; done".into(),
        ]);
        let created = self.run(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
        let container = String::from_utf8(created).map_err(host)?.trim().to_owned();
        if container.len() != 64 || !container.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(host("invalid Docker container ID"));
        }
        self.run(&["start", &container])?;
        self.run(&[
            "exec",
            &container,
            "/bin/sh",
            "-c",
            "cp -RP /croon-input/. /work/",
        ])?;
        let mut command = self.command();
        command
            .args(["exec", "-i", "--workdir=/work", &container])
            .args(&request.argv)
            .stdin(File::open(path.join("stdin")).map_err(host)?)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(host)?;
        let stdout = drain(child.stdout.take().unwrap(), self.config.output_bytes);
        let stderr = drain(child.stderr.take().unwrap(), self.config.output_bytes);
        let deadline = Instant::now() + request.timeout;
        let (watchdog_cancel, cancel) = std::sync::mpsc::channel();
        let (status, watchdog_status) = std::sync::mpsc::channel();
        let docker = self.config.docker.clone();
        let target = container.clone();
        let cleanup_timeout = self.config.cleanup_timeout;
        let watchdog = std::thread::Builder::new()
            .name("execution-timeout".into())
            .spawn(move || {
                if matches!(
                    cancel.recv_timeout(request.timeout),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                ) {
                    let mut pause = Command::new(docker);
                    pause.args(["pause", &target]);
                    let _ = status.send(bounded(pause, cleanup_timeout, 65536));
                }
            });
        if let Err(error) = watchdog {
            let _ = child.kill();
            let _ = child.wait();
            return Err(host(error));
        }
        Ok(Running {
            provider: Docker {
                config: self.config.clone(),
                prefix: self.prefix.clone(),
                runs: Default::default(),
            },
            attempt: call.attempt.into(),
            container,
            content: call.content.clone(),
            child,
            stdout: Some(stdout),
            stderr: Some(stderr),
            deadline,
            watchdog_cancel,
            watchdog_status,
            exit: None,
            timed_out: false,
            output: None,
            removed: false,
            done: None,
        })
    }
}
