use crate::data::keys::{ALT, CTRL, Combo, SHIFT, WIN};

pub const MAX_STEPS: usize = 5000;
pub const MAX_DEPTH: usize = 8;
pub const MAX_LOOP: u32 = 1_000_000;
pub const MAX_DELAY_MS: u32 = 3_600_000;
pub const MAX_LINES: usize = 20_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Method {
    #[default]
    Scan,
    Vk,
    Legacy,
    Post,
}

impl Method {
    pub const ALL: [Method; 4] = [Method::Scan, Method::Vk, Method::Legacy, Method::Post];

    pub fn id(self) -> &'static str {
        match self {
            Method::Scan => "scan",
            Method::Vk => "vk",
            Method::Legacy => "legacy",
            Method::Post => "post",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Method::Scan => "Scan codes",
            Method::Vk => "Virtual keys",
            Method::Legacy => "Legacy events",
            Method::Post => "Window messages",
        }
    }

    pub fn parse(text: &str) -> Option<Method> {
        Self::ALL.into_iter().find(|m| m.id().eq_ignore_ascii_case(text.trim()))
    }

    pub fn step(self) -> Method {
        let at = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

impl Button {
    pub fn id(self) -> &'static str {
        match self {
            Button::Left => "left",
            Button::Right => "right",
            Button::Middle => "middle",
            Button::X1 => "x1",
            Button::X2 => "x2",
        }
    }

    pub fn parse(text: &str) -> Option<Button> {
        match text.to_ascii_lowercase().as_str() {
            "left" | "l" | "lmb" => Some(Button::Left),
            "right" | "r" | "rmb" => Some(Button::Right),
            "middle" | "m" | "mmb" => Some(Button::Middle),
            "x1" | "back" => Some(Button::X1),
            "x2" | "forward" => Some(Button::X2),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Step {
    Tap(Vec<u16>),
    Down(u16),
    Up(u16),
    Type(String),
    Delay(u32, u32),
    Move { x: i32, y: i32, relative: bool },
    Click { button: Button, at: Option<(i32, i32)> },
    MouseDown(Button),
    MouseUp(Button),
    Wheel(i32),
    Run { target: String, args: String },
    WaitWindow { title: String, ms: u32 },
    PadTap(u16),
    PadDown(u16),
    PadUp(u16),
    PadStick { right: bool, x: i8, y: i8 },
    PadTrigger { right: bool, value: u8 },
    Loop { count: u32, body: Vec<Step> },
    IfWindow { title: String, body: Vec<Step> },
}

#[derive(Clone, PartialEq, Debug)]
pub struct Macro {
    pub name: String,
    pub hotkey: Option<Combo>,
    pub method: Method,
    pub repeat: u32,
    pub target: String,
    pub steps: Vec<Step>,
}

impl Default for Macro {
    fn default() -> Self {
        Self {
            name: String::new(),
            hotkey: None,
            method: Method::Scan,
            repeat: 1,
            target: String::new(),
            steps: Vec::new(),
        }
    }
}

const KEYS: [(&str, u16); 48] = [
    ("Enter", 0x0D),
    ("Esc", 0x1B),
    ("Tab", 0x09),
    ("Space", 0x20),
    ("Backspace", 0x08),
    ("Delete", 0x2E),
    ("Insert", 0x2D),
    ("Home", 0x24),
    ("End", 0x23),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("Up", 0x26),
    ("Down", 0x28),
    ("Left", 0x25),
    ("Right", 0x27),
    ("CapsLock", 0x14),
    ("NumLock", 0x90),
    ("ScrollLock", 0x91),
    ("PrintScreen", 0x2C),
    ("Pause", 0x13),
    ("Menu", 0x5D),
    ("Ctrl", 0x11),
    ("LCtrl", 0xA2),
    ("RCtrl", 0xA3),
    ("Alt", 0x12),
    ("LAlt", 0xA4),
    ("RAlt", 0xA5),
    ("Shift", 0x10),
    ("LShift", 0xA0),
    ("RShift", 0xA1),
    ("Win", 0x5B),
    ("RWin", 0x5C),
    ("Plus", 0xBB),
    ("Minus", 0xBD),
    ("Comma", 0xBC),
    ("Period", 0xBE),
    ("Slash", 0xBF),
    ("Semicolon", 0xBA),
    ("Quote", 0xDE),
    ("Backslash", 0xDC),
    ("LBracket", 0xDB),
    ("RBracket", 0xDD),
    ("Backtick", 0xC0),
    ("Num*", 0x6A),
    ("Num+", 0x6B),
    ("Num-", 0x6D),
    ("Num.", 0x6E),
    ("Num/", 0x6F),
];

const ALIASES: [(&str, u16); 17] = [
    ("return", 0x0D),
    ("escape", 0x1B),
    ("bksp", 0x08),
    ("del", 0x2E),
    ("ins", 0x2D),
    ("pgup", 0x21),
    ("pgdn", 0x22),
    ("prtsc", 0x2C),
    ("apps", 0x5D),
    ("control", 0x11),
    ("lwin", 0x5B),
    ("windows", 0x5B),
    ("equals", 0xBB),
    ("tilde", 0xC0),
    ("num,", 0x6C),
    ("break", 0x13),
    ("capital", 0x14),
];

pub fn parse_key(name: &str) -> Option<u16> {
    let name = name.trim();
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return match c {
            'a'..='z' | 'A'..='Z' => Some(c.to_ascii_uppercase() as u16),
            '0'..='9' => Some(c as u16),
            ';' => Some(0xBA),
            '=' => Some(0xBB),
            ',' => Some(0xBC),
            '-' => Some(0xBD),
            '.' => Some(0xBE),
            '/' => Some(0xBF),
            '`' => Some(0xC0),
            '[' => Some(0xDB),
            '\\' => Some(0xDC),
            ']' => Some(0xDD),
            '\'' => Some(0xDE),
            _ => None,
        };
    }
    let lower = name.to_ascii_lowercase();
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u16>().ok())
        && (1..=24).contains(&n)
    {
        return Some(0x6F + n);
    }
    let pad = lower.strip_prefix("numpad").or_else(|| lower.strip_prefix("num"));
    if let Some(n) = pad.and_then(|n| n.parse::<u16>().ok())
        && n <= 9
    {
        return Some(0x60 + n);
    }
    KEYS.iter().chain(ALIASES.iter()).find(|(n, _)| n.eq_ignore_ascii_case(&lower)).map(|(_, vk)| *vk)
}

pub fn key_name(vk: u16) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk as u32).map(String::from).unwrap_or_default(),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        _ => KEYS.iter().find(|(_, v)| *v == vk).map_or_else(|| format!("0x{vk:02X}"), |(n, _)| (*n).to_string()),
    }
}

