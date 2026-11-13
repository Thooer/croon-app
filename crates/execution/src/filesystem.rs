//! Linux FD 锚定的目录导入和物化。
//! 参考：docs/Directory/README.md（Tree、实现）。

use crate::fd as fs;
use croon_eval::host::HostError;
use croon_port::store::Content;
use croon_tree::{Entry, Tree, error, required, valid_name};
use serde_json::{Value as Json, json};
use std::fs::File;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// 按 ID 读一个目录 Bundle 并校验它的 Tree。读取范围由调用者的 `Content` 决定。
pub fn read(content: &Content, id: &str) -> Result<Tree, HostError> {
    let bundle = required(content.bundle(id)?, id)?;
    croon_tree::validate(
        &bundle,
        &required(content.json(&bundle.content)?, &bundle.content)?,
    )
}

fn file_bytes(content: &Content, id: &str) -> Result<Vec<u8>, HostError> {
    let bundle = required(content.bundle(id)?, id)?;
    if !bundle.dependencies.is_empty() {
        return Err(error("file Bundle has dependencies"));
    }
    required(content.bytes(&bundle.content)?, &bundle.content)
}
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub nodes: usize,
    pub bytes: usize,
}
struct Budget {
    nodes: usize,
    bytes: usize,
}
impl Budget {
    fn node(&mut self) -> Result<(), HostError> {
        self.nodes = self
            .nodes
            .checked_sub(1)
            .ok_or_else(|| error("directory node budget exhausted"))?;
        Ok(())
    }
    fn bytes(&mut self, n: usize) -> Result<(), HostError> {
        self.bytes = self
            .bytes
            .checked_sub(n)
            .ok_or_else(|| error("directory byte budget exhausted"))?;
        Ok(())
    }
}
/// 目标必须是空目录；FD 锚定的写入不穿过已存在或并发替换的链接。
pub fn materialize(
    content: &Content,
    id: &str,
    destination: &Path,
    limits: Limits,
) -> Result<(), HostError> {
    let root = fs::root(destination).map_err(error)?;
    if !fs::names(&root).map_err(error)?.is_empty() {
        return Err(error("materialize destination is not empty"));
    }
    let mut budget = Budget {
        nodes: limits.nodes,
        bytes: limits.bytes,
    };
    let mut pending = vec![(root, id.to_owned())];
    while let Some((dir, id)) = pending.pop() {
        budget.node()?;
        for (name, entry) in read(content, &id)? {
            budget.node()?;
            match entry {
                Entry::Directory { bundle } => {
                    pending.push((fs::mkdir(&dir, &name).map_err(error)?, bundle))
                }
                Entry::File { bundle, executable } => {
                    let bytes = file_bytes(content, &bundle)?;
                    budget.bytes(bytes.len())?;
                    fs::write(&dir, &name, &bytes, executable).map_err(error)?;
                }
                Entry::Link { target } => fs::symlink(&dir, &name, &target).map_err(error)?,
            }
        }
    }
    Ok(())
}
/// 来源应已停止写入；通过 lstat 分类，openat(O_NOFOLLOW) 读取普通文件。
pub fn import(content: &Content, source: &Path, limits: Limits) -> Result<String, HostError> {
    struct Frame {
        dir: File,
        names: Vec<String>,
        tree: serde_json::Map<String, Json>,
        dependencies: Vec<String>,
        parent_name: String,
    }
    fn frame(dir: File, parent_name: String) -> Result<Frame, HostError> {
        let mut names = fs::names(&dir).map_err(error)?;
        names.sort();
        Ok(Frame {
            dir,
            names,
            tree: Default::default(),
            dependencies: vec![],
            parent_name,
        })
    }
    let mut budget = Budget {
        nodes: limits.nodes,
        bytes: limits.bytes,
    };
    let mut stack = vec![frame(fs::root(source).map_err(error)?, String::new())?];
    while let Some(current) = stack.last_mut() {
        if let Some(name) = current.names.pop() {
            budget.node()?;
            if !valid_name(&name) {
                return Err(error("invalid imported name"));
            }
            let meta = fs::metadata(&current.dir, &name).map_err(error)?;
            if meta.file_type().is_symlink() {
                current.tree.insert(
                    name.clone(),
                    json!({"link":fs::link(&current.dir,&name).map_err(error)?}),
                );
            } else if meta.is_dir() {
                let dir = fs::open(&current.dir, &name, true).map_err(error)?;
                stack.push(frame(dir, name)?);
            } else if meta.is_file() {
                let file = fs::open(&current.dir, &name, false).map_err(error)?;
                let executable = file.metadata().map_err(error)?.permissions().mode() & 0o111 != 0;
                let bytes = fs::bytes(file, budget.bytes).map_err(error)?;
                budget.bytes(bytes.len())?;
                let raw = content.put_bytes(&bytes)?;
                let id = content.pack(&raw, &[])?;
                current.dependencies.push(id.clone());
                current.tree.insert(name, json!({"file":id,"x":executable}));
            } else {
                return Err(error("unsupported filesystem object"));
            }
        } else {
            budget.node()?;
            let current = stack.pop().unwrap();
            let raw = content.put_json(&Json::Object(current.tree))?;
            let id = content.pack(&raw, &current.dependencies)?;
            if let Some(parent) = stack.last_mut() {
                parent.dependencies.push(id.clone());
                parent.tree.insert(current.parent_name, json!({"dir":id}));
            } else {
                return Ok(id);
            }
        }
    }
    unreachable!()
}
