//! croon-app：一次性运行一个任务。
//! 用法：croon-app run --config 配置 --repo 仓库 --out 新目录 任务
//! 参考：docs/Host/README.md（客户端）。

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "用法：croon-app run --config 配置 --repo 仓库 --out 新目录 任务";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("run") {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let (mut config, mut repo, mut out, mut task) = (None, None, None, None);
    let mut rest = args[1..].iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--config" => config = rest.next().map(PathBuf::from),
            "--repo" => repo = rest.next().map(PathBuf::from),
            "--out" => out = rest.next().map(PathBuf::from),
            _ if task.is_none() => task = Some(arg.clone()),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let (Some(config), Some(repo), Some(out), Some(task)) = (config, repo, out, task) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match host::Settings::load(&config)
        .and_then(|s| host::run(&s, host::Assembly::default(), &repo, &task, &out))
    {
        Ok(finished) => {
            println!("{}", finished.summary);
            println!("结果：{}（{}）", out.display(), finished.snapshot);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