fn parse_key_or_code(name: &str) -> Option<u16> {
    parse_key(name).or_else(|| {
        let hex = name.strip_prefix("0x")?;
        u16::from_str_radix(hex, 16).ok().filter(|v| (1..=0xFE).contains(v))
    })
}

pub fn parse_combo(text: &str) -> Result<Vec<u16>, String> {
    let mut keys = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let (token, tail) = if let Some(after) = rest.strip_prefix('+') {
            ("+", after)
        } else {
            match rest.find('+') {
                Some(i) => (&rest[..i], &rest[i..]),
                None => (rest, ""),
            }
        };
        let tail = tail.strip_prefix('+').unwrap_or(tail);
        let vk = if token == "+" { Some(0xBB) } else { parse_key_or_code(token) };
        keys.push(vk.ok_or_else(|| format!("unknown key \"{}\"", token.trim()))?);
        rest = tail.trim_start();
    }
    if keys.is_empty() {
        return Err("a key is missing".into());
    }
    if keys.len() > 8 {
        return Err("too many keys in one combination".into());
    }
    Ok(keys)
}

pub fn parse_hotkey(text: &str) -> Result<Option<Combo>, String> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("none") || text.eq_ignore_ascii_case("not set") {
        return Ok(None);
    }
    let mut mods = 0;
    let mut vk = None;
    for token in text.split('+') {
        match token.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= CTRL,
            "alt" => mods |= ALT,
            "shift" => mods |= SHIFT,
            "win" | "windows" => mods |= WIN,
            other => {
                if vk.is_some() {
                    return Err("a hotkey has one main key".into());
                }
                vk = Some(parse_key(other).ok_or_else(|| format!("unknown key \"{other}\""))? as u32);
            }
        }
    }
    let combo = Combo::new(mods, vk.ok_or("a hotkey needs a main key")?);
    if combo.valid() { Ok(Some(combo)) } else { Err("a hotkey needs a modifier and a regular key".into()) }
}

