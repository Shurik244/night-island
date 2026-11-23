# Night Island macros: guide

A macro is a small text file. Night Island plays it back by pressing keys, moving and clicking the mouse, typing text, starting programs and pressing buttons on a virtual gamepad. Macros work in any program that accepts input from the system, including games that read scan codes.

Open the **Macros** tab, press **New**, edit the text and press **Run**. Everything below can be written by hand, recorded, or mixed.

## Where macros live

- Folder: `%APPDATA%\NightIsland\macros\`. Press **Folder** in the tab to open it.
- One macro is one file named `<name>.nimacro`. Plain UTF-8 text, at most 256 KB.
- **Open** opens the selected macro in your default text editor (Notepad when `.nimacro` has no program assigned). The tab picks up your changes when you come back to it.
- **Guide** opens this guide.
- Edit as you like and save; the tab re-reads the file when you open it. The editor in the tab saves itself when it loses focus, when you pick another macro or when you leave the tab.

## File layout

```
name: Open notes             <- header lines first
hotkey: Ctrl+Alt+1
method: scan
repeat: 1

# a comment
key Win+R                    <- then one step per line
delay 300
type notepad\n
```

Lines starting with `#` or `//` are comments, empty lines are ignored. Commands are not case-sensitive. Header lines must come before the first step. Indentation is free and ignored.

## Header settings

| Line | Meaning |
| --- | --- |
| `name: Text` | Name shown in the list (up to 80 characters). The file name stays as it is. |
| `hotkey: Ctrl+Alt+1` | What starts the macro: a combination, a single key (`F8`, `X`) or a mouse button (`Mouse4`). See "Hotkeys". Leave out or write `none` for no hotkey. |
| `swallow: yes` | Only for single keys and mouse buttons: the press is not passed on to the program under the cursor. Default `no`. |
| `process: game.exe` | Only start the macro when the program in front is this one. Several: `game.exe \| other.exe`. Optional. See "Limit a macro to a program or window". |
| `window: Level 1` | Only start the macro when the title of the front window contains this text (case-insensitive). Several: `A \| B`. Optional. |
| `method: scan` | How keys are sent: `scan`, `vk`, `legacy` or `post`. See "Input methods". Default `scan`. |
| `repeat: 3` | How many times the whole macro runs. `forever` (or `0`) repeats until you stop it. Default `1`. |
| `target: Notepad` | Only for `method: post`: part of the title of the window that receives the keys. |

Unknown header names are an error, so typos do not pass silently.

## Steps

### Keyboard

| Step | What it does |
| --- | --- |
| `key Ctrl+C` | Presses the keys of the combination in order and releases them in reverse. Aliases: `tap`, `press`. |
| `down Shift` | Presses a key and keeps it down. Alias: `keydown`. |
| `up Shift` | Releases a key. Alias: `keyup`. |
| `type Hello, world` | Types the rest of the line as text (any language, emoji too). Escapes: `\n` Enter, `\t` Tab, `\\` backslash. Alias: `text`. |

Keys held with `down` are released automatically when the macro ends or is stopped.

**Key names**

