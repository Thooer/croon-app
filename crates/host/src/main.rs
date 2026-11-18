//! croon-app：开始一次运行，续做被中断的运行，或直接运行一份源码。
//! 用法：
//!   croon-app run    [--config 配置] [--session 名字] --repo 仓库 --out 结果目录 [--tools 清单] [--quiet] 任务
//!   croon-app resume [--config 配置] --session 名字 --out 结果目录 [--quiet]
//!   croon-app describe --source 源码文件 [--tool 注册名 [--snapshot 目录]]
//!   croon-app exec   [--config 配置] [--session 名字] --source 源码文件 --input JSON [--snapshot 目录] --out 结果目录 [--json] [--quiet]
//! 没给 --config 时找 $XDG_CONFIG_HOME/croon/config.json（默认 ~/.config/croon/config.json）。
//! 参考：docs/Host/README.md（配置与会话、客户端）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "用法：
  croon-app run    [--config 配置] [--session 名字] --repo 仓库 --out 结果目录 [--tools 清单] [--quiet] 任务
  croon-app resume [--config 配置] --session 名字 --out 结果目录 [--quiet]
  croon-app describe --source 源码文件 [--tool 注册名 [--snapshot 目录]]
  croon-app exec   [--config 配置] [--session 名字] --source 源码文件 --input JSON [--snapshot 目录] --out 结果目录 [--json] [--quiet]

--config 不给时找 ~/.config/croon/config.json；每次运行的目录放在 ~/.local/state/croon/sessions/<名字>/。";

#[derive(Default)]
struct Args {
    config: Option<PathBuf>,
    session: Option<String>,
    repo: Option<PathBuf>,
    out: Option<PathBuf>,
    tools: Option<PathBuf>,
    tool_name: Option<String>,
    source: Option<PathBuf>,
    input: Option<String>,
    task: Option<String>,
    quiet: bool,
    json: bool,
}

fn parse(command: &str, rest: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = rest.iter();
    let value = |it: &mut std::slice::Iter<String>, flag: &str| {
        it.next().cloned().ok_or_else(|| format!("{flag} 后面要跟一个值"))
    };
    while let Some(arg) = it.next() {
        if command == "describe" && !matches!(arg.as_str(), "--source" | "--tool" | "--snapshot") {
            return Err(format!("describe 不收 {arg}"));
        }
        match arg.as_str() {
            "--config" => a.config = Some(value(&mut it, arg)?.into()),
            "--session" => a.session = Some(value(&mut it, arg)?),
            "--repo" | "--snapshot" => a.repo = Some(value(&mut it, arg)?.into()),
            "--out" => a.out = Some(value(&mut it, arg)?.into()),
            "--tools" => a.tools = Some(value(&mut it, arg)?.into()),
            "--tool" if command == "describe" => a.tool_name = Some(value(&mut it, arg)?),
            "--source" => a.source = Some(value(&mut it, arg)?.into()),
            "--input" => a.input = Some(value(&mut it, arg)?),
            "--quiet" => a.quiet = true,
            "--json" => a.json = true,
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

fn read_source(path: &Path) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("读 {} 失败：{e}", path.display());
        ExitCode::from(2)
    })
}

fn describe(args: &Args) -> ExitCode {
    let Some(path) = args.source.as_deref() else {
        return missing("--source 源码文件");
    };
    if args.repo.is_some() && args.tool_name.is_none() {
        eprintln!("查询绑定目录的工具说明时，请给 --tool 注册名");
        return ExitCode::from(2);
    }
    let source = match read_source(path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let result = host::describe(&source).and_then(|interface| match &args.tool_name {
        Some(name) => host::describe_tool(&interface, name, args.repo.as_deref()),
        None => Ok(interface),
    });
    match result {
        Ok(description) => {
            println!("{description}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
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
    if !matches!(command, "run" | "resume" | "exec" | "describe") {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let a = match parse(command, &args[1..]) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if command == "describe" {
        return describe(&a);
    }
    if a.json && command != "exec" {
        eprintln!("--json 只用于 exec");
        return ExitCode::from(2);
    }
    let resuming = command == "resume";
    let out = a.out.clone();
    if resuming && (a.repo.is_some() || a.task.is_some() || a.tools.is_some() || a.source.is_some() || a.input.is_some()) {
        eprintln!("resume 不收仓库、任务、源码与清单：它们取自这次运行自己的记录\n\n{USAGE}");
        return ExitCode::from(2);
    }

    // 配置：--config，或默认位置。都没有时 exec 用全默认值（它不要模型）；run 与 resume 要模型，说清放哪里。
    let config = a.config.clone().or_else(host::default_config);
    let settings = match (config, command) {
        (Some(config), _) => match host::Settings::load(&config) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("读配置 {} 失败：{e}", config.display());
                return ExitCode::FAILURE;
            }
        },
        (None, "exec") => match host::Settings::from_json(&serde_json::json!({})) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        },
        (None, _) => {
            eprintln!(
                "没有找到配置：用 --config 指定一个，或把它放在 ~/.config/croon/config.json。\n最小的配置只有模型服务：{{\"model\": {{\"provider\": \"/绝对路径/agent-provider.json\"}}}}，见 README「上手」。"
            );
            return ExitCode::from(2);
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
            let recent = host::list_sessions(&sessions);
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
    // resume 的会话要先存在：名字拼错的人该先知道「找不到」，不是先被要求给 --out。
    if resuming && host::session_state(&session) == host::SessionState::Missing {
        let others = host::list_sessions(&sessions);
        let hint = if others.is_empty() {
            "这台机器上还没有别的会话".to_owned()
        } else {
            format!("已有的会话：{}", others.iter().take(8).cloned().collect::<Vec<_>>().join("、"))
        };
        eprintln!("找不到会话 {}：它没有开始过。要开始一次运行用 run；{hint}", session.name);
        return ExitCode::FAILURE;
    }
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

    // --out 到这里才要：前面只是在查询会话，列出或报「找不到」都不需要结果目录。
    let Some(out) = out else {
        return missing("--out 结果目录");
    };
    let result = match command {
        "resume" => host::resume(&settings, assembly, &out),
        "exec" => {
            let (Some(source), Some(input)) = (a.source.clone(), a.input.clone()) else {
                return missing("--source 与 --input");
            };
            let source = match read_source(&source) {
                Ok(s) => s,
                Err(code) => return code,
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
        Ok(finished) if a.json => {
            println!("{}", finished.value);
            ExitCode::SUCCESS
        }
        Ok(finished) => {
            if let Some(summary) = finished.value.get("summary").and_then(serde_json::Value::as_str) {
                println!("{summary}");
            } else {
                println!("{}", finished.value);
            }
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
