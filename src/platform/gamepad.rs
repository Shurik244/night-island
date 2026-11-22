use vigem_client::{Client, TargetId, XButtons, XGamepad, Xbox360Wired};

pub struct VirtualPad {
    target: Xbox360Wired<Client>,
    state: XGamepad,
}

fn axis(percent: i8) -> i16 {
    (percent.clamp(-100, 100) as i32 * 32767 / 100) as i16
}

impl VirtualPad {
    pub fn connect() -> Result<Self, String> {
        let client = Client::connect().map_err(|_| "The ViGEmBus driver is not installed".to_string())?;
        let mut target = Xbox360Wired::new(client, TargetId::XBOX360_WIRED);
        target.plugin().map_err(|_| "Could not create the virtual gamepad".to_string())?;
        target.wait_ready().map_err(|_| "The virtual gamepad did not become ready".to_string())?;
        Ok(Self { target, state: XGamepad::default() })
    }

    fn send(&mut self) {
        let _ = self.target.update(&self.state);
    }

    pub fn button(&mut self, mask: u16, down: bool) {
        let raw = self.state.buttons.raw;
        self.state.buttons = XButtons(if down { raw | mask } else { raw & !mask });
        self.send();
    }

    pub fn stick(&mut self, right: bool, x: i8, y: i8) {
        if right {
            self.state.thumb_rx = axis(x);
            self.state.thumb_ry = axis(y);
        } else {
            self.state.thumb_lx = axis(x);
            self.state.thumb_ly = axis(y);
        }
        self.send();
    }

    pub fn trigger(&mut self, right: bool, percent: u8) {
        let value = (percent.min(100) as u32 * 255 / 100) as u8;
        if right {
            self.state.right_trigger = value;
        } else {
            self.state.left_trigger = value;
        }
        self.send();
    }

    pub fn release_all(&mut self) {
        self.state = XGamepad::default();
        self.send();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_maps_to_the_full_axis_range() {
        assert_eq!(axis(100), 32767);
        assert_eq!(axis(-100), -32767);
        assert_eq!(axis(0), 0);
        assert_eq!(axis(50), 16383);
    }
}
