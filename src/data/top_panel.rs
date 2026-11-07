use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelEntrance {
    #[default]
    Island,
    Sides,
    Top,
}

impl PanelEntrance {
    pub const ALL: [Self; 3] = [Self::Island, Self::Sides, Self::Top];

    pub fn name(self) -> &'static str {
        match self {
            Self::Island => "From the island",
            Self::Sides => "From screen sides",
            Self::Top => "From the top edge",
        }
    }

    pub fn step(self, forward: bool) -> Self {
        let index = Self::ALL.iter().position(|value| *value == self).unwrap_or(0);
        Self::ALL[(index + if forward { 1 } else { 2 }) % 3]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TopPanelSettings {
    pub enabled: bool,
    pub application: bool,
    pub auto_hide: bool,
    pub tray: bool,
    pub wifi: bool,
    pub bluetooth: bool,
    pub volume: bool,
    pub battery: bool,
    pub hide_fullscreen: bool,
    pub reserve_space: bool,
    pub clear_window_controls: bool,
    pub microphone: bool,
    pub keyboard: bool,
    pub workspaces: bool,
    pub animations: bool,
    pub entrance: PanelEntrance,
    pub island_gap: u32,
    pub motion_ms: u32,
    pub liquid: bool,
}

impl Default for TopPanelSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            application: true,
            auto_hide: true,
            tray: true,
            wifi: true,
            bluetooth: true,
            volume: true,
            battery: true,
            hide_fullscreen: true,
            reserve_space: true,
            clear_window_controls: true,
            microphone: false,
            keyboard: false,
            workspaces: false,
            animations: true,
            entrance: PanelEntrance::Island,
            island_gap: 20,
            motion_ms: 360,
            liquid: true,
        }
    }
}

impl TopPanelSettings {
    pub fn sanitize(&mut self) {
        self.island_gap = self.island_gap.clamp(12, 160);
        self.motion_ms = self.motion_ms.clamp(160, 800);
    }

    pub fn has_blocks(&self) -> bool {
        self.application
            || self.tray
            || self.wifi
            || self.bluetooth
            || self.volume
            || self.battery
            || self.microphone
            || self.keyboard
            || self.workspaces
    }
}
