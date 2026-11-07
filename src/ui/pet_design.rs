use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PetBody {
    #[default]
    Bear,
    Cat,
    Fox,
    Rabbit,
    Ghost,
    Robot,
}

impl PetBody {
    pub const ALL: [Self; 6] = [Self::Bear, Self::Cat, Self::Fox, Self::Rabbit, Self::Ghost, Self::Robot];

    pub fn next(self) -> Self {
        Self::ALL[(Self::ALL.iter().position(|body| *body == self).unwrap_or(0) + 1) % Self::ALL.len()]
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Bear => "Bear",
            Self::Cat => "Cat",
            Self::Fox => "Fox",
            Self::Rabbit => "Rabbit",
            Self::Ghost => "Ghost",
            Self::Robot => "Robot",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PetPattern {
    #[default]
    Solid,
    Patches,
    Spots,
    Stripes,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PetAccessory {
    #[default]
    None,
    Scarf,
    Bow,
    Star,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PetDesign {
    pub name: String,
    pub body: PetBody,
    pub fur: [u8; 3],
    pub belly: [u8; 3],
    pub eyes: [u8; 3],
    pub accent: [u8; 3],
    pub ear_size: f32,
    pub width: f32,
    pub pattern: PetPattern,
    pub accessory: PetAccessory,
    pub tail: bool,
}

impl Default for PetDesign {
    fn default() -> Self {
        Self {
            name: "Bear".into(),
            body: PetBody::Bear,
            fur: [150, 102, 69],
            belly: [229, 197, 152],
            eyes: [36, 29, 27],
            accent: [229, 144, 113],
            ear_size: 1.0,
            width: 1.0,
            pattern: PetPattern::Solid,
            accessory: PetAccessory::None,
            tail: false,
        }
    }
}

impl PetDesign {
    pub fn bear() -> &'static Self {
        static BEAR: std::sync::OnceLock<PetDesign> = std::sync::OnceLock::new();
        BEAR.get_or_init(Self::default)
    }

    pub fn for_character(character: super::characters::Character, custom: &Self) -> Self {
        use super::characters::Character;
        if character == Character::Custom {
            return custom.clone();
        }
        let (body, fur, belly, tail) = match character {
            Character::Cat => (PetBody::Cat, [111, 116, 139], [195, 199, 216], true),
            Character::Robot => (PetBody::Robot, [149, 164, 184], [56, 63, 82], false),
            Character::Pixel => (PetBody::Robot, [108, 205, 162], [178, 232, 203], false),
            Character::Blob => (PetBody::Ghost, [177, 160, 229], [217, 206, 250], false),
            Character::Spark | Character::Eyes => (PetBody::Ghost, [239, 239, 247], [255, 255, 255], false),
            _ => return Self::default(),
        };
        Self { name: character.name().into(), body, fur, belly, tail, ..Default::default() }
    }

    pub fn sanitize(&mut self) {
        self.name = self.name.chars().filter(|c| !c.is_control()).take(32).collect::<String>().trim().to_string();
        if self.name.is_empty() {
            self.name = "My pet".into();
        }
        self.ear_size = if self.ear_size.is_finite() { self.ear_size.clamp(0.6, 1.25) } else { 1.0 };
        self.width = if self.width.is_finite() { self.width.clamp(0.8, 1.15) } else { 1.0 };
    }

    pub fn from_reply(reply: &str) -> Result<Self, &'static str> {
        if reply.len() > 12000 {
            return Err("Could not build this pet. Try a simpler description.");
        }
        let trimmed = reply.trim();
        let json = if trimmed.starts_with("```") {
            trimmed.split_once('\n').and_then(|(_, rest)| rest.strip_suffix("```")).map(str::trim).unwrap_or(trimmed)
        } else {
            trimmed
        };
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|_| "Could not build this pet. Try a simpler description.")?;
        if !value.get("body").is_some_and(serde_json::Value::is_string) || value.get("fur").is_none() {
            return Err("Could not build this pet. Try a simpler description.");
        }
        let mut pet: Self =
            serde_json::from_value(value).map_err(|_| "Could not build this pet. Try a simpler description.")?;
        pet.sanitize();
        Ok(pet)
    }
}

pub const DESIGN_PROMPT: &str = "Create a stylized animated desktop pet from the user's description or attached image. Preserve distinctive colors, markings, ears and accessories visible in the reference. Return ONLY one JSON object with these fields: name (short name in the user's language), body (bear|cat|fox|rabbit|ghost|robot), fur ([red,green,blue] integers 0..255), belly (RGB), eyes (RGB contrasting with fur), accent (RGB), ear_size (0.6..1.25), width (0.8..1.15), pattern (solid|patches|spots|stripes), accessory (none|scarf|bow|star), tail (boolean). Choose the closest stylized body, not a photographic reproduction. Follow subsequent requested revisions to the supplied current design. Do not invent observations of images you cannot see. No code, URLs, prose or extra fields.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_designs_are_bounded_and_cannot_carry_executable_content() {
        let mut pet = PetDesign::from_reply(r#"{"body":"fox","fur":[220,100,30],"width":99,"ear_size":-7}"#).unwrap();
        assert_eq!(pet.width, 1.15);
        assert_eq!(pet.ear_size, 0.6);
        assert!(PetDesign::from_reply(r#"{"body":"fox","fur":[0,0,0],"script":"bad"}"#).is_err());
        assert!(PetDesign::from_reply("{}").is_err());
        assert!(PetDesign::from_reply("A beautiful bear").is_err());
        pet.name = "\0 ".into();
        pet.width = f32::NAN;
        pet.sanitize();
        assert_eq!(pet.name, "My pet");
        assert_eq!(pet.width, 1.0);
        let restored: PetDesign = serde_json::from_str(&serde_json::to_string(&pet).unwrap()).unwrap();
        assert_eq!(pet, restored);
    }
}
