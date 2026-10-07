mod common;

use common::firmware_update_helpers::{
    ACTIVE, CONFIRMED, OLD_HASH, OTHER_HASH, PENDING, PENDING_PERMANENT, STABLE, TARGET_HASH,
    image_state,
};
use mcumgr_toolkit::client::image_run_state::{self, ImageRunState};

// Typical Zephyr/MCUmgr state reports

#[test]
fn stable_when_active_and_confirmed_are_same_slot() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), STABLE),
        image_state(0, 1, Some(&OLD_HASH), Default::default()),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Stable(&states[0])
    );
}

#[test]
fn stable_in_secondary_slot() {
    // e.g. MCUboot direct-xip, where the application may run from slot 1
    let states = vec![
        image_state(0, 0, Some(&OLD_HASH), Default::default()),
        image_state(0, 1, Some(&TARGET_HASH), STABLE),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Stable(&states[1])
    );
}

#[test]
fn pending_uses_active_as_current() {
    let states = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), PENDING),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Pending {
            current: Some(&states[0]),
            next: &states[1],
        }
    );
}

#[test]
fn pending_falls_back_to_confirmed_when_active_is_missing() {
    let states = vec![
        image_state(0, 0, Some(&OLD_HASH), CONFIRMED),
        image_state(0, 1, Some(&TARGET_HASH), PENDING),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Pending {
            current: Some(&states[0]),
            next: &states[1],
        }
    );
}

#[test]
fn pending_without_active_or_confirmed_has_no_current() {
    let states = vec![
        image_state(0, 0, Some(&OLD_HASH), Default::default()),
        image_state(0, 1, Some(&TARGET_HASH), PENDING),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Pending {
            current: None,
            next: &states[1],
        }
    );
}

#[test]
fn pending_prefers_active_over_different_confirmed_slot() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), ACTIVE),
        image_state(0, 1, Some(&OLD_HASH), CONFIRMED),
        image_state(0, 2, Some(&OTHER_HASH), PENDING),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Pending {
            current: Some(&states[0]),
            next: &states[2],
        }
    );
}

#[test]
fn permanent_is_preserved_on_pending_target() {
    let states = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), PENDING_PERMANENT),
    ];

    match image_run_state::analyze(&states, 0) {
        ImageRunState::Pending { next, .. } => assert!(next.permanent),
        other => panic!("expected pending state, got {other:?}"),
    }
}

#[test]
fn different_active_and_confirmed_slots_mean_testing() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), ACTIVE),
        image_state(0, 1, Some(&OLD_HASH), CONFIRMED),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Testing {
            current: &states[0],
            fallback: &states[1],
        }
    );
}

#[test]
fn active_only_is_treated_as_stable() {
    let states = vec![image_state(0, 0, Some(&TARGET_HASH), ACTIVE)];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Stable(&states[0])
    );
}

#[test]
fn confirmed_only_is_treated_as_stable() {
    let states = vec![image_state(0, 0, Some(&TARGET_HASH), CONFIRMED)];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Stable(&states[0])
    );
}

// Missing state flags (e.g. MCUboot serial recovery)

#[test]
fn no_flags_guesses_slot_zero() {
    let states = vec![
        image_state(0, 1, None, Default::default()),
        image_state(0, 0, None, Default::default()),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Unknown(Some(&states[1]))
    );
}

#[test]
fn empty_state_is_unknown_without_guess() {
    assert_eq!(
        image_run_state::analyze(&[], 0),
        ImageRunState::Unknown(None)
    );
}

#[test]
fn no_slots_for_image_is_unknown_without_guess() {
    let states = vec![image_state(1, 0, Some(&OTHER_HASH), STABLE)];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Unknown(None)
    );
}

// Contradicting state flags

#[test]
fn duplicate_active_flags_in_same_image_are_inconsistent() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), ACTIVE),
        image_state(0, 1, Some(&OLD_HASH), ACTIVE),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Inconsistent
    );
}

#[test]
fn duplicate_confirmed_flags_in_same_image_are_inconsistent() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), CONFIRMED),
        image_state(0, 1, Some(&OLD_HASH), CONFIRMED),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Inconsistent
    );
}

#[test]
fn duplicate_pending_flags_in_same_image_are_inconsistent() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), PENDING),
        image_state(0, 1, Some(&OLD_HASH), PENDING),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Inconsistent
    );
}

#[test]
fn inconsistency_takes_precedence_over_pending() {
    let states = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), PENDING),
        image_state(0, 2, Some(&OTHER_HASH), ACTIVE),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Inconsistent
    );
}

// Multi-image systems

#[test]
fn states_from_other_image_ids_do_not_affect_analysis() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), STABLE),
        image_state(1, 0, Some(&OLD_HASH), STABLE),
        image_state(1, 1, Some(&OTHER_HASH), PENDING),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Stable(&states[0])
    );
}

#[test]
fn inconsistent_other_image_does_not_affect_analysis() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), STABLE),
        image_state(1, 0, Some(&OLD_HASH), ACTIVE),
        image_state(1, 1, Some(&OTHER_HASH), ACTIVE),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Stable(&states[0])
    );
    assert_eq!(
        image_run_state::analyze(&states, 1),
        ImageRunState::Inconsistent
    );
}

#[test]
fn non_zero_image_id_is_analyzed() {
    let states = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(1, 0, Some(&TARGET_HASH), ACTIVE),
        image_state(1, 1, Some(&OLD_HASH), CONFIRMED),
    ];

    assert_eq!(
        image_run_state::analyze(&states, 1),
        ImageRunState::Testing {
            current: &states[1],
            fallback: &states[2],
        }
    );
}