fn strip_quotes(text: &str) -> &str {
    let text = text.trim();
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') { &text[1..text.len() - 1] } else { text }
}

fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('\n', "\\n").replace('\r', "").replace('\t', "\\t")
}

fn parse_duration(text: &str) -> Result<u32, String> {
    let text = text.trim().to_ascii_lowercase();
    let (number, factor) = match text.strip_suffix("ms") {
        Some(n) => (n, 1.0),
        None => match text.strip_suffix('s') {
            Some(n) => (n, 1000.0),
            None => (text.as_str(), 1.0),
        },
    };
    let value: f64 = number.trim().parse().map_err(|_| format!("\"{text}\" is not a time"))?;
    if !(0.0..=MAX_DELAY_MS as f64).contains(&(value * factor)) {
        return Err("the delay is out of range".into());
    }
    Ok((value * factor).round() as u32)
}

fn parse_delay(text: &str) -> Result<Step, String> {
    match text.split_once("..") {
        Some((a, b)) => {
            let (a, b) = (parse_duration(a)?, parse_duration(b)?);
            Ok(Step::Delay(a.min(b), a.max(b)))
        }
        None => {
            let d = parse_duration(text)?;
            Ok(Step::Delay(d, d))
        }
    }
}

fn parse_int(text: &str) -> Result<i32, String> {
    text.trim().parse().map_err(|_| format!("\"{}\" is not a number", text.trim()))
}

fn split_first(rest: &str) -> (&str, &str) {
    let rest = rest.trim();
    if let Some(inner) = rest.strip_prefix('"')
        && let Some(end) = inner.find('"')
    {
        return (&inner[..end], inner[end + 1..].trim());
    }
    match rest.split_once(char::is_whitespace) {
        Some((a, b)) => (a, b.trim()),
        None => (rest, ""),
    }
}

fn title_and_time(rest: &str) -> (String, Option<u32>) {
    let rest = rest.trim();
    if let Some((head, tail)) = rest.rsplit_once(char::is_whitespace)
        && let Ok(ms) = parse_duration(tail)
        && !head.trim().is_empty()
    {
        return (strip_quotes(head).to_string(), Some(ms));
    }
    (strip_quotes(rest).to_string(), None)
}

pub const PAD_BUTTONS: [(&str, u16); 15] = [
    ("A", 0x1000),
    ("B", 0x2000),
    ("X", 0x4000),
    ("Y", 0x8000),
    ("LB", 0x0100),
    ("RB", 0x0200),
    ("Back", 0x0020),
    ("Start", 0x0010),
    ("LS", 0x0040),
    ("RS", 0x0080),
    ("Up", 0x0001),
    ("Down", 0x0002),
    ("Left", 0x0004),
    ("Right", 0x0008),
    ("Guide", 0x0400),
];

pub fn pad_button(name: &str) -> Option<u16> {
    PAD_BUTTONS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name.trim())).map(|(_, mask)| *mask)
}

pub fn pad_button_name(mask: u16) -> &'static str {
    PAD_BUTTONS.iter().find(|(_, m)| *m == mask).map_or("A", |(n, _)| *n)
}

fn parse_side(text: &str) -> Result<bool, String> {
    match text.to_ascii_lowercase().as_str() {
        "left" | "l" => Ok(false),
        "right" | "r" => Ok(true),
        _ => Err("use left or right".into()),
    }
}

