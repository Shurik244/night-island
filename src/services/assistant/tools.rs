use std::path::PathBuf;

use super::perms::Cap;
use super::{shell, web};

pub const MAX_FILE_CHARS: usize = 20_000;
const MAX_LISTED: usize = 120;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tool {
    Cmd(String),
    PowerShell(String),
    Wsl(String),
    Fetch(String),
    Render(String),
    Search(String),
    ReadFile(String),
    WriteFile { path: String, text: String, append: bool },
    Spotify { query: String, kind: String, artist: String },
    Yandex(crate::services::yandex::VoiceAction),
    SaveScript { name: String, description: String, body: String },
    RunScript { name: String, args: String },
}

impl Tool {
    pub fn cap(&self) -> Cap {
        match self {
            Tool::Cmd(_) | Tool::PowerShell(_) | Tool::Wsl(_) | Tool::SaveScript { .. } | Tool::RunScript { .. } => {
                Cap::Shell
            }
            Tool::Fetch(_) | Tool::Render(_) => Cap::Browser,
            Tool::Search(_) => Cap::WebSearch,
            Tool::ReadFile(_) => Cap::FilesRead,
            Tool::WriteFile { .. } => Cap::FilesWrite,
            Tool::Spotify { .. } | Tool::Yandex(_) => Cap::Media,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Tool::Cmd(_) => "shell",
            Tool::PowerShell(_) => "powershell",
            Tool::Wsl(_) => "wsl",
            Tool::Fetch(_) => "fetch",
            Tool::Render(_) => "fetch",
            Tool::Search(_) => "search",
            Tool::ReadFile(_) => "read_file",
            Tool::WriteFile { .. } => "write_file",
            Tool::Spotify { .. } => "spotify",
            Tool::Yandex(_) => "yandex",
            Tool::SaveScript { .. } => "save_script",
            Tool::RunScript { .. } => "run_script",
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Tool::Cmd(t)
            | Tool::PowerShell(t)
            | Tool::Wsl(t)
            | Tool::Fetch(t)
            | Tool::Render(t)
            | Tool::Search(t)
            | Tool::ReadFile(t) => t,
            Tool::WriteFile { path, .. } => path,
            Tool::Spotify { query, .. } => query,
            Tool::Yandex(action) => match action {
                crate::services::yandex::VoiceAction::Station { query, .. } => query,
                crate::services::yandex::VoiceAction::Track { title, .. } => title,
                crate::services::yandex::VoiceAction::Rate(kind) => kind,
                _ => "My Wave",
            },
            Tool::SaveScript { name, .. } | Tool::RunScript { name, .. } => name,
        }
    }

    pub fn signature(&self) -> String {
        match self {
            Tool::Yandex(action) => format!("{}:{action:?}", self.name()),
            _ => format!("{}:{}", self.name(), self.detail()),
        }
    }

    pub fn script(&self) -> bool {
        matches!(
            self,
            Tool::Cmd(_) | Tool::PowerShell(_) | Tool::Wsl(_) | Tool::SaveScript { .. } | Tool::RunScript { .. }
        )
    }

    pub fn looks_only(&self) -> bool {
        matches!(self, Tool::Cmd(c) if shell::classify(c) == shell::Class::Look)
    }

    pub fn external(&self) -> bool {
        match self {
            Tool::Fetch(_) | Tool::Render(_) | Tool::Search(_) | Tool::ReadFile(_) => true,
            Tool::Cmd(t) | Tool::PowerShell(t) | Tool::Wsl(t) => t.to_ascii_lowercase().contains("http"),
            Tool::WriteFile { .. }
            | Tool::Spotify { .. }
            | Tool::Yandex(_)
            | Tool::SaveScript { .. }
            | Tool::RunScript { .. } => false,
        }
    }
}

pub const MAX_SCRIPTS: usize = 40;
pub const MAX_SCRIPT_CHARS: usize = 4000;
const MAX_ARGS_CHARS: usize = 400;

