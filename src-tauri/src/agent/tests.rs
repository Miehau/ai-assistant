use crate::db::PhaseKind;

#[test]
fn phase_transition_triage_to_planning_allowed() {
    let phase = PhaseKind::Triage;
    assert!(phase.is_valid_transition(&PhaseKind::Planning { revision: 0 }));
}

#[test]
fn phase_transition_triage_to_executing_rejected() {
    let phase = PhaseKind::Triage;
    assert!(!phase.is_valid_transition(&PhaseKind::Executing {
        step_id: "step-1".to_string(),
        tool_iteration: 0,
    }));
}