fn parse_percent(text: &str, low: i32) -> Result<i32, String> {
    let value: i32 =
        text.trim().trim_end_matches('%').parse().map_err(|_| format!("\"{}\" is not a number", text.trim()))?;
    if !(low..=100).contains(&value) {
        return Err(format!("the value must be from {low} to 100"));
    }
    Ok(value)
}

fn parse_pad(rest: &str) -> Result<Step, String> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    let button = |name: &str| pad_button(name).ok_or_else(|| format!("unknown gamepad button \"{name}\""));
    match parts.as_slice() {
        [name] | ["press", name] | ["tap", name] => Ok(Step::PadTap(button(name)?)),
        ["down", name] => Ok(Step::PadDown(button(name)?)),
        ["up", name] => Ok(Step::PadUp(button(name)?)),
        ["stick", side, x, y] => Ok(Step::PadStick {
            right: parse_side(side)?,
            x: parse_percent(x, -100)? as i8,
            y: parse_percent(y, -100)? as i8,
        }),
        ["trigger", side, value] => {
            Ok(Step::PadTrigger { right: parse_side(side)?, value: parse_percent(value, 0)? as u8 })
        }
        _ => Err("use: pad A, pad down A, pad up A, pad stick left X Y, pad trigger right N".into()),
    }
}

fn parse_step(command: &str, rest: &str) -> Result<Step, String> {
    match command {
        "pad" | "gamepad" => parse_pad(rest),
        "key" | "tap" | "press" => Ok(Step::Tap(parse_combo(rest)?)),
        "down" | "keydown" => Ok(Step::Down(parse_key_or_code(rest).ok_or_else(|| format!("unknown key \"{rest}\""))?)),
        "up" | "keyup" => Ok(Step::Up(parse_key_or_code(rest).ok_or_else(|| format!("unknown key \"{rest}\""))?)),
        "type" | "text" => Ok(Step::Type(unescape(rest.strip_prefix(' ').unwrap_or(rest)))),
        "delay" | "sleep" | "wait" => parse_delay(rest),
        "move" => {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            match parts.as_slice() {
                ["rel", x, y] | ["relative", x, y] => {
                    Ok(Step::Move { x: parse_int(x)?, y: parse_int(y)?, relative: true })
                }
                [x, y] => Ok(Step::Move { x: parse_int(x)?, y: parse_int(y)?, relative: false }),
                _ => Err("use: move X Y or move rel DX DY".into()),
            }
        }
        "click" | "mouseclick" => {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            let (button, pos) = match parts.first().and_then(|p| Button::parse(p)) {
                Some(b) => (b, &parts[1..]),
                None => (Button::Left, parts.as_slice()),
            };
            match pos {
                [] => Ok(Step::Click { button, at: None }),
                [x, y] => Ok(Step::Click { button, at: Some((parse_int(x)?, parse_int(y)?)) }),
                _ => Err("use: click [button] [X Y]".into()),
            }
        }
        "mousedown" => Button::parse(rest.trim()).map(Step::MouseDown).ok_or_else(|| "unknown mouse button".into()),
        "mouseup" => Button::parse(rest.trim()).map(Step::MouseUp).ok_or_else(|| "unknown mouse button".into()),
        "wheel" | "scroll" => Ok(Step::Wheel(parse_int(rest)?.clamp(-100_000, 100_000))),
        "run" | "open" => {
            let (target, args) = split_first(rest);
            if target.is_empty() {
                return Err("a program or address is missing".into());
            }
            Ok(Step::Run { target: target.to_string(), args: args.to_string() })
        }
        "wait_window" | "waitwindow" => {
            let (title, ms) = title_and_time(rest);
            if title.is_empty() {
                return Err("a window title is missing".into());
            }
            Ok(Step::WaitWindow { title, ms: ms.unwrap_or(10_000) })
        }
        other => Err(format!("unknown command \"{other}\"")),
    }
}

enum Block {
    Loop(u32),
    IfWindow(String),
}

