#[derive(Debug)]
pub struct Input {
    pub universal_regions: Vec<String>,
    pub blocks: Vec<Block>,
    pub var_uses_region: Vec<(String, String)>,
    pub var_drops_region: Vec<(String, String)>,
}

impl Input {
    pub fn new(
        universal_regions: Vec<String>,
        var_uses_region: Option<Vec<(String, String)>>,
        var_drops_region: Option<Vec<(String, String)>>,
        blocks: Vec<Block>,
    ) -> Input {
        Input {
            universal_regions,
            var_uses_region: var_uses_region.unwrap_or(Vec::default()),
            var_drops_region: var_drops_region.unwrap_or(Vec::default()),
            blocks,
        }
    }
}

#[derive(Debug)]
pub struct Block {
    pub name: String,
    pub statements: Vec<Statement>,
    pub goto: Vec<String>,
}

#[derive(Debug)]
pub struct Statement {
    /// Effects destined to be emitted at the Statement's Start point
    pub effects_start: Vec<Effect>,

    /// Effects destined to be emitted at the Statement's Mid point
    pub effects: Vec<Effect>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Use { regions: Vec<String> },
    Fact(Fact),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fact {
    Outlives { a: String, b: String },
    BorrowRegionAt { region: String, loan: String },
    Invalidates { loan: String },
    Kill { loan: String },
    RegionLiveAt { region: String },
    DefineVariable { variable: String },
    UseVariable { variable: String },
}

impl Statement {
    pub(crate) fn new(effects: Vec<Effect>) -> Self {
        // Anything live on entry to the "mid point" is also live on
        // entry to the start point.
        let effects_start = effects
            .iter()
            .filter(|effect| match effect {
                Effect::Fact(Fact::RegionLiveAt { .. }) => true,
                _ => false,
            })
            .cloned()
            .collect();

        Self {
            effects_start,
            effects,
        }
    }

    pub(crate) fn with_start_effects(effects_start: Vec<Effect>, effects: Vec<Effect>) -> Self {
        Self {
            effects_start,
            effects,
        }
    }
}
