//! croon-app：开始一次运行，或续做被中断的运行。
//! 用法：
//!   croon-app run --config 配置 --repo 仓库 --out 结果目录 [--tools 清单] 任务
//!   croon-app resume --config 配置 --out 结果目录
//!   croon-app exec --config 配置 --source 源码文件 --input JSON [--snapshot 目录] --out 结果目录
//! 参考：docs/Host/README.md（客户端）。

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "用法：
  croon-app run --config 配置 --repo 仓库 --out 结果目录 [--tools 清单] 任务
  croon-app resume --config 配置 --out 结果目录
  croon-app exec --config 配置 --source 源码文件 --input JSON [--snapshot 目录] --out 结果目录";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str);
    if !matches!(command, Some("run" | "resume" | "exec")) {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let (mut config, mut repo, mut out, mut task, mut tools) = (None, None, None, None, None);
    let (mut source, mut input) = (None, None);
    let mut rest = args[1..].iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--config" => config = rest.next().map(PathBuf::from),
            "--repo" => repo = rest.next().map(PathBuf::from),
            "--out" => out = rest.next().map(PathBuf::from),
            "--tools" => tools = rest.next().map(PathBuf::from),
            "--source" => source = rest.next().map(PathBuf::from),
            "--input" => input = rest.next().cloned(),
            "--snapshot" => repo = rest.next().map(PathBuf::from),
            _ if task.is_none() => task = Some(arg.clone()),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let (Some(config), Some(out)) = (config, out) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let resuming = command == Some("resume");
    if resuming && (repo.is_some() || task.is_some() || tools.is_some() || source.is_some() || input.is_some()) {
        eprintln!("resume 不收仓库、任务、源码与清单：它们取自 Journal\n{USAGE}");
        return ExitCode::from(2);
    }
    let assembly = match tools.map(|p| {
        std::fs::read(&p)
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
    }) {
        None => host::Assembly::default(),
        Some(Ok(tools)) => host::Assembly {
            tools: Some(tools),
            ..Default::default()
        },
        Some(Err(error)) => {
            eprintln!("--tools：{error}");
            return ExitCode::from(2);
        }
    };
    let result = host::Settings::load(&config).and_then(|settings| {
        if resuming {
            host::resume(&settings, assembly, &out)
        } else if command == Some("exec") {
            let (Some(source), Some(input)) = (source, input) else {
                return Err(host::Error(USAGE.into()));
            };
            let source = std::fs::read_to_string(&source).map_err(|e| host::Error(format!("{}：{e}", source.display())))?;
            let input = serde_json::from_str(&input).map_err(|e| host::Error(format!("--input：{e}")))?;
            host::exec(&settings, assembly, &source, input, repo.as_deref(), &out)
        } else {
            let (Some(repo), Some(task)) = (repo, task) else {
                return Err(host::Error(USAGE.into()));
            };
            host::run(&settings, assembly, &repo, &task, &out)
        }
    });
    match result {
        Ok(finished) => {
            println!("{}", finished.summary);
            if !finished.snapshot.is_empty() {
                println!("结果：{}（{}）", out.display(), finished.snapshot);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