pub fn parse(text: &str) -> Result<Macro, String> {
    let mut out = Macro::default();
    let mut stack: Vec<(Block, Vec<Step>)> = Vec::new();
    let mut top: Vec<Step> = Vec::new();
    let mut count = 0;
    for (index, raw) in text.lines().enumerate() {
        if index >= MAX_LINES {
            return Err("the macro is too long".into());
        }
        let line = raw.trim();
        let at = |why: String| format!("line {}: {why}", index + 1);
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if stack.is_empty()
            && top.is_empty()
            && let Some((key, value)) = line.split_once(':')
            && !key.contains(char::is_whitespace)
        {
            match key.to_ascii_lowercase().as_str() {
                "name" => out.name = value.trim().chars().take(80).collect(),
                "hotkey" => out.hotkey = parse_hotkey(value).map_err(at)?,
                "method" => {
                    out.method = Method::parse(value).ok_or_else(|| at("method is scan, vk, legacy or post".into()))?
                }
                "repeat" => {
                    let value = value.trim();
                    out.repeat = if value.eq_ignore_ascii_case("forever") {
                        0
                    } else {
                        value.parse::<u32>().map_err(|_| at("repeat is a number or forever".into()))?.min(MAX_LOOP)
                    }
                }
                "target" => out.target = value.trim().chars().take(120).collect(),
                other => return Err(at(format!("unknown setting \"{other}\""))),
            }
            continue;
        }
        let (command, rest) = match line.split_once(char::is_whitespace) {
            Some((c, r)) => (c.to_ascii_lowercase(), r),
            None => (line.to_ascii_lowercase(), ""),
        };
        count += 1;
        if count > MAX_STEPS {
            return Err("the macro has too many steps".into());
        }
        match command.as_str() {
            "loop" | "repeat" => {
                if stack.len() >= MAX_DEPTH {
                    return Err(at("blocks are nested too deeply".into()));
                }
                let n = if rest.trim().eq_ignore_ascii_case("forever") {
                    0
                } else {
                    rest.trim().parse::<u32>().map_err(|_| at("loop needs a count".into()))?.min(MAX_LOOP)
                };
                stack.push((Block::Loop(n), std::mem::take(&mut top)));
            }
            "if_window" | "ifwindow" => {
                if stack.len() >= MAX_DEPTH {
                    return Err(at("blocks are nested too deeply".into()));
                }
                let title = strip_quotes(rest).to_string();
                if title.is_empty() {
                    return Err(at("a window title is missing".into()));
                }
                stack.push((Block::IfWindow(title), std::mem::take(&mut top)));
            }
            "end" => {
                let (block, outer) = stack.pop().ok_or_else(|| at("end without a block".into()))?;
                let body = std::mem::replace(&mut top, outer);
                top.push(match block {
                    Block::Loop(count) => Step::Loop { count, body },
                    Block::IfWindow(title) => Step::IfWindow { title, body },
                });
            }
            _ => top.push(parse_step(&command, rest).map_err(at)?),
        }
    }
    if !stack.is_empty() {
        return Err("a block is not closed with end".into());
    }
    out.steps = top;
    Ok(out)
}

