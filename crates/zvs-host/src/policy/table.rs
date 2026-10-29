use zvs_contract::ExecMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Auto,
    Benign,
    Files,
    Browser,
    Process,
    Declared,
    Floor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Run,
    Ask,
    Review,
    Deny,
}

pub const fn decide(mode: ExecMode, family: Family) -> Decision {
    match (mode, family) {
        (_, Family::Floor) => Decision::Deny,
        (_, Family::Auto) | (ExecMode::NoRestrictions, _) => Decision::Run,
        (ExecMode::AlwaysAsk, _) => Decision::Ask,
        (_, Family::Benign) => Decision::Run,
        (ExecMode::AutoBypass, Family::Files | Family::Process) => Decision::Review,
        _ => Decision::Ask,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODES: [ExecMode; 4] = [
        ExecMode::AlwaysAsk,
        ExecMode::SafetyAsk,
        ExecMode::AutoBypass,
        ExecMode::NoRestrictions,
    ];

    fn row(family: Family) -> [Decision; 4] {
        MODES.map(|mode| decide(mode, family))
    }

    #[test]
    fn every_cell_of_the_passport_table_is_enforced() {
        use Decision::{Ask, Deny, Review, Run};
        assert_eq!(row(Family::Auto), [Run, Run, Run, Run]);
        assert_eq!(row(Family::Benign), [Ask, Run, Run, Run]);
        assert_eq!(row(Family::Files), [Ask, Ask, Review, Run]);
        assert_eq!(row(Family::Browser), [Ask, Ask, Ask, Run]);
        assert_eq!(row(Family::Process), [Ask, Ask, Review, Run]);
        assert_eq!(row(Family::Floor), [Deny, Deny, Deny, Deny]);
    }

    #[test]
    fn a_declared_destructive_tool_of_unknown_effect_asks_until_restrictions_are_off() {
        use Decision::{Ask, Run};
        assert_eq!(row(Family::Declared), [Ask, Ask, Ask, Run]);
    }

    #[test]
    fn no_mode_reaches_the_floor() {
        for mode in MODES {
            assert_eq!(decide(mode, Family::Floor), Decision::Deny, "{mode}");
        }
    }
}
