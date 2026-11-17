//! croon-app：开始一次运行，续做被中断的运行，或直接运行一份源码。
//! 用法：
//!   croon-app run    [--config 配置] [--session 名字] --repo 仓库 --out 结果目录 [--tools 清单] [--quiet] 任务
//!   croon-app resume [--config 配置] --session 名字 --out 结果目录 [--quiet]
//!   croon-app exec   [--config 配置] [--session 名字] --source 源码文件 --input JSON [--snapshot 目录] --out 结果目录 [--quiet]
//! 没给 --config 时找 $XDG_CONFIG_HOME/croon/config.json（默认 ~/.config/croon/config.json）。
//! 参考：docs/Host/README.md（配置与会话、客户端）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "用法：
  croon-app run    [--config 配置] [--session 名字] --repo 仓库 --out 结果目录 [--tools 清单] [--quiet] 任务
  croon-app resume [--config 配置] --session 名字 --out 结果目录 [--quiet]
  croon-app exec   [--config 配置] [--session 名字] --source 源码文件 --input JSON [--snapshot 目录] --out 结果目录 [--quiet]

--config 不给时找 ~/.config/croon/config.json；每次运行的目录放在 ~/.local/state/croon/sessions/<名字>/。";

#[derive(Default)]
struct Args {
    config: Option<PathBuf>,
    session: Option<String>,
    repo: Option<PathBuf>,
    out: Option<PathBuf>,
    tools: Option<PathBuf>,
    source: Option<PathBuf>,
    input: Option<String>,
    task: Option<String>,
    quiet: bool,
}

fn parse(rest: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = rest.iter();
    let value = |it: &mut std::slice::Iter<String>, flag: &str| {
        it.next().cloned().ok_or_else(|| format!("{flag} 后面要跟一个值"))
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--config" => a.config = Some(value(&mut it, arg)?.into()),
            "--session" => a.session = Some(value(&mut it, arg)?),
            "--repo" | "--snapshot" => a.repo = Some(value(&mut it, arg)?.into()),
            "--out" => a.out = Some(value(&mut it, arg)?.into()),
            "--tools" => a.tools = Some(value(&mut it, arg)?.into()),
            "--source" => a.source = Some(value(&mut it, arg)?.into()),
            "--input" => a.input = Some(value(&mut it, arg)?),
            "--quiet" => a.quiet = true,
            s if s.starts_with("--") => return Err(format!("不认识的参数 {s}")),
            _ if a.task.is_none() => a.task = Some(arg.clone()),
            _ => return Err(format!("多出来的参数 {arg:?}（任务要写成一个整体，加引号）")),
        }
    }
    Ok(a)
}

/// 用户说「我要做什么」时缺的东西，一句话说清缺什么，再给用法。
fn missing(what: &str) -> ExitCode {
    eprintln!("缺少 {what}\n\n{USAGE}");
    ExitCode::from(2)
}

/// 最近的几个会话，按修改时间新到旧：名字与有没有结束（Kernel 目录里有 journal 就算开始过了）。
fn recent_sessions(sessions: &Path) -> Vec<String> {
    let mut found: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(sessions)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, e.file_name().to_string_lossy().into_owned()))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().take(8).map(|(_, n)| n).collect()
}

/// 自动取的会话名：UTC 的日期时间加任务的短哈希。同一秒里同一个任务的重复不会撞：哈希之后再加序号。
fn auto_name(task: &str, sessions: &Path) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let hash = task
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    let base = format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}-{:06x}",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60,
        hash & 0xff_ffff
    );
    let mut name = base.clone();
    let mut n = 2;
    while sessions.join(&name).exists() {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("");
    if !matches!(command, "run" | "resume" | "exec") {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let a = match parse(&args[1..]) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let Some(out) = a.out.clone() else {
        return missing("--out 结果目录");
    };
    let resuming = command == "resume";
    if resuming && (a.repo.is_some() || a.task.is_some() || a.tools.is_some() || a.source.is_some() || a.input.is_some()) {
        eprintln!("resume 不收仓库、任务、源码与清单：它们取自这次运行自己的记录\n\n{USAGE}");
        return ExitCode::from(2);
    }

    // 配置：--config，或默认位置；都没有就报错，说清放哪里。
    let config = a.config.clone().or_else(host::default_config);
    let Some(config) = config else {
        eprintln!(
            "没有找到配置：用 --config 指定一个，或把它放在 ~/.config/croon/config.json。\n最小的配置只有模型服务：{{\"model\": {{\"provider\": \"/绝对路径/agent-provider.json\"}}}}，见 README「上手」。"
        );
        return ExitCode::from(2);
    };
    let settings = match host::Settings::load(&config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("读配置 {} 失败：{e}", config.display());
            return ExitCode::FAILURE;
        }
    };
    let sessions = match host::state_home() {
        Ok(h) => h.join("sessions"),
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    // 会话：resume 一定要名字；run 与 exec 不给就自己取一个并打印出来。
    let name = match (&a.session, resuming) {
        (Some(n), _) => n.clone(),
        (None, true) if settings.system.is_some() => settings.system.clone().unwrap(),
        (None, true) => {
            let recent = recent_sessions(&sessions);
            if recent.is_empty() {
                eprintln!("resume 要 --session 名字，而这台机器上还没有任何会话（{}）。", sessions.display());
            } else {
                eprintln!("resume 要 --session 名字。最近的会话：\n  {}", recent.join("\n  "));
            }
            return ExitCode::from(2);
        }
        (None, false) if settings.system.is_some() => settings.system.clone().unwrap(),
        (None, false) => auto_name(a.task.as_deref().or(a.input.as_deref()).unwrap_or(""), &sessions),
    };
    let session = match settings.session(Some(&name), &sessions) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    if !resuming && !a.quiet {
        eprintln!("会话 {}（resume 时用 --session {}）", session.name, session.name);
    }

    let tools = match a.tools.as_ref().map(|p| {
        std::fs::read(p)
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
    }) {
        None => None,
        Some(Ok(t)) => Some(t),
        Some(Err(e)) => {
            eprintln!("--tools：{e}");
            return ExitCode::from(2);
        }
    };
    let assembly = host::Assembly {
        tools,
        session: Some(session),
        quiet: a.quiet,
        ..Default::default()
    };

    let result = match command {
        "resume" => host::resume(&settings, assembly, &out),
        "exec" => {
            let (Some(source), Some(input)) = (a.source.clone(), a.input.clone()) else {
                return missing("--source 与 --input");
            };
            let source = match std::fs::read_to_string(&source) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("读 {} 失败：{e}", source.display());
                    return ExitCode::from(2);
                }
            };
            let input = match serde_json::from_str(&input) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("--input 不是合法的 JSON：{e}");
                    return ExitCode::from(2);
                }
            };
            host::exec(&settings, assembly, &source, input, a.repo.as_deref(), &out)
        }
        _ => {
            let (Some(repo), Some(task)) = (a.repo.clone(), a.task.clone()) else {
                return missing("--repo 仓库 与 任务");
            };
            host::run(&settings, assembly, &repo, &task, &out)
        }
    };
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