fn write_steps(out: &mut String, steps: &[Step], depth: usize) {
    let pad = "  ".repeat(depth);
    for step in steps {
        match step {
            Step::Tap(keys) => {
                let names: Vec<String> = keys.iter().map(|k| key_name(*k)).collect();
                out.push_str(&format!("{pad}key {}\n", names.join("+")));
            }
            Step::Down(k) => out.push_str(&format!("{pad}down {}\n", key_name(*k))),
            Step::Up(k) => out.push_str(&format!("{pad}up {}\n", key_name(*k))),
            Step::Type(text) => out.push_str(&format!("{pad}type {}\n", escape(text))),
            Step::Delay(a, b) if a == b => out.push_str(&format!("{pad}delay {a}\n")),
            Step::Delay(a, b) => out.push_str(&format!("{pad}delay {a}..{b}\n")),
            Step::Move { x, y, relative: true } => out.push_str(&format!("{pad}move rel {x} {y}\n")),
            Step::Move { x, y, relative: false } => out.push_str(&format!("{pad}move {x} {y}\n")),
            Step::Click { button, at: None } => out.push_str(&format!("{pad}click {}\n", button.id())),
            Step::Click { button, at: Some((x, y)) } => out.push_str(&format!("{pad}click {} {x} {y}\n", button.id())),
            Step::MouseDown(b) => out.push_str(&format!("{pad}mousedown {}\n", b.id())),
            Step::MouseUp(b) => out.push_str(&format!("{pad}mouseup {}\n", b.id())),
            Step::Wheel(n) => out.push_str(&format!("{pad}wheel {n}\n")),
            Step::Run { target, args } if args.is_empty() => out.push_str(&format!("{pad}run \"{target}\"\n")),
            Step::Run { target, args } => out.push_str(&format!("{pad}run \"{target}\" {args}\n")),
            Step::WaitWindow { title, ms } => out.push_str(&format!("{pad}wait_window \"{title}\" {ms}\n")),
            Step::PadTap(m) => out.push_str(&format!("{pad}pad {}\n", pad_button_name(*m))),
            Step::PadDown(m) => out.push_str(&format!("{pad}pad down {}\n", pad_button_name(*m))),
            Step::PadUp(m) => out.push_str(&format!("{pad}pad up {}\n", pad_button_name(*m))),
            Step::PadStick { right, x, y } => {
                out.push_str(&format!("{pad}pad stick {} {x} {y}\n", if *right { "right" } else { "left" }))
            }
            Step::PadTrigger { right, value } => {
                out.push_str(&format!("{pad}pad trigger {} {value}\n", if *right { "right" } else { "left" }))
            }
            Step::Loop { count, body } => {
                let n = if *count == 0 { "forever".to_string() } else { count.to_string() };
                out.push_str(&format!("{pad}loop {n}\n"));
                write_steps(out, body, depth + 1);
                out.push_str(&format!("{pad}end\n"));
            }
            Step::IfWindow { title, body } => {
                out.push_str(&format!("{pad}if_window \"{title}\"\n"));
                write_steps(out, body, depth + 1);
                out.push_str(&format!("{pad}end\n"));
            }
        }
    }
}

pub fn to_text(m: &Macro) -> String {
    let mut out = format!("name: {}\n", m.name);
    if let Some(combo) = m.hotkey {
        out.push_str(&format!("hotkey: {}\n", combo.text()));
    }
    out.push_str(&format!("method: {}\n", m.method.id()));
    out.push_str(&format!("repeat: {}\n", if m.repeat == 0 { "forever".to_string() } else { m.repeat.to_string() }));
    if !m.target.is_empty() {
        out.push_str(&format!("target: {}\n", m.target));
    }
    out.push('\n');
    write_steps(&mut out, &m.steps, 0);
    out
}

