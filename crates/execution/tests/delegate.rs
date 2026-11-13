//! The Delegate scenario generates its Library into a temporary root first.
//! This keeps generated modules out of Git while letting `$tree: library` scan the
//! exact files that a real app checkout supplies.

#[path = "../examples/generate_library.rs"]
mod generator;

use kernel::host::Language;
use lang0::Lang0;
use scenarios::{
    drive::Setup,
    scenario::{self, Scn},
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
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
    fs::create_dir_all(root.join("library")).unwrap();
    for name in ["files.py", "react.py", "s.py", "w0.py"] {
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../library")
                .join(name),
            root.join("library").join(name),
        )
        .unwrap();
    }
    generator::generate(root.join("library"));
    Temp(root)
}

fn fixture() -> (Rc<Scn>, Setup, Temp) {
    let root = library_root();
    let mut fixture: serde_json::Value =
        serde_json::from_str(include_str!("../schema/lang0.scenario.delegate.json")).unwrap();
    let mut all = scenario::all();
    all.push(("lang0.scenario.delegate", fixture.clone()));
    let mut values = scenario::values(&all);
    // The shared reconnect root uses deliberately broad fake-provider specs.
    // Adapt only its test Definition to the app-owned full Port specs so the
    // same References can flow into real Lang0 child instances.
    //
    // 这一段依赖 croon 的 scenario.reconnect 里 root 的形状。它变了就让测试直接说出来，
    // 而不是在后面某个毫不相干的断言上失败。
    {
        let mut ports: Vec<_> = values["sha256:droot"]["$bundle"]["interface"]["requires"]
            .as_object()
            .expect("croon 的共享 root 应当有 requires")
            .keys()
            .cloned()
            .collect();
        ports.sort();
        assert_eq!(
            ports,
            ["asks", "back", "construct", "engine", "exec", "instantiate"],
            "croon 的 scenario.reconnect 改了共享 root 的接口，本测试的适配要跟着改"
        );
    }
    for port in ["engine", "exec"] {
        let spec = values["sha256:ls"]["$bundle"]["interface"]["requires"][port].clone();
        values
            .get_mut("sha256:droot")
            .unwrap()
            .get_mut("$bundle")
            .unwrap()
            .get_mut("interface")
            .unwrap()
            .get_mut("requires")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(port.into(), spec.clone());
        fixture["journals"]["i-root"][0]["what"]["birth"]["binding"]["sources"][port]["spec"] =
            spec;
    }
    let setup = Setup::new(&root.0, &values, || {
        vec![Box::new(Lang0) as Box<dyn Language>]
    });
    (
        Rc::new(Scn::of("lang0.scenario.delegate", &fixture)),
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