pub fn scripts_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("NightIsland").join("assistant").join("scripts"))
}

pub fn valid_script_name(name: &str) -> bool {
    (1..=40).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

fn script_path(dir: &std::path::Path, name: &str) -> Result<PathBuf, String> {
    if valid_script_name(name) {
        Ok(dir.join(format!("{name}.ps1")))
    } else {
        Err("A script name is letters, digits, - and _ only".into())
    }
}

fn describe(text: &str) -> String {
    let first = text.trim_start_matches('\u{feff}').lines().next().unwrap_or("");
    let line = first.trim().trim_start_matches('#').trim();
    line.chars().filter(|c| !c.is_control()).take(90).collect()
}

pub fn save_script_in(dir: &std::path::Path, name: &str, description: &str, body: &str) -> Result<String, String> {
    let path = script_path(dir, name)?;
    let body = body.trim();
    if body.is_empty() || body.chars().count() > MAX_SCRIPT_CHARS {
        return Err("The script is empty or too long".into());
    }
    std::fs::create_dir_all(dir).map_err(|_| "The scripts folder could not be created".to_string())?;
    let known = std::fs::read_dir(dir).map(|d| d.flatten().count()).unwrap_or(0);
    if known >= MAX_SCRIPTS && !path.exists() {
        return Err("There are too many saved scripts; remove one first".into());
    }
    let note: String = description.lines().next().unwrap_or("").chars().filter(|c| !c.is_control()).take(90).collect();
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(format!("# {}\n{body}\n", note.trim()).as_bytes());
    std::fs::write(&path, bytes).map_err(|_| "The script could not be saved".to_string())?;
    Ok(format!("Saved the script {name}; run it with run_script"))
}

pub fn list_scripts_in(dir: &std::path::Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<(String, String)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_stem()?.to_str()?.to_string();
            if path.extension()?.to_str()? != "ps1" || !valid_script_name(&name) {
                return None;
            }
            let mut head = Vec::new();
            std::io::Read::read_to_end(&mut std::io::Read::take(std::fs::File::open(&path).ok()?, 400), &mut head)
                .ok()?;
            Some((name, describe(&String::from_utf8_lossy(&head))))
        })
        .collect();
    out.sort();
    out.truncate(MAX_SCRIPTS);
    out
}

pub fn scripts_prompt(budget: usize) -> String {
    let Some(dir) = scripts_dir() else { return String::new() };
    let mut out = String::new();
    for (name, about) in list_scripts_in(&dir) {
        let item = if about.is_empty() { name } else { format!("{name} ({about})") };
        if out.chars().count() + item.chars().count() + 2 > budget {
            break;
        }
        if !out.is_empty() {
            out.push_str("; ");
        }
        out.push_str(&item);
    }
    out
}

pub fn run_script_in(dir: &std::path::Path, name: &str, args: &str) -> Result<String, String> {
    let path = script_path(dir, name)?;
    if !path.is_file() {
        return Err(format!("There is no saved script called {name}"));
    }
    let args: String = args.chars().filter(|c| !c.is_control()).take(MAX_ARGS_CHARS).collect();
    let call = format!("& '{}' {args}", path.display().to_string().replace('\'', "''"));
    shell::run_powershell(&call)
}

fn resolve(path: &str) -> Result<PathBuf, String> {
    let path = path.trim().trim_matches('"');
    if path.is_empty() || path.contains('\0') {
        return Err("The path is empty".into());
    }
    let home = PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_else(|| ".".into()));
    let expanded = match path.strip_prefix('~') {
        Some(rest) => home.join(rest.trim_start_matches(['\\', '/'])),
        None => PathBuf::from(path),
    };
    Ok(if expanded.is_absolute() { expanded } else { home.join(expanded) })
}

