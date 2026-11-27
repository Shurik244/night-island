use crate::island::*;

impl App {
    pub(in crate::island) fn disarm(&mut self) {
        self.kill_timer(Timer::Disarm);
        self.ui.armed = None;
    }

    pub(in crate::island) fn click_ctl(&mut self, x: i32, y: i32) {
        let Some(i) =
            view::hit_ctl(self.pill_rect(true), self.scale, x as f32, y as f32, !self.ui.peripherals.is_empty())
        else {
            self.disarm();
            self.repaint_expanded();
            return;
        };
        let ctl = view::CTLS[i].0;
        if view::needs_confirm(ctl) && self.ui.armed != Some(i) {
            self.ui.armed = Some(i);
            self.set_timer(Timer::Disarm, 3000);
            self.repaint_expanded();
            return;
        }
        self.disarm();
        self.run_ctl(ctl);
        self.repaint_expanded();
    }

    pub(in crate::island) fn run_ctl(&mut self, ctl: view::Ctl) {
        match ctl {
            view::Ctl::WiFi => {
                if let Some(on) = self.ui.radios.wifi {
                    system::radios_async(self.hwnd, Some((RadioSwitch::WiFi, !on)));
                }
            }
            view::Ctl::Bluetooth => {
                if let Some(on) = self.ui.radios.bluetooth {
                    system::radios_async(self.hwnd, Some((RadioSwitch::Bluetooth, !on)));
                }
            }
            view::Ctl::Vpn => self.toggle_vpn(),
            view::Ctl::Theme => {
                self.ui.dark = !self.ui.dark;
                system::set_dark_theme(self.ui.dark);
            }
            view::Ctl::Mic => self.toggle_mic(),
            view::Ctl::Battery => {}
            view::Ctl::Lock => system::lock(),
            view::Ctl::Sleep => system::sleep(),
            view::Ctl::Restart => system::restart(),
            view::Ctl::PowerOff => system::power_off(),
        }
    }

    pub(in crate::island) fn on_radios(&mut self, bits: usize) {
        self.ui.radios = system::unpack(bits);
        self.update_top_panel(false);
        if self.ui.tab == view::Tab::Controls {
            self.repaint_expanded();
        }
    }
}