- Letters `A`..`Z`, digits `0`..`9`, `F1`..`F24`, numpad `Num0`..`Num9`, `Num*`, `Num+`, `Num-`, `Num.`, `Num/`.
- `Enter` (`Return`), `Esc` (`Escape`), `Tab`, `Space`, `Backspace` (`Bksp`), `Delete` (`Del`), `Insert` (`Ins`), `Home`, `End`, `PageUp` (`PgUp`), `PageDown` (`PgDn`), `Up`, `Down`, `Left`, `Right`.
- `CapsLock`, `NumLock`, `ScrollLock`, `PrintScreen` (`PrtSc`), `Pause`, `Menu` (`Apps`).
- Modifiers: `Ctrl` (`Control`), `Alt`, `Shift`, `Win` (`Windows`), and sides `LCtrl`, `RCtrl`, `LAlt`, `RAlt`, `LShift`, `RShift`, `RWin`.
- Punctuation: write the character (`;` `=` `,` `-` `.` `/` `` ` `` `[` `\` `]` `'`) or its name: `Semicolon`, `Equals`, `Comma`, `Minus`, `Period`, `Slash`, `Backtick`, `LBracket`, `Backslash`, `RBracket`, `Quote`, `Plus`.
- A raw virtual-key code in hex: `0xAF`.
- In a combination write the plus key as `Plus` or `Ctrl++`.

### Waiting

| Step | What it does |
| --- | --- |
| `delay 200` | Waits 200 ms. Units: `ms` (default) and `s`, for example `delay 1.5s`. Aliases: `sleep`, `wait`. |
| `delay 50..150` | Waits a random time in the range. Useful so repeated actions do not look mechanical. |
| `wait_window "Title" 5000` | Waits until a visible window whose title contains the text appears (case-insensitive). The time limit is optional (default 10 s). If the window does not appear, the macro stops with an error instead of typing into the wrong place. |

The longest single delay is one hour. A long wait can be stopped at any moment.

### Mouse

| Step | What it does |
| --- | --- |
| `move 800 400` | Moves the cursor to the screen position (pixels, over all monitors; the top-left of the primary monitor is `0 0`, monitors to the left or above have negative values). |
| `move rel 20 -10` | Moves the cursor relative to where it is (raw relative motion, what games read). |
| `click` | Clicks the left button where the cursor is. |
| `click right` | Buttons: `left`, `right`, `middle`, `x1` (`back`), `x2` (`forward`); also `l`, `r`, `m`. |
| `click left 800 400` | Moves there first, then clicks. |
| `mousedown left` / `mouseup left` | Holds and releases a button, for dragging. |
| `wheel 120` | Scrolls up by one notch (120 units); negative values scroll down. Alias: `scroll`. |

### Programs and files

| Step | What it does |
| --- | --- |
| `run "notepad.exe"` | Starts a program, opens a file, a folder or an address with the default program. Alias: `open`. |
| `run "C:\Tools\app.exe" --fast` | Everything after the quoted target is passed as arguments. |
| `run https://example.com` | A web address opens in the default browser. |

If Windows cannot start the target, the macro stops with the message "Could not start ...".

### Gamepad

```
pad A                       press and release A
pad down LB                 hold LB
pad up LB                   release LB
pad stick left 50 -100      left stick: X 50%, Y -100% (-100..100, Y up is positive)
pad stick right 0 0         right stick back to the centre
pad trigger right 100       right trigger fully pressed (0..100)
```

Buttons: `A B X Y LB RB Back Start LS RS Up Down Left Right Guide`. Alias: `gamepad` instead of `pad`.

The pad is a virtual Xbox 360 controller created through the **ViGEmBus** driver, which has to be installed on the PC. Without it a macro with `pad` steps stops with "The ViGEmBus driver is not installed". The virtual pad appears when the first `pad` step runs and everything is released when the macro ends.

### Blocks

```
loop 5                      repeat the steps up to "end" 5 times; "loop forever" never ends on its own
  key Down
  delay 100
end

if_window "Notepad"         run the steps only when the front window's title contains the text
  type Hi
end
```

Blocks nest up to 8 deep. `repeat 5` is an alias of `loop 5`.

## Input methods

Some programs accept only one kind of keyboard input, so the method is yours to pick (the **Method** button cycles through them and rewrites the `method:` line).

| Method | Sends | Use it when |
| --- | --- | --- |
| `scan` | Keys as hardware scan codes (SendInput). | Default. Best for games and DirectInput programs. |
| `vk` | Keys as virtual-key codes (SendInput). | A program ignores scan codes. |
| `legacy` | The old `keybd_event` / `mouse_event` calls. | Some older programs or remote-desktop clients behave differently with it. |
| `post` | Window messages (`WM_KEYDOWN`, `WM_CHAR`) posted to the focused control, or to the window named in `target:`. | You want to type into a window that is not in front. Many programs ignore these messages. |

`type` always uses Unicode input (so any layout and language work), except with `post`, which posts characters. Mouse steps always go through the system input, whatever the method.

Limits that no method can get around:

- A program running as administrator ignores input from a Night Island that does not run as administrator. Start Night Island as administrator for those.
- Some games and anti-cheat systems block injected input altogether.

## Limit a macro to a program or window

Without `process:` and `window:` a macro is global: its trigger works everywhere. Add one or both lines to make it work only in some places:

```
name: Reload in the game
hotkey: Mouse4
process: game.exe
window: My Game
key R
```

- `process:` is the program's file name (`game.exe`; `.exe` may be left out). It is the program that owns the window that is in front.
- `window:` is a part of the title of the front window, in any letter case.
- With both lines both must match. With `|` any of the listed names matches (`process: chrome.exe | firefox.exe`).
- The check happens when the trigger is pressed. Outside the scope nothing happens and the press goes to the program as usual, even with `swallow: yes`.
- A macro with a scope uses the same watching hook as single keys, even when its hotkey is a normal combination such as `Ctrl+Alt+1`, so that the press can be passed on when it does not belong to the macro.
- A global macro and a scoped macro may share a trigger: the scoped one wins in its program, the global one works everywhere else. Two global macros cannot share a trigger.
- The **Run** button always runs the macro: it is meant for testing. While a scoped macro runs, its trigger can stop it only when its window is in front; the **Stop** button always works.
- The island shows the scope next to the macro's name.

## Running and stopping

- **Run** in the tab (the button becomes **Stop**), or press the macro's own hotkey.
- Pressing the hotkey of a macro that is running stops it. This is the quick way to stop a macro that repeats forever.
- Only one macro runs at a time.
- A macro started from the tab waits 0.3 s so the focus can return to the previous window. A macro started with its hotkey waits (up to 2 s) until you release the modifier keys, so they do not mix into the macro.
- Hotkeys keep working when a full-screen game hides the island.
- Runaway protection: at most 5000 steps in a file, nesting 8 deep, a loop count up to 1 000 000, and a run stops after 50 million executed steps.

## Hotkeys

Press **Hotkey**, then the new key, combination or mouse button (Esc cancels, Delete clears; a mouse button is taken when you click it over the island). Or write the `hotkey:` line by hand.

- **Combination with a modifier and a key**: `Ctrl+Alt+1`, `Win+Shift+F5`, `Ctrl+Alt+Num5`. These are registered with Windows. A combination already used by the system or another program is refused and reported.
- **One key**: `F8`, `X`, `Space`, `Num0`, even a lone modifier such as `Shift`.
- **A mouse button**: `Mouse1` (left), `Mouse2` (right), `Mouse3` (middle), `Mouse4` and `Mouse5` (the side buttons). Also `Shift+Mouse4`: the modifiers must be held, extra modifiers do not matter for a hotkey without any.

Single keys and mouse buttons are watched with a low-level hook that only runs while at least one macro uses them. The macro starts when the button goes down, once per press (holding it does not repeat). The press is passed on to the program unless you add `swallow: yes`. Input that Night Island itself injects never triggers a macro, so a macro cannot start itself. Be careful with bare letters: the macro will fire whenever you type that letter, in any program. Two macros cannot share the same trigger.

## Recording

1. Press **Record**.
2. Do what you want to repeat: type, click, move the mouse, press pad buttons or move the sticks.
3. Press **Ctrl+Alt+F12**, or **Stop recording** in the tab (the last click on the button is cut off).

A new macro `Recording` appears with the steps and the pauses between them (shorter than 10 ms is dropped). Mouse movement is thinned out, a key down and up pair becomes `key`, a button press and release becomes `click`. Check the result, trim or tidy it, then run it.

Everything you type is recorded, including passwords. Do not record while typing secrets.

## Examples

**Open Notepad and type**
```
name: Hello in Notepad
hotkey: Ctrl+Alt+N

run "notepad.exe"
wait_window "Notepad" 5000
type Hello from Night Island!\nSecond line\twith a tab
```

**Autoclicker at a fixed place, until stopped**
```
name: Click here
hotkey: Ctrl+Alt+C
repeat: forever

click left 960 540
delay 80..140
```
Press the hotkey again to stop it.

**One mouse button as the trigger**
```
name: Click, Q, nudge down
hotkey: Mouse4
method: vk

mousedown left
mouseup left
down Q
up Q
move rel 0 15
```

**Hold a key, then release**
```
name: Walk forward
hotkey: Ctrl+Alt+W

down W
delay 3s
up W
```

**Drag from one place to another**
```
move 400 300
mousedown left
delay 100
move 800 300
delay 100
mouseup left
```

**Do something only in a particular window**
```
if_window "Visual Studio Code"
  key Ctrl+S
  delay 100
  key Ctrl+Shift+P
  type Format Document\n
end
```

**Repeat a sequence**
```
loop 10
  key Ctrl+V
  key Enter
  delay 200
end
```

**Type into a window that is in the background**
```
name: Background note
method: post
target: Notepad

type Written without taking the focus\n
```

**Gamepad: jump, then sprint to the right**
```
name: Jump and run
hotkey: Ctrl+Alt+J

pad A
delay 200
pad stick left 100 0
pad trigger right 100
delay 2s
pad stick left 0 0
pad trigger right 0
```

**Combine everything**
```
name: Morning routine
hotkey: Ctrl+Alt+M
repeat: 1

run "https://mail.example.com"
delay 2s
run "C:\Program Files\Notepad++\notepad++.exe"
wait_window "Notepad++" 8000
type TODO:\n
key Ctrl+S
```

## Troubleshooting

- **Nothing happens in a game.** Try `method: vk`, then `legacy`. If the game runs as administrator, start Night Island as administrator too. Some games block injected input entirely.
- **Wrong characters or layout.** Use `type` for text (it ignores the layout). `key` sends the physical key, which depends on the current layout.
- **The macro is red with `!` in the list.** The file has an error; the tab shows the line number and the reason under the editor.
- **The hotkey does nothing.** Another program may hold the combination; the tab says so. Pick another one.
- **Keys seem to stick.** Stop the macro (Stop or the hotkey again): everything held is released.
- **`The ViGEmBus driver is not installed`.** Install ViGEmBus, or remove the `pad` steps.
