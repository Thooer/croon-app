//! 配置与会话：最小配置只写模型服务，其余有默认值；一次运行的身份与目录由会话名派生；会话名要能放进容器名与路径。
//! 不需要 Docker，也不调用模型。
//! 参考：docs/Host/README.md（配置与会话）。

use host::{ModelSource, Settings};
use serde_json::json;
use std::path::Path;

fn minimal() -> Settings {
    Settings::from_json(&json!({"model": {"provider": "/p/agent-provider.json"}})).unwrap()
}

#[test]
fn a_config_with_only_a_model_gets_every_default() {
    let s = minimal();
    assert!(matches!(&s.model, ModelSource::Provider(p) if p == Path::new("/p/agent-provider.json")));
    assert!(s.system.is_none() && s.kernel.is_none() && s.directory.state.is_none());
    let d = &s.directory;
    assert_eq!(d.image, "croon-work:2");
    assert_eq!(d.collector_image, "python:3.12-slim");
    assert_eq!(d.platform, "linux/amd64");
    assert_eq!(d.cpus, "1");
    assert_eq!(d.memory_bytes, 512 << 20);
    assert_eq!(d.work_bytes, 64 << 20);
    assert_eq!(d.pids, 128);
    assert_eq!(d.cleanup_timeout.as_secs(), 60);
    assert!(d.docker.is_absolute() && d.python.is_absolute(), "docker 与 python 取自 PATH 的绝对路径");
}

#[test]
fn what_the_config_writes_wins_over_the_default_and_a_wrong_value_is_an_error() {
    let s = Settings::from_json(&json!({
        "model": {"provider": "/p/a.json"},
        "directory": {"image": "mine:1", "cpus": "2", "pids": 7}
    }))
    .unwrap();
    assert_eq!((s.directory.image.as_str(), s.directory.cpus.as_str(), s.directory.pids), ("mine:1", "2", 7));
    assert_eq!(s.directory.memory_bytes, 512 << 20, "没写的仍是默认");

    let error = Settings::from_json(&json!({"model": {"provider": "/p/a.json"}, "directory": {"pids": -1}}))
        .err()
        .expect("写错了要报错，不悄悄用默认");
    assert!(error.0.contains("pids"), "错误要点名是哪个字段：{error}");
    let relative = Settings::from_json(&json!({"model": {"provider": "relative.json"}})).err().unwrap();
    assert!(relative.0.contains("绝对路径"), "{relative}");
    let no_model = Settings::from_json(&json!({})).err().unwrap();
    assert!(no_model.0.contains("host.config"), "{no_model}");
}

#[test]
fn a_session_name_decides_where_a_run_lives() {
    let s = minimal();
    let sessions = Path::new("/state/croon/sessions");
    let session = s.session(Some("fix-add"), sessions).unwrap();
    assert_eq!(session.name, "fix-add");
    assert_eq!(session.kernel, Path::new("/state/croon/sessions/fix-add/kernel"));
    assert_eq!(session.state, Path::new("/state/croon/sessions/fix-add/state"));
    assert_eq!(session.cassette, Path::new("/state/croon/sessions/fix-add/cassette.json"));
    let other = s.session(Some("other"), sessions).unwrap();
    assert_ne!(session.kernel, other.kernel, "不同会话不共用目录");
}

#[test]
fn the_old_spelling_still_decides_the_paths_itself() {
    // 测试与实验脚本要自己控制每个目录：配置里写了 system、kernel、state、record 就按它们办。
    let s = Settings::from_json(&json!({
        "system": "legacy", "kernel": "/k", 
        "model": {"provider": "/p/a.json", "record": "/r/tape.json"},
        "directory": {"state": "/s"}
    }))
    .unwrap();
    let session = s.session(None, Path::new("/ignored")).unwrap();
    assert_eq!(session.name, "legacy");
    assert_eq!(session.kernel, Path::new("/k"));
    assert_eq!(session.state, Path::new("/s"));
    assert_eq!(session.cassette, Path::new("/r/tape.json"));
}

#[test]
fn no_name_anywhere_is_a_clear_error() {
    let error = minimal().session(None, Path::new("/s")).err().unwrap();
    assert!(error.0.contains("会话名"), "{error}");
}

#[test]
fn a_session_name_has_to_fit_in_a_container_name_and_a_path() {
    for good in ["a", "fix-add", "20260101-120000-ab12cd", "v1.2_x"] {
        assert!(host::valid_session_name(good), "{good}");
    }
    for bad in ["", ".hidden", "a/b", "a b", "a:b", "../x", "中文", "a\n"] {
        assert!(!host::valid_session_name(bad), "{bad:?}");
        let error = minimal().session(Some(bad), Path::new("/s")).err().unwrap();
        assert!(error.0.contains("不合法"), "{bad:?}: {error}");
    }
}

#[test]
fn record_true_puts_the_tape_in_the_session_and_a_path_puts_it_there() {
    let on = Settings::from_json(&json!({"model": {"provider": "/p/a.json", "record": true}})).unwrap();
    let session = on.session(Some("x"), Path::new("/s")).unwrap();
    assert_eq!(session.cassette, Path::new("/s/x/cassette.json"));
    assert!(matches!(on.model, ModelSource::Record { record: None, .. }));

    let at = Settings::from_json(&json!({"model": {"provider": "/p/a.json", "record": "/r/t.json"}})).unwrap();
    assert_eq!(at.session(Some("x"), Path::new("/s")).unwrap().cassette, Path::new("/r/t.json"));

    let off = Settings::from_json(&json!({"model": {"provider": "/p/a.json", "record": false}}));
    assert!(off.is_err(), "false 不是合法的写法：不想录就不写 record");
}
