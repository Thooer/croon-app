//! 交办的端到端：脚本化模型、真的 Docker，W0 交办给 S，审过再并入。
//! 快路径（W0 没动过就采用）与目录级三方合并（W0 自己也改过）各跑一遍，每遍三次：
//! 脚本化模型加录制；另一个系统只回放录像，结果相同；原 Kernel 目录配空录像，结果来自 Journal。
//! 录像只在临时目录里，不提交，所以不会随镜像或工具说明过期。
//! 参考：docs/Library/Runtime.md（工作区、交办）；docs/Host/README.md（运行参数）。

mod support;

use croon_port::Port;
use std::fs;
use std::rc::Rc;
use support::{Scripted, Temp};

fn delegation(three_way: bool) {
    let temp = Temp::new();
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.py"), "broken\n").unwrap();
    let task = "修好 a.py";

    // 第一次：脚本化模型，边跑边录。配置里的回放来源不会被用到。
    let first = temp.system("record");
    let cassette = temp.root.join("cassette.json");
    let unused = temp.root.join("unused.json");
    let settings = host::Settings::load(&temp.config(&first, &unused)).unwrap();
    let scripted = Rc::new(Scripted {
        identity: format!("{first}/model"),
        three_way,
    });
    let recorder: Rc<dyn Port> = Rc::new(model::Recorder::new(scripted, &cassette).unwrap());
    let assembly = host::Assembly {
        model: Some(recorder),
        fault: None,
        tools: None,
        session: None,
        quiet: true,
    };
    let out1 = temp.root.join("out1");
    let recorded = host::run(&settings, assembly, &repo, task, &out1).unwrap();
    assert_eq!(recorded.value["summary"], "已修好 a.py");
    assert_eq!(fs::read_to_string(out1.join("a.py")).unwrap(), "fixed\n");
    if three_way {
        assert_eq!(fs::read_to_string(out1.join("o.txt")).unwrap(), "ours\n");
    } else {
        assert!(!out1.join("o.txt").exists());
    }
    assert_eq!(
        fs::read_to_string(repo.join("a.py")).unwrap(),
        "broken\n",
        "用户仓库不动"
    );

    // 第二次：另一个系统、新的 Kernel 目录，只回放。真 git 输出与产物快照都确定，结果逐字相同。
    let second = temp.system("replay");
    let settings = host::Settings::load(&temp.config(&second, &cassette)).unwrap();
    let out2 = temp.root.join("out2");
    let replayed = host::run(&settings, host::Assembly::default(), &repo, task, &out2).unwrap();
    assert_eq!(replayed.snapshot, recorded.snapshot);
    assert_eq!(replayed.value, recorded.value);

    // 第三次：第一个系统的 Kernel 目录配空录像，任何一次模型调用都会失败，结果只能来自 Journal。
    let empty = temp.root.join("empty.json");
    fs::write(&empty, "{\"entries\":[]}").unwrap();
    let settings = host::Settings::load(&temp.config(&first, &empty)).unwrap();
    let out3 = temp.root.join("out3");
    let again = host::resume(&settings, host::Assembly::default(), &out3).unwrap();
    assert_eq!(again.snapshot, recorded.snapshot);
}

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
fn delegation_takes_the_candidate_when_w0_has_not_moved() {
    delegation(false);
}

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
fn delegation_merges_by_directory_when_w0_has_moved() {
    delegation(true);
}
