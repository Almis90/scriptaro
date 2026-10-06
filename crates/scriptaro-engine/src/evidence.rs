use scriptaro_core::Action;

/// Evidence about backend calls, never an acknowledgement by the receiving app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectEvidence {
    pub outcome: EffectOutcome,
    pub last_operation: Option<&'static str>,
    pub characters_dispatched: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectOutcome {
    None,
    Dispatched,
    PartiallyDispatched,
    Observed,
    Uncertain,
    Simulated,
}

#[derive(Default)]
pub(crate) struct Effects {
    operation: Option<&'static str>,
    in_flight: bool,
    dispatched: bool,
    pub characters: usize,
}
impl Effects {
    pub fn begin(&mut self, operation: &'static str) {
        self.operation = Some(operation);
        self.in_flight = true;
    }
    pub fn finish(&mut self, dispatched: bool) {
        self.in_flight = false;
        self.dispatched |= dispatched;
    }
    pub fn snapshot(&self, action: &Action, success: bool, simulated: bool) -> EffectEvidence {
        let typing = matches!(action, Action::TypeText { .. });
        let outcome = if simulated {
            EffectOutcome::Simulated
        } else if self.in_flight {
            EffectOutcome::Uncertain
        } else if self.dispatched {
            if success {
                EffectOutcome::Dispatched
            } else {
                EffectOutcome::PartiallyDispatched
            }
        } else if success
            && matches!(
                action,
                Action::WaitUntil { .. } | Action::AssertControl { .. }
            )
        {
            EffectOutcome::Observed
        } else {
            EffectOutcome::None
        };
        EffectEvidence {
            outcome,
            last_operation: self.operation,
            characters_dispatched: typing.then_some(self.characters),
        }
    }
}
