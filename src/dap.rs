//! `gw --dap`: a Debug Adapter Protocol server on standard input and output,
//! so editors (VS Code first) can debug Globw programs: breakpoints, stepping,
//! the call stack, and the variables of every frame.
//!
//! Two threads: this one reads the editor's requests; a worker compiles and runs
//! the program (the compiler and the VM must stay on one thread). The worker's
//! [`DebugHook`] decides where to stop; when it stops it leaves a snapshot of the
//! stack and variables for this thread to answer from, then waits for the next
//! command (continue, step, terminate).

use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use globw::debug::{DebugAction, DebugHook, DebugState, DebugVar};
use serde_json::{json, Value as Json};

/// Writes protocol messages (responses and events) to standard output.
struct Writer {
    seq: Mutex<i64>,
}

impl Writer {
    fn send(&self, mut msg: Json) {
        let mut seq = self.seq.lock().unwrap_or_else(|e| e.into_inner());
        *seq += 1;
        msg["seq"] = json!(*seq);
        let body = msg.to_string();
        let mut out = io::stdout().lock();
        let _ = write!(out, "Content-Length: {}\r\n\r\n{}", body.len(), body);
        let _ = out.flush();
    }

    fn event(&self, event: &str, body: Json) {
        self.send(json!({"type": "event", "event": event, "body": body}));
    }

    fn respond(&self, req: &Json, body: Json) {
        self.send(json!({
            "type": "response", "request_seq": req["seq"], "success": true,
            "command": req["command"], "body": body,
        }));
    }

    fn fail(&self, req: &Json, message: &str) {
        self.send(json!({
            "type": "response", "request_seq": req["seq"], "success": false,
            "command": req["command"], "message": message,
        }));
    }

    fn output(&self, category: &str, text: &str) {
        self.event("output", json!({"category": category, "output": text}));
    }
}

/// The program's output, sent to the editor's debug console.
struct ConsoleOut(Arc<Writer>);

impl Write for ConsoleOut {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !buf.is_empty() {
            self.0.output("stdout", &String::from_utf8_lossy(buf));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A path in a form that compares equal however the editor spells it.
fn norm(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    // a canonical Windows path may start with \\?\
    let s = s.strip_prefix("//?/").map(str::to_string).unwrap_or(s);
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Run,
    StepOver,
    StepIn,
    StepOut,
}

enum Cmd {
    /// configuration is done: start the program
    Start,
    Resume(Mode),
    Terminate,
}

/// A variable as the editor shows it: `reference` > 0 when it has parts.
#[derive(Clone)]
struct ShownVar {
    name: String,
    value: String,
    kind: String,
    reference: i64,
}

struct ShownFrame {
    name: String,
    path: String,
    line: u32,
    col: u32,
    locals: i64,
}

/// What the editor can ask about while the program is stopped.
#[derive(Default)]
struct Snapshot {
    frames: Vec<ShownFrame>,
    globals: i64,
    vars: HashMap<i64, Vec<ShownVar>>,
}

impl Snapshot {
    /// Registers a list of variables (and, recursively, their parts); returns its reference.
    fn add(&mut self, vars: &[DebugVar]) -> i64 {
        let id = self.vars.len() as i64 + 1;
        self.vars.insert(id, Vec::new());
        let shown = vars
            .iter()
            .map(|v| ShownVar {
                name: v.name.clone(),
                value: v.value.clone(),
                kind: v.kind.clone(),
                reference: if v.children.is_empty() { 0 } else { self.add(&v.children) },
            })
            .collect();
        self.vars.insert(id, shown);
        id
    }
}

struct Shared {
    out: Arc<Writer>,
    /// breakpoint lines by normalized file path
    breakpoints: Mutex<HashMap<String, HashSet<u32>>>,
    pause: AtomicBool,
    terminate: AtomicBool,
    snapshot: Mutex<Option<Snapshot>>,
}

struct Hook {
    shared: Arc<Shared>,
    cmds: Receiver<Cmd>,
    /// the normalized path of each source file of the program
    paths: Vec<String>,
    mode: Mode,
    stop_on_entry: bool,
    /// where the last stop was: (depth, file, line)
    from: (usize, u16, u32),
}

impl DebugHook for Hook {
    fn on_line(&mut self, st: &DebugState) -> DebugAction {
        if self.shared.terminate.load(Ordering::SeqCst) {
            return DebugAction::Terminate;
        }
        let (depth, file, line) = (st.depth, st.file(), st.line());
        let (d0, f0, l0) = self.from;
        let mut reason = if std::mem::take(&mut self.stop_on_entry) {
            Some("entry")
        } else if self.shared.pause.swap(false, Ordering::SeqCst) {
            Some("pause")
        } else {
            match self.mode {
                Mode::Run => None,
                Mode::StepIn => Some("step"),
                // the next line of this function, or the caller when it returns
                Mode::StepOver => {
                    let moved = !st.returned && (line != l0 || file != f0);
                    (depth < d0 || (depth == d0 && moved)).then_some("step")
                }
                Mode::StepOut => (depth < d0).then_some("step"),
            }
        };
        if reason.is_none() && !st.returned {
            let path = self.paths.get(file as usize).cloned().unwrap_or_default();
            let bps = self.shared.breakpoints.lock().unwrap_or_else(|e| e.into_inner());
            if bps.get(&path).is_some_and(|lines| lines.contains(&line)) {
                reason = Some("breakpoint");
            }
        }
        let Some(reason) = reason else {
            return DebugAction::Continue;
        };
        self.stop(st, json!({"reason": reason, "threadId": 1, "allThreadsStopped": true}))
    }

