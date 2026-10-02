//! The little of sway's IPC the menu needs, through swaymsg.

use std::process::{Command, Stdio};

use serde_json::Value;

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    fn of(v: &Value) -> Rect {
        let n = |k: &str| v[k].as_i64().unwrap_or(0) as i32;
        Rect { x: n("x"), y: n("y"), w: n("width"), h: n("height") }
    }
    pub fn contains(&self, (x, y): (i32, i32)) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
}

pub fn get(kind: &str) -> Option<Value> {
    let out = Command::new("swaymsg").args(["-r", "-t", kind]).stderr(Stdio::null()).output().ok()?;
    serde_json::from_slice(&out.stdout).ok()
}

pub fn run(cmd: &str) -> bool {
    Command::new("swaymsg")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A window, as the menu sees it.
#[derive(Clone, Debug, Default)]
pub struct Win {
    pub id: i64,
    pub pid: i32,
    /// Wayland app_id, or the X11 class.
    pub app_id: String,
    pub title: String,
    pub floating: bool,
    pub fullscreen: bool,
    pub desk: Option<i64>,
    pub output: String,
    pub rect: Rect,
}

fn walk(node: &Value, output: &str, desk: Option<i64>, f: &mut dyn FnMut(&Value, &str, Option<i64>)) {
    let kind = node["type"].as_str().unwrap_or("");
    let output = if kind == "output" { node["name"].as_str().unwrap_or(output) } else { output };
    let desk = if kind == "workspace" { node["num"].as_i64().filter(|n| *n >= 0) } else { desk };
    f(node, output, desk);
    for key in ["nodes", "floating_nodes"] {
        for kid in node[key].as_array().into_iter().flatten() {
            walk(kid, output, desk, f);
        }
    }
}

fn windows(tree: &Value) -> Vec<(Win, bool)> {
    let mut out = Vec::new();
    walk(tree, "", None, &mut |n, output, desk| {
        let kind = n["type"].as_str().unwrap_or("");
        let pid = n["pid"].as_i64().unwrap_or(0) as i32;
        if (kind != "con" && kind != "floating_con") || pid <= 0 {
            return;
        }
        let app_id = n["app_id"]
            .as_str()
            .or_else(|| n["window_properties"]["class"].as_str())
            .unwrap_or("")
            .to_string();
        let win = Win {
            id: n["id"].as_i64().unwrap_or(0),
            pid,
            app_id,
            title: n["name"].as_str().unwrap_or("").to_string(),
            floating: kind == "floating_con",
            fullscreen: n["fullscreen_mode"].as_i64().unwrap_or(0) > 0,
            desk,
            output: output.to_string(),
            rect: Rect::of(&n["rect"]),
        };
        out.push((win, n["focused"].as_bool().unwrap_or(false)));
    });
    out
}

pub fn focused_window(tree: &Value) -> Option<Win> {
    windows(tree).into_iter().find(|(_, f)| *f).map(|(w, _)| w)
}

pub fn window_of_pid(tree: &Value, pid: i32) -> Option<Win> {
    windows(tree).into_iter().map(|(w, _)| w).find(|w| w.pid == pid)
}

#[derive(Clone, Debug)]
pub struct Output {
    pub name: String,
    pub rect: Rect,
    /// The part windows may use: the output minus its bars.
    pub usable: Rect,
}

pub fn outputs() -> Vec<Output> {
    let desks = get("get_workspaces").unwrap_or_default();
    let mut outs: Vec<Output> = get("get_outputs")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter(|o| o["active"].as_bool().unwrap_or(false))
        .map(|o| {
            let name = o["name"].as_str().unwrap_or("").to_string();
            let rect = Rect::of(&o["rect"]);
            let usable = desks
                .as_array()
                .into_iter()
                .flatten()
                .find(|d| d["visible"].as_bool() == Some(true) && d["output"].as_str() == Some(&name))
                .map(|d| Rect::of(&d["rect"]))
                .filter(|r| r.w > 0 && r.h > 0)
                .unwrap_or(rect);
            Output { name, rect, usable }
        })
        .collect();
    outs.sort_by_key(|o| (o.rect.x, o.rect.y));
    outs
}