fn read_file(path: &str) -> Result<String, String> {
    let path = resolve(path)?;
    let meta = std::fs::metadata(&path).map_err(|_| "That path does not exist or cannot be read".to_string())?;
    if meta.is_dir() {
        let mut names: Vec<String> = std::fs::read_dir(&path)
            .map_err(|_| "That folder cannot be read".to_string())?
            .flatten()
            .map(|e| {
                let dir = e.file_type().is_ok_and(|t| t.is_dir());
                format!("{}{}", e.file_name().to_string_lossy(), if dir { "\\" } else { "" })
            })
            .collect();
        names.sort_unstable_by_key(|n| n.to_lowercase());
        let total = names.len();
        names.truncate(MAX_LISTED);
        let mut out = names.join("\n");
        if total > MAX_LISTED {
            out.push_str(&format!("\n[{} more]", total - MAX_LISTED));
        }
        return Ok(if out.is_empty() { "The folder is empty".into() } else { out });
    }
    let mut buf = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(std::fs::File::open(&path).map_err(|_| "That file cannot be opened")?, 200_000),
        &mut buf,
    )
    .map_err(|_| "That file cannot be read")?;
    if buf.contains(&0) {
        return Err("That file is not text".into());
    }
    Ok(shell::clip_output(&String::from_utf8_lossy(&buf)))
}

fn write_file(path: &str, text: &str, append: bool) -> Result<String, String> {
    let path = resolve(path)?;
    if text.chars().count() > MAX_FILE_CHARS {
        return Err("The text is too long for one file write".into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "The folder could not be created".to_string())?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&path)
        .map_err(|_| "The file could not be written".to_string())?;
    std::io::Write::write_all(&mut file, text.as_bytes()).map_err(|_| "The file could not be written".to_string())?;
    Ok(format!("Saved {}", path.display()))
}

