use crate::island::*;
use crate::platform::memory_tools::{self, Command};

impl App {
    pub(in crate::island) fn open_memory_tools(&mut self) {
        if !self.settings.memory_tools {
            return;
        }
        self.ui.sw.memory.open = true;
        self.ui.sw.memory.busy = true;
        self.ui.sw.dropdown = None;
        self.ui.sw.field = None;
        if !memory_tools::send(self.hwnd, Command::Open) {
            self.ui.sw.memory.busy = false;
            self.ui.sw.memory.failed = true;
        }
        self.render_settings_win();
    }

    pub(in crate::island) fn close_memory_tools(&mut self) {
        if self.ui.sw.memory.open {
            self.ui.sw.memory.open = false;
            memory_tools::send(self.hwnd, Command::Close);
        }
    }

    pub(in crate::island) fn on_memory_tools(&mut self) {
        if let Some(snapshot) = memory_tools::snapshot() {
            self.ui.sw.memory.update(snapshot, &self.settings.memory_excluded);
        }
        if self.ui.sw.memory.open {
            self.render_settings_win();
        }
    }

    pub(in crate::island) fn on_memory_action(&mut self, action: view::MemoryAction) {
        use view::MemoryAction::*;
        let mut save = false;
        let command = match action {
            Back => {
                self.close_memory_tools();
                None
            }
            Refresh if !self.ui.sw.memory.busy => Some(Command::Refresh),
            Clean if !self.ui.sw.memory.busy && self.settings.memory_tools => {
                let paths = self
                    .ui
                    .sw
                    .memory
                    .selected
                    .iter()
                    .filter(|path| !self.settings.memory_excluded.contains(path))
                    .cloned()
                    .collect();
                Some(Command::Clean(paths, self.settings.memory_watch))
            }
            Watch => {
                self.settings.memory_watch = !self.settings.memory_watch;
                save = true;
                Some(Command::Watch(self.settings.memory_watch))
            }
            Select(index) if !self.ui.sw.memory.busy => {
                if let Some(app) = self.ui.sw.memory.snapshot.as_ref().and_then(|s| s.apps.get(index))
                    && app.minimized
                    && !self.settings.memory_excluded.contains(&app.path)
                    && !self.ui.sw.memory.selected.remove(&app.path)
                {
                    self.ui.sw.memory.selected.insert(app.path.clone());
                }
                None
            }
            Exclude(index) if !self.ui.sw.memory.busy => {
                if let Some(app) = self.ui.sw.memory.snapshot.as_ref().and_then(|s| s.apps.get(index)) {
                    if let Some(index) = self.settings.memory_excluded.iter().position(|p| p == &app.path) {
                        self.settings.memory_excluded.remove(index);
                    } else {
                        self.settings.memory_excluded.push(app.path.clone());
                        self.ui.sw.memory.selected.remove(&app.path);
                    }
                    save = true;
                }
                None
            }
            Sort => {
                if let Some(snapshot) = &mut self.ui.sw.memory.snapshot {
                    snapshot.apps.sort_by(|a, b| b.growth.cmp(&a.growth).then_with(|| b.working.cmp(&a.working)));
                }
                self.ui.sw.memory.page = 0;
                None
            }
            Page(page) => {
                self.ui.sw.memory.page = page;
                None
            }
            _ => None,
        };
        if let Some(command) = command {
            self.ui.sw.memory.busy = true;
            if !memory_tools::send(self.hwnd, command) {
                self.ui.sw.memory.busy = false;
                self.ui.sw.memory.failed = true;
            }
        }
        if save {
            self.ui.cfg = self.settings.clone();
            self.settings.save();
        }
        self.render_settings_win();
    }
}
