//! 宿主层的崩溃恢复，提交窗口：在每个事务提交前后让 Kernel 停下，同一个系统重跑，
//! 结果快照与不崩时相同，各 Instance 的记录序列相同，每个逻辑行动只提交一次结果。
//! 不同系统的密钥不同，Instance ID 与 Handle 比不了，所以记录按 Definition 归组、只比种类与键。
//! 参考：docs/Host/README.md（客户端、运行参数）。

mod support;

use croon_port::Port;
use croon_port::store::Budgets;
use host::{Assembly, Fault, FaultHook};
use kernel::host::Config;
use kernel::index::what;
use kernel::kernel::Kernel;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::rc::Rc;
use support::{Making, Scripted, Temp};

/// 只数提交，不让 Kernel 停下。
fn counter(count: Rc<Cell<usize>>) -> FaultHook {
    Box::new(move |fault| {
        if matches!(fault, Fault::BeforeCommit { .. }) {
            count.set(count.get() + 1);
        }
        false
    })
}

/// 在第 n 个事务提交之前，或提交之后，让 Kernel 停下。
fn crash_at(n: usize, after: bool) -> FaultHook {
    let mut seen = 0;
    Box::new(move |fault| match fault {
        Fault::BeforeCommit { .. } => {
            seen += 1;
            seen == n && !after
        }
        Fault::AfterCommit { .. } => seen == n && after,
    })
}

/// 每份 Definition 的各 Instance 的记录序列：种类，send 与 receive 带键。
/// 一本 Journal 里同一个键只出现一次：每个逻辑行动只提交一次结果。
fn summary(dir: &Path) -> BTreeMap<String, Vec<Vec<String>>> {
    let kernel = Kernel::open(
        dir,
        Config {
            languages: Vec::new(),
            budgets: Budgets {
                nodes: 10_000,
                read_bytes: 1 << 20,
                write_bytes: 1 << 20,
            },
            ports: BTreeMap::new(),
            clock: Box::new(String::new),
            fault: None,
        },
    )
    .unwrap();
    let mut out: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
    for (instance, journal) in &kernel.index().journals {
        let mut keys = BTreeSet::new();
        let sequence = journal
            .records
            .iter()
            .map(|record| {
                let (kind, body) = what(record);
                match body["key"].as_str() {
                    Some(key) if kind == "send" || kind == "receive" => {
                        assert!(
                            keys.insert(key.to_owned()),
                            "{instance} 的 {key} 提交了两次"
                        );
                        format!("{kind}:{key}")
                    }
                    _ => kind.to_string(),
                }
            })
            .collect();
        let definition = journal.birth()["definition"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        out.entry(definition).or_default().push(sequence);
    }
    for sequences in out.values_mut() {
        sequences.sort();
    }
    out
}

/// 逐提交点崩溃再恢复；`make` 造这一轮的脚本化模型。
fn recovers_at_every_commit(make: impl Fn(String) -> Rc<dyn Port>, task: &str) {
    let temp = Temp::new();
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.py"), "broken\n").unwrap();

    // 基线：脚本化模型边跑边录，数出提交的个数。
    let first = temp.system("record");
    let cassette = temp.root.join("cassette.json");
    let settings =
        host::Settings::load(&temp.config(&first, &temp.root.join("unused.json"))).unwrap();
    let recorder: Rc<dyn Port> =
        Rc::new(model::Recorder::new(make(format!("{first}/model")), &cassette).unwrap());
    let commits = Rc::new(Cell::new(0));
    let assembly = Assembly {
        model: Some(recorder),
        fault: Some(counter(commits.clone())),
        tools: None,
    };
    let baseline = host::run(
        &settings,
        assembly,
        &repo,
        task,
        &temp.root.join("baseline"),
    )
    .unwrap();
    let expected = summary(&temp.kernel(&first));
    assert!(commits.get() > 0);

    for n in 1..=commits.get() {
        for after in [false, true] {
            let label = format!("c{n}{}", if after { "after" } else { "before" });
            let system = temp.system(&label);
            let settings = host::Settings::load(&temp.config(&system, &cassette)).unwrap();
            let assembly = Assembly {
                model: None,
                fault: Some(crash_at(n, after)),
                tools: None,
            };
            let crashed = host::run(
                &settings,
                assembly,
                &repo,
                task,
                &temp.root.join(format!("{label}-crashed")),
            );
            assert!(crashed.is_err(), "{label}：Kernel 应当在这里停下");
            // 磁盘在两次之间变了也不要紧：恢复用的是 Journal 里记下的输入，不读仓库。
            let moved = temp.root.join("repo-moved");
            let _ = fs::remove_dir_all(&moved);
            fs::rename(&repo, &moved).unwrap();
            let done = host::resume(
                &settings,
                Assembly::default(),
                &temp.root.join(format!("{label}-out")),
            )
            .map(|d| {
                fs::rename(&moved, &repo).unwrap();
                d
            })
            .unwrap_or_else(|e| panic!("{label}：恢复失败：{e}"));
            assert_eq!(done.snapshot, baseline.snapshot, "{label}");
            assert_eq!(done.summary, baseline.summary, "{label}");
            assert_eq!(summary(&temp.kernel(&system)), expected, "{label}");
        }
    }
}

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim; runs the task once per commit point"]
fn the_host_recovers_at_every_commit_with_the_same_result() {
    recovers_at_every_commit(
        |identity| {
            Rc::new(Scripted {
                identity,
                three_way: true,
            })
        },
        "修好 a.py",
    );
}

/// 造出来的主体：源码是模型写的，infer、pack、construct、instantiate、收取每一步前后停下都能恢复。
#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim; runs the task once per commit point"]
fn the_host_recovers_at_every_commit_when_the_agent_is_made_on_the_way() {
    recovers_at_every_commit(
        |identity| {
            Rc::new(Making {
                identity,
                slow: false,
            })
        },
        "造一个 Agent 修好 a.py",
    );
}
