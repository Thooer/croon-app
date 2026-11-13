//! Docker CLI 文件执行，attempt 的持久封印与命名环境共同兑现 retire。
//! 参考：docs/Directory/README.md（文件执行、隔离与恢复）。

use super::*;

impl Execution for Running {
    fn collect(&mut self) -> Result<Option<HostDone>, HostError> {
        if let Some(done) = &self.done {
            return Ok(Some(done.clone()));
        }
        let provider = &self.provider;
        let _lock = provider.lock(&self.attempt)?;
        let path = provider.path(&self.attempt);
        if path.join("retired").exists() {
            return Err(host("attempt retired before stable collection"));
        }
        let name = self.container.clone();
        if let Ok(status) = self.watchdog_status.try_recv() {
            self.timed_out = true;
            status.map_err(|e| host(format!("timeout freeze failed; retry collection: {e}")))?;
        }
        if self.exit.is_none() {
            if let Some(status) = self.child.try_wait().map_err(host)? {
                self.exit = Some(status.code().unwrap_or(137));
            } else if Instant::now() < self.deadline {
                return Ok(None);
            } else {
                self.timed_out = true;
                self.exit = Some(137);
            }
        }
        let archive = path.join("work.tar");
        if !archive.exists() {
            let paused = provider.run(&["inspect", "--format", "{{.State.Paused}}", &name])?;
            if paused != b"true\n" {
                provider.run(&["pause", &name])?;
            }
            let collector_name = format!("{}-collector", provider.name(&self.attempt));
            provider.remove(&collector_name)?;
            let args = vec![
                "create".to_owned(),
                "--pull=never".into(),
                "--name".into(),
                collector_name.clone(),
                "--label".into(),
                format!("croon.provider={}", provider.prefix),
                format!("--pid=container:{name}"),
                "--network=none".into(),
                "--read-only".into(),
                "--user=0:0".into(),
                "--cap-drop=ALL".into(),
                "--cap-add=SYS_PTRACE".into(),
                "--security-opt=no-new-privileges".into(),
                "--log-driver=none".into(),
                "--pids-limit".into(),
                provider.config.pids.to_string(),
                "--memory".into(),
                provider.config.memory_bytes.to_string(),
                "--memory-swap".into(),
                provider.config.memory_bytes.to_string(),
                "--cpus".into(),
                provider.config.cpus.clone(),
                "--platform".into(),
                provider.config.platform.clone(),
                "--entrypoint=python3".into(),
                provider.config.collector_image.clone(),
                "-I".into(),
                "-c".into(),
                include_str!("collect.py").into(),
                provider.config.nodes.to_string(),
                provider.config.work_bytes.to_string(),
            ];
            let created = provider.run(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
            let collector = String::from_utf8(created).map_err(host)?.trim().to_owned();
            if collector.len() != 64 || !collector.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(host("invalid collector ID"));
            }
            let mut command = provider.command();
            command.args(["start", "-a", &collector]);
            let temp = path.join("archive.tmp");
            let collected = process::archive(
                command,
                provider.config.cleanup_timeout,
                provider.config.archive_bytes,
                &temp,
            );
            if let Err(error) = collected {
                let cleanup = provider.remove_id(&collector);
                return Err(host(format!("{error}; collector cleanup: {cleanup:?}")));
            }
            let status =
                provider.run(&["inspect", "--format", "{{.State.ExitCode}}", &collector])?;
            if status != b"0\n" {
                provider.remove_id(&collector)?;
                return Err(host(
                    "trusted collector failed; stable archive not established",
                ));
            }
            provider.run(&["rm", "-f", &collector])?;
            fs::rename(temp, &archive).map_err(host)?;
            File::open(&path).map_err(host)?.sync_all().map_err(host)?;
        }
        if !self.removed {
            provider.remove_id(&name)?;
            self.removed = true;
        }
        let _ = self.watchdog_cancel.send(());
        if self.output.is_none() {
            // Removal closes all namespace writers and their inherited stdout/stderr descriptors.
            let _ = self.child.kill();
            self.child.wait().map_err(host)?;
            let (stdout, trunc_out) = self
                .stdout
                .take()
                .unwrap()
                .join()
                .map_err(|_| host("stdout reader panicked"))?
                .map_err(host)?;
            let (stderr, trunc_err) = self
                .stderr
                .take()
                .unwrap()
                .join()
                .map_err(|_| host("stderr reader panicked"))?
                .map_err(host)?;
            self.output = Some((stdout, stderr, trunc_out || trunc_err));
        }
        let artifacts = path.join("artifacts");
        if artifacts.exists() {
            fs::remove_dir_all(&artifacts).map_err(host)?;
        }
        fs::create_dir(&artifacts).map_err(host)?;
        let mut parser = Command::new(&provider.config.python);
        parser
            .args(["-I", "-c", include_str!("archive.py")])
            .arg(&archive)
            .arg(&artifacts)
            .arg(provider.config.nodes.to_string())
            .arg(provider.config.work_bytes.to_string());
        bounded(parser, provider.config.cleanup_timeout, 65536)?;
        let snapshot = import(
            &self.content,
            &artifacts,
            Limits {
                nodes: provider.config.nodes,
                bytes: provider.config.work_bytes,
            },
        )?;
        let (stdout, stderr, truncated) = self.output.as_ref().unwrap();
        let done = HostDone {
            outcome: json!({"ok":true,"value":{
            "exit": self.exit.unwrap(), "stdout":String::from_utf8_lossy(stdout), "stderr":String::from_utf8_lossy(stderr),
            "timed_out":self.timed_out,"truncated":truncated,"snapshot":snapshot},"dependencies":[snapshot]}),
        };
        self.done = Some(done.clone());
        Ok(Some(done))
    }
}