pub fn run(tool: &Tool) -> Result<String, String> {
    match tool {
        Tool::Cmd(command) => shell::run(command),
        Tool::PowerShell(script) => shell::run_powershell(script),
        Tool::Wsl(script) => shell::run_wsl(script),
        Tool::Fetch(url) => web::fetch(url),
        Tool::Render(url) => web::render(url),
        Tool::Spotify { query, kind, artist } => crate::services::spotify::play_search(query, kind, artist),
        Tool::Yandex(action) => crate::services::yandex::voice(action.clone()).map_err(str::to_string),
        Tool::SaveScript { name, description, body } => {
            let dir = scripts_dir().ok_or("There is no profile folder")?;
            save_script_in(&dir, name, description, body)
        }
        Tool::RunScript { name, args } => {
            let dir = scripts_dir().ok_or("There is no profile folder")?;
            run_script_in(&dir, name, args)
        }
        Tool::Search(query) => web::search(query),
        Tool::ReadFile(path) => read_file(path),
        Tool::WriteFile { path, text, append } => write_file(path, text, *append),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ni-tools-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn every_tool_maps_to_a_capability_and_a_name() {
        let all = [
            (Tool::Cmd("dir".into()), Cap::Shell, "shell"),
            (Tool::PowerShell("ls".into()), Cap::Shell, "powershell"),
            (Tool::Wsl("ls".into()), Cap::Shell, "wsl"),
            (Tool::Fetch("https://a.b".into()), Cap::Browser, "fetch"),
            (Tool::Search("x".into()), Cap::WebSearch, "search"),
            (Tool::ReadFile("a".into()), Cap::FilesRead, "read_file"),
            (Tool::WriteFile { path: "a".into(), text: String::new(), append: false }, Cap::FilesWrite, "write_file"),
        ];
        for (tool, cap, name) in all {
            assert_eq!((tool.cap(), tool.name()), (cap, name));
        }
        assert!(Tool::Cmd("dir".into()).looks_only() && !Tool::Cmd("mkdir x".into()).looks_only());
        assert!(!Tool::PowerShell("ls".into()).looks_only());
    }

    #[test]
    fn what_may_carry_outside_text_is_marked() {
        assert!(Tool::Fetch("https://a.b".into()).external() && Tool::Search("x".into()).external());
        assert!(Tool::PowerShell("iwr https://x.y | select Content".into()).external());
        assert!(!Tool::PowerShell("Get-ChildItem".into()).external());
        assert!(!Tool::WriteFile { path: "a".into(), text: "http".into(), append: false }.external());
    }

    #[test]
    fn files_are_written_appended_and_read_back() {
        let dir = scratch("rw");
        let file = dir.join("deep").join("notes.txt");
        let path = file.to_string_lossy().into_owned();
        assert!(write_file(&path, "one\n", false).unwrap().starts_with("Saved"));
        write_file(&path, "two\n", true).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "one\ntwo\n");
        write_file(&path, "fresh", false).unwrap();
        assert_eq!(read_file(&path).unwrap(), "fresh");
        let listing = read_file(&dir.to_string_lossy()).unwrap();
        assert_eq!(listing, "deep\\");
        assert!(write_file(&path, &"x".repeat(MAX_FILE_CHARS + 1), false).is_err());
        assert!(read_file(&dir.join("missing").to_string_lossy()).is_err());
        assert!(resolve("   ").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_script_is_saved_listed_with_its_description_and_run_only_by_a_safe_name() {
        let dir = scratch("scripts");
        assert!(
            save_script_in(&dir, "hello", "Says hello\nsecond line", "Write-Output 'hi'").unwrap().contains("hello")
        );
        save_script_in(&dir, "zeta", "", "Get-Date").unwrap();
        let list = list_scripts_in(&dir);
        assert_eq!(list, [("hello".to_string(), "Says hello".to_string()), ("zeta".to_string(), String::new())]);
        let text = std::fs::read_to_string(dir.join("hello.ps1")).unwrap();
        assert!(text.trim_start_matches('\u{feff}').starts_with("# Says hello\nWrite-Output 'hi'"));
        assert!(save_script_in(&dir, "../x", "", "ls").is_err());
        assert!(save_script_in(&dir, "a b", "", "ls").is_err());
        assert!(save_script_in(&dir, "empty", "", "  ").is_err());
        assert!(save_script_in(&dir, "long", "", &"x".repeat(MAX_SCRIPT_CHARS + 1)).is_err());
        assert!(run_script_in(&dir, "nothing", "").is_err());
        assert!(run_script_in(&dir, "..\\hello", "").is_err());
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        assert_eq!(list_scripts_in(&dir).len(), 2, "only .ps1 files count");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_script_limit_stops_new_names_but_not_an_overwrite() {
        let dir = scratch("limit");
        for i in 0..MAX_SCRIPTS {
            save_script_in(&dir, &format!("s{i}"), "", "ls").unwrap();
        }
        assert!(save_script_in(&dir, "extra", "", "ls").is_err());
        assert!(save_script_in(&dir, "s0", "new", "Get-Date").is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore = "starts powershell.exe"]
    fn a_saved_script_runs_with_arguments() {
        let dir = scratch("run");
        save_script_in(&dir, "greet", "Greets", "param($Who)\nWrite-Output \"hi $Who\"").unwrap();
        let out = run_script_in(&dir, "greet", "-Who 'Мир'").unwrap();
        assert!(out.contains("hi Мир"), "{out}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binary_files_are_refused() {
        let dir = scratch("bin");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.bin");
        std::fs::write(&file, [1u8, 0, 2]).unwrap();
        assert!(read_file(&file.to_string_lossy()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relative_paths_and_tilde_start_in_the_user_folder() {
        let home = PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_else(|| ".".into()));
        assert_eq!(resolve("~/Docs/a.txt").unwrap(), home.join("Docs/a.txt"));
        assert_eq!(resolve("Docs\\a.txt").unwrap(), home.join("Docs\\a.txt"));
    }
}