    fn on_error(&mut self, st: &DebugState, message: &str) {
        if self.shared.terminate.load(Ordering::SeqCst) {
            return;
        }
        // stop on the failing line, so its variables can be inspected
        let body = json!({
            "reason": "exception", "threadId": 1, "allThreadsStopped": true,
            "description": "Run-time error", "text": message,
        });
        let _ = self.stop(st, body);
    }
}

impl Hook {
    /// Stops: leaves a snapshot for the editor's questions, tells the editor,
    /// and waits for a command.
    fn stop(&mut self, st: &DebugState, stopped: Json) -> DebugAction {
        let (depth, file, line) = (st.depth, st.file(), st.line());
        let mut snap = Snapshot::default();
        for f in st.frames() {
            let locals = snap.add(&f.locals);
            let path = self.paths.get(f.file as usize).cloned().unwrap_or_default();
            snap.frames.push(ShownFrame { name: f.name, path, line: f.line, col: f.col, locals });
        }
        snap.globals = snap.add(&st.globals());
        *self.shared.snapshot.lock().unwrap_or_else(|e| e.into_inner()) = Some(snap);
        self.shared.out.event("stopped", stopped);
        let action = match self.cmds.recv() {
            Ok(Cmd::Resume(mode)) => {
                self.mode = mode;
                self.from = (depth, file, line);
                DebugAction::Continue
            }
            Ok(Cmd::Start) => DebugAction::Continue,
            Ok(Cmd::Terminate) | Err(_) => DebugAction::Terminate,
        };
        *self.shared.snapshot.lock().unwrap_or_else(|e| e.into_inner()) = None;
        action
    }
}

/// Compiles and runs the program on this (worker) thread.
fn run_program(shared: Arc<Shared>, cmds: Receiver<Cmd>, args: Json) {
    let out = shared.out.clone();
    let finish = |code: i32| {
        out.event("exited", json!({"exitCode": code}));
        out.event("terminated", json!({}));
    };
    let Some(program) = args["program"].as_str() else {
        out.output("stderr", "launch: \"program\" is missing (the .gw file to debug)\n");
        return finish(3);
    };
    let program = std::path::absolute(program).unwrap_or_else(|_| PathBuf::from(program));
    // permissions, as on the command line: "args": ["--allow-read", ...]
    let mut caps = globw::library::Caps::none();
    for a in args["args"].as_array().into_iter().flatten().filter_map(Json::as_str) {
        let (key, val) = match a.split_once('=') {
            Some((k, v)) => (k, Some(v)),
            None => (a, None),
        };
        match key {
            "--allow-run" => caps.allow_run(),
            "--allow-net" => caps.allow_net(val),
            "--allow-read" => caps.allow_read(val),
            "--allow-write" => caps.allow_write(val),
            "--allow-all" => caps.allow_all(),
            _ => out.output("console", &format!("ignored option: {}\n", a)),
        }
    }
    globw::library::set_caps(caps);
    let source = match std::fs::read_to_string(&program) {
        Ok(s) => s,
        Err(e) => {
            out.output("stderr", &format!("cannot read \"{}\": {}\n", program.display(), e));
            return finish(3);
        }
    };
    let dir = program.parent().map(Path::to_path_buf).unwrap_or_default();
    let _ = std::env::set_current_dir(&dir);
    let name = program.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let (files, compiled) = globw::compile_program(globw::SourceFile { name, text: source }, Path::new("."));
    let module = match compiled {
        Ok(m) => m,
        Err(errors) => {
            let text: Vec<String> = errors.iter().map(|e| e.render_in(&files)).collect();
            out.output("stderr", &format!("{}\n", text.join("\n")));
            return finish(1);
        }
    };
    let paths: Vec<String> = files.iter().map(|f| norm(&dir.join(&f.name))).collect();
    // wait for the editor to send its breakpoints
    loop {
        match cmds.recv() {
            Ok(Cmd::Start) => break,
            Ok(Cmd::Resume(_)) => {}
            Ok(Cmd::Terminate) | Err(_) => return finish(0),
        }
    }
    let hook = Hook {
        shared: shared.clone(),
        cmds,
        paths,
        mode: Mode::Run,
        stop_on_entry: args["stopOnEntry"].as_bool().unwrap_or(false),
        from: (0, 0, 0),
    };
    let text = args["input"].as_str().unwrap_or("").to_string();
    let mut input = io::Cursor::new(text.into_bytes());
    let mut console = ConsoleOut(out.clone());
    match globw::run_debug(&module, &mut input, &mut console, Box::new(hook)) {
        Ok(()) => finish(0),
        Err(e) => {
            out.output("stderr", &e.render_in(&files));
            finish(2)
        }
    }
}

/// Reads one protocol message (`Content-Length` header, blank line, JSON body).
fn read_message(input: &mut impl BufRead) -> Option<Json> {
    let mut len = None;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim();
        if line.is_empty() {
            if len.is_some() {
                break;
            }
            continue;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            len = v.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0u8; len?];
    input.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

/// The `gw --dap` main loop: answers the editor until it disconnects.
pub fn serve() -> std::process::ExitCode {
    let out = Arc::new(Writer { seq: Mutex::new(0) });
    let shared = Arc::new(Shared {
        out: out.clone(),
        breakpoints: Mutex::new(HashMap::new()),
        pause: AtomicBool::new(false),
        terminate: AtomicBool::new(false),
        snapshot: Mutex::new(None),
    });
    let (tx, rx): (Sender<Cmd>, Receiver<Cmd>) = channel();
    let mut rx = Some(rx);
    let stdin = io::stdin();
    let mut input = stdin.lock();
    while let Some(req) = read_message(&mut input) {
        let command = req["command"].as_str().unwrap_or("").to_string();
        let a = &req["arguments"];
        let snapshot = || shared.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        match command.as_str() {
            "initialize" => {
                out.respond(
                    &req,
                    json!({
                        "supportsConfigurationDoneRequest": true,
                        "supportsTerminateRequest": true,
                        "supportsEvaluateForHovers": true,
                    }),
                );
                out.event("initialized", json!({}));
            }
            "launch" => match rx.take() {
                Some(cmds) => {
                    let (sh, args) = (shared.clone(), a.clone());
                    // a large stack: deep recursion in the debugged program
                    let spawned = std::thread::Builder::new()
                        .name("globw-program".into())
                        .stack_size(256 << 20)
                        .spawn(move || run_program(sh, cmds, args));
                    match spawned {
                        Ok(_) => out.respond(&req, json!({})),
                        Err(e) => out.fail(&req, &format!("cannot start the program: {}", e)),
                    }
                }
                None => out.fail(&req, "the program is already running"),
            },
            "setBreakpoints" => {
                let path = a["source"]["path"].as_str().map(|p| norm(Path::new(p))).unwrap_or_default();
                let lines: Vec<u32> = a["breakpoints"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|b| b["line"].as_u64())
                    .map(|l| l as u32)
                    .collect();
                shared.breakpoints.lock().unwrap_or_else(|e| e.into_inner()).insert(path, lines.iter().copied().collect());
                let shown: Vec<Json> = lines.iter().map(|l| json!({"verified": true, "line": l})).collect();
                out.respond(&req, json!({"breakpoints": shown}));
            }
            "configurationDone" => {
                let _ = tx.send(Cmd::Start);
                out.respond(&req, json!({}));
            }
            "threads" => out.respond(&req, json!({"threads": [{"id": 1, "name": "main"}]})),
            "stackTrace" => {
                let snap = snapshot();
                let frames: Vec<Json> = snap
                    .as_ref()
                    .map(|s| {
                        s.frames
                            .iter()
                            .enumerate()
                            .map(|(i, f)| {
                                let file = Path::new(&f.path).file_name().map(|n| n.to_string_lossy().into_owned());
                                json!({
                                    "id": i + 1, "name": f.name, "line": f.line, "column": f.col.max(1),
                                    "source": {"name": file, "path": f.path},
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let total = frames.len();
                out.respond(&req, json!({"stackFrames": frames, "totalFrames": total}));
            }
            "scopes" => {
                let snap = snapshot();
                let id = a["frameId"].as_u64().unwrap_or(1) as usize;
                let scopes = match snap.as_ref().and_then(|s| s.frames.get(id.saturating_sub(1)).map(|f| (f.locals, s.globals))) {
                    Some((locals, globals)) => json!([
                        {"name": "Locals", "variablesReference": locals, "expensive": false},
                        {"name": "Globals", "variablesReference": globals, "expensive": false},
                    ]),
                    None => json!([]),
                };
                out.respond(&req, json!({"scopes": scopes}));
            }
            "variables" => {
                let snap = snapshot();
                let id = a["variablesReference"].as_i64().unwrap_or(0);
                let vars: Vec<Json> = snap
                    .as_ref()
                    .and_then(|s| s.vars.get(&id))
                    .map(|vs| {
                        vs.iter()
                            .map(|v| json!({"name": v.name, "value": v.value, "type": v.kind, "variablesReference": v.reference}))
                            .collect()
                    })
                    .unwrap_or_default();
                out.respond(&req, json!({"variables": vars}));
            }
            "evaluate" => {
                // a variable's value, by name (for hovers and the watch panel)
                let snap = snapshot();
                let name = a["expression"].as_str().unwrap_or("").trim().to_string();
                let frame = a["frameId"].as_u64().unwrap_or(1) as usize;
                let found = snap.as_ref().and_then(|s| {
                    let locals = s.frames.get(frame.saturating_sub(1)).and_then(|f| s.vars.get(&f.locals));
                    let globals = s.vars.get(&s.globals);
                    locals.into_iter().chain(globals).flatten().find(|v| v.name == name).cloned()
                });
                match found {
                    Some(v) => out.respond(&req, json!({"result": v.value, "type": v.kind, "variablesReference": v.reference})),
                    None => out.fail(&req, &format!("\"{}\" is not a variable here", name)),
                }
            }
            "continue" => {
                let _ = tx.send(Cmd::Resume(Mode::Run));
                out.respond(&req, json!({"allThreadsContinued": true}));
            }
            "next" => {
                let _ = tx.send(Cmd::Resume(Mode::StepOver));
                out.respond(&req, json!({}));
            }
            "stepIn" => {
                let _ = tx.send(Cmd::Resume(Mode::StepIn));
                out.respond(&req, json!({}));
            }
            "stepOut" => {
                let _ = tx.send(Cmd::Resume(Mode::StepOut));
                out.respond(&req, json!({}));
            }
            "pause" => {
                shared.pause.store(true, Ordering::SeqCst);
                out.respond(&req, json!({}));
            }
            "terminate" => {
                shared.terminate.store(true, Ordering::SeqCst);
                let _ = tx.send(Cmd::Terminate);
                out.respond(&req, json!({}));
            }
            "disconnect" => {
                shared.terminate.store(true, Ordering::SeqCst);
                let _ = tx.send(Cmd::Terminate);
                out.respond(&req, json!({}));
                break;
            }
            _ => out.fail(&req, &format!("\"{}\" is not supported by the Globw debugger", command)),
        }
    }
    std::process::ExitCode::SUCCESS
}
