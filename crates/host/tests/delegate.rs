//! Delegate 场景：预期是 block `library.scenario.delegate`，它检验什么见 docs/Library/Scenarios/Delegate.md。Library 先生成进临时根目录，
//! 生成模块不进 Git，`$tree: library` 扫到的仍是真实检出提供的那些文件。

use kernel::host::{Channel, Language};
use lang0::Lang0;
use scenarios::{
    drive::{ProviderRole, Setup},
    scenario::{self, Scn},
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn library_root() -> Temp {
    let root = std::env::temp_dir().join(format!(
        "croon-delegate-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    host::library::install(root.join("library"));
    Temp(root)
}

fn fixture() -> (Rc<Scn>, Setup, Temp) {
    let root = library_root();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../schema/library.scenario.delegate.json")).unwrap();
    let mut all = scenario::all();
    all.push(("library.scenario.delegate", fixture.clone()));
    // root entry 用 app 自己的 host.root（场景里的 sha256:lroot），不借 croon 共享场景的 root。
    let values = scenario::values(&all);
    let mut setup = Setup::new(&root.0, &values, || {
        vec![Box::new(Lang0) as Box<dyn Language>]
    });
    // host.root 多接了 definition.infer：它是无世界效果的宿主 Port，场景里只占一个假身份。
    setup
        .channels
        .insert("fake-infer".into(), Channel::Discardable);
    setup
        .provider_roles
        .insert("fake-infer".into(), ProviderRole::Provider);
    (
        Rc::new(Scn::of("library.scenario.delegate", &fixture)),
        setup,
        root,
    )
}

#[test]
fn delegate_runs_with_real_lang0() {
    let (scn, setup, _root) = fixture();
    setup.run(&scn, 0, None);
}

#[test]
fn delegate_has_the_same_journals_in_compatible_orders() {
    let (scn, setup, _root) = fixture();
    for seed in 1..=8 {
        setup.run(&scn, seed, None);
    }
}

#[test]
fn delegate_recovers_at_every_commit() {
    let (scn, setup, _root) = fixture();
    let baseline = setup.run(&scn, 0, None);
    let commits = baseline.faults.borrow().commits;
    for n in 1..=commits {
        for after in [false, true] {
            let run = setup.run(&scn, 0, Some((n, after)));
            assert!(run.crashes > 0);
        }
    }
}

#[test]
fn delegate_recovers_with_every_provider_atom_in_flight() {
    use kernel::index::what;

    let (scn, setup, _root) = fixture();
    let baseline = setup.run(&scn, 0, None);
    let mut keys = Vec::new();
    for (instance, journal) in &scn.journals {
        let definition = what(&journal[0]).1["definition"].as_str().unwrap();
        if setup.values[definition]["$bundle"]["language"] != "lang0@1" {
            continue;
        }
        for record in journal {
            let (kind, body) = what(record);
            if kind == "send" && baseline.to_provider(instance, body) {
                keys.push((instance.clone(), body["key"].as_str().unwrap().to_owned()));
            }
        }
    }
    assert!(!keys.is_empty());
    for (instance, key) in &keys {
        let mut perturbed = Scn {
            name: scn.name.clone(),
            journals: scn.journals.clone(),
            pending: scn.pending.clone(),
            order: scn.order.clone(),
            perturb: scn.perturb.clone(),
        };
        perturbed
            .perturb
            .push(json!({"crash":{"instance":instance,"key":key}}));
        let run = setup.run(&Rc::new(perturbed), 0, None);
        assert_eq!(
            run.crashes, 1,
            "{instance} 的 {key} 必须在结果提交前崩溃一次"
        );
    }
}