pub fn set_header(text: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let is_key = |line: &str| {
        line.split_once(':').is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key) && !k.trim().contains(' '))
    };
    let mut headers_end = 0;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.split_once(':').is_some_and(|(k, _)| !k.contains(char::is_whitespace)) {
            headers_end = i + 1;
        } else {
            break;
        }
    }
    let replacement = if value.is_empty() { None } else { Some(format!("{key}: {value}")) };
    match lines[..headers_end.min(lines.len())].iter().position(|l| is_key(l)) {
        Some(i) => match replacement {
            Some(line) => lines[i] = line,
            None => {
                lines.remove(i);
            }
        },
        None => {
            if let Some(line) = replacement {
                lines.insert(headers_end.min(lines.len()), line);
            }
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

pub const TEMPLATE: &str = "name: New macro\nmethod: scan\nrepeat: 1\n\n# Commands: key Ctrl+C, down/up KEY, type TEXT, delay 100 or 50..150,\n# move X Y, move rel DX DY, click [left|right|middle] [X Y], mousedown/mouseup BUTTON,\n# pad A, pad down/up LB, pad stick left X Y (-100..100), pad trigger right N (0..100),\n# wheel N, run \"program\" ARGS, wait_window \"Title\" MS, loop N ... end, if_window \"Title\" ... end\n\nkey Ctrl+C\ndelay 100\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headers_and_steps() {
        let m = parse("name: Test\nhotkey: Ctrl+Alt+1\nmethod: vk\nrepeat: 3\n\nkey Ctrl+C\ndelay 50..120\nclick right 10 20\ntype Hi\\nthere\n")
            .unwrap();
        assert_eq!(m.name, "Test");
        assert_eq!(m.hotkey, Some(Combo::new(CTRL | ALT, b'1' as u32)));
        assert_eq!(m.method, Method::Vk);
        assert_eq!(m.repeat, 3);
        assert_eq!(m.steps.len(), 4);
        assert_eq!(m.steps[0], Step::Tap(vec![0x11, 0x43]));
        assert_eq!(m.steps[1], Step::Delay(50, 120));
        assert_eq!(m.steps[2], Step::Click { button: Button::Right, at: Some((10, 20)) });
        assert_eq!(m.steps[3], Step::Type("Hi\nthere".into()));
    }

    #[test]
    fn blocks_nest_and_round_trip() {
        let text = "name: Loops\nmethod: scan\nrepeat: forever\n\nloop 2\n  key A\n  if_window \"Notepad\"\n    type x\n  end\nend\n";
        let m = parse(text).unwrap();
        assert_eq!(m.repeat, 0);
        assert!(matches!(&m.steps[0], Step::Loop { count: 2, body } if body.len() == 2));
        assert_eq!(parse(&to_text(&m)).unwrap(), m);
    }

    #[test]
    fn errors_name_the_line() {
        assert!(parse("key Ctrl+Nope").unwrap_err().starts_with("line 1"));
        assert!(parse("loop 2\nkey A").is_err());
        assert!(parse("end").is_err());
        assert!(parse("hotkey: A").is_err());
    }

    #[test]
    fn combos_accept_plus_and_codes() {
        assert_eq!(parse_combo("Ctrl+Shift+Esc").unwrap(), vec![0x11, 0x10, 0x1B]);
        assert_eq!(parse_combo("Ctrl++").unwrap(), vec![0x11, 0xBB]);
        assert_eq!(parse_combo("0x41").unwrap(), vec![0x41]);
        assert!(parse_combo("").is_err());
    }

    #[test]
    fn durations_have_units() {
        assert_eq!(parse_duration("2s").unwrap(), 2000);
        assert_eq!(parse_duration("150ms").unwrap(), 150);
        assert!(parse_duration("99999999").is_err());
    }

    #[test]
    fn gamepad_steps_round_trip() {
        let text = "pad A\npad down LB\npad up LB\npad stick left 50 -100\npad trigger right 100\n";
        let m = parse(text).unwrap();
        assert_eq!(m.steps[0], Step::PadTap(0x1000));
        assert_eq!(m.steps[3], Step::PadStick { right: false, x: 50, y: -100 });
        assert_eq!(m.steps[4], Step::PadTrigger { right: true, value: 100 });
        assert_eq!(parse(&to_text(&m)).unwrap(), m);
        assert!(parse("pad stick left 500 0").is_err());
        assert!(parse("pad Z").is_err());
    }

    #[test]
    fn nesting_is_limited() {
        let deep = "loop 1\n".repeat(MAX_DEPTH + 1);
        assert!(parse(&deep).is_err());
    }

    #[test]
    fn headers_are_replaced_inserted_and_removed() {
        let text = "name: A\nmethod: scan\n\nkey A\n";
        let changed = set_header(text, "method", "post");
        assert_eq!(parse(&changed).unwrap().method, Method::Post);
        let added = set_header(text, "hotkey", "Ctrl+Alt+2");
        assert_eq!(parse(&added).unwrap().hotkey, Some(Combo::new(CTRL | ALT, b'2' as u32)));
        let removed = set_header(&added, "hotkey", "");
        assert_eq!(parse(&removed).unwrap().hotkey, None);
        assert_eq!(parse(&removed).unwrap().steps.len(), 1);
    }

    #[test]
    fn the_template_parses() {
        assert!(parse(TEMPLATE).is_ok());
    }
}
