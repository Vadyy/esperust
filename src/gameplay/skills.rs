#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Skill {
    None,
    #[default]
    SpMastery,
}

impl Skill {
    pub fn sp_bonus(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::SpMastery => 40.0,
        }
    }

    pub fn hud_sprite(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::SpMastery => Some("skill-sp40"),
        }
    }
}
