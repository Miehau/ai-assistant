use crate::db::PhaseKind;

#[test]
fn phase_transition_controller_to_executing_allowed() {
    let phase = PhaseKind::Controller;
    assert!(phase.is_valid_transition(&PhaseKind::Executing {
        step_id: "step-1".to_string(),
        tool_iteration: 0,
    }));
}

#[test]
fn phase_transition_controller_to_planning_rejected() {
    let phase = PhaseKind::Controller;
    assert!(!phase.is_valid_transition(&PhaseKind::Planning { revision: 0 }));
}
