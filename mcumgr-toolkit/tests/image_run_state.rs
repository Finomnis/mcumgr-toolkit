mod common;

use common::firmware_update_helpers::{
    ACTIVE, CONFIRMED, OLD_HASH, OTHER_HASH, PENDING, PENDING_PERMANENT, STABLE, TARGET_HASH,
    image_state,
};
use mcumgr_toolkit::client::image_run_state::{self, ImageRunState};

#[test]
fn stable_when_active_and_confirmed_are_same_slot() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), STABLE),
        image_state(0, 1, Some(&OLD_HASH), Default::default()),
    ];

    match image_run_state::analyze(&states, 0) {
        ImageRunState::Stable(current) => assert_eq!(current.slot, 0),
        _ => panic!("expected stable state"),
    }
}

#[test]
fn pending_uses_active_as_current() {
    let states = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), PENDING),
    ];

    match image_run_state::analyze(&states, 0) {
        ImageRunState::Pending { current, next } => {
            assert_eq!(current.unwrap().slot, 0);
            assert_eq!(next.slot, 1);
            assert!(!next.permanent);
        }
        _ => panic!("expected pending state"),
    }
}

#[test]
fn pending_falls_back_to_confirmed_when_active_is_missing() {
    let states = vec![
        image_state(0, 0, Some(&OLD_HASH), CONFIRMED),
        image_state(0, 1, Some(&TARGET_HASH), PENDING),
    ];

    match image_run_state::analyze(&states, 0) {
        ImageRunState::Pending { current, next } => {
            assert_eq!(current.unwrap().slot, 0);
            assert_eq!(next.slot, 1);
        }
        _ => panic!("expected pending state"),
    }
}

#[test]
fn pending_prefers_active_over_different_confirmed_slot() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), ACTIVE),
        image_state(0, 1, Some(&OLD_HASH), CONFIRMED),
        image_state(0, 2, Some(&OTHER_HASH), PENDING),
    ];

    match image_run_state::analyze(&states, 0) {
        ImageRunState::Pending { current, next } => {
            assert_eq!(current.unwrap().slot, 0);
            assert_eq!(next.slot, 2);
        }
        _ => panic!("expected pending state"),
    }
}

#[test]
fn permanent_is_preserved_on_pending_target() {
    let states = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), PENDING_PERMANENT),
    ];

    match image_run_state::analyze(&states, 0) {
        ImageRunState::Pending { next, .. } => assert!(next.permanent),
        _ => panic!("expected pending state"),
    }
}

#[test]
fn different_active_and_confirmed_slots_mean_testing() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), ACTIVE),
        image_state(0, 1, Some(&OLD_HASH), CONFIRMED),
    ];

    match image_run_state::analyze(&states, 0) {
        ImageRunState::Testing { current, fallback } => {
            assert_eq!(current.slot, 0);
            assert_eq!(fallback.slot, 1);
        }
        _ => panic!("expected testing state"),
    }
}

#[test]
fn active_only_is_treated_as_stable() {
    let states = vec![image_state(0, 0, Some(&TARGET_HASH), ACTIVE)];

    match image_run_state::analyze(&states, 0) {
        ImageRunState::Stable(current) => assert_eq!(current.slot, 0),
        _ => panic!("expected stable state"),
    }
}

#[test]
fn confirmed_only_is_treated_as_stable() {
    let states = vec![image_state(0, 0, Some(&TARGET_HASH), CONFIRMED)];

    match image_run_state::analyze(&states, 0) {
        ImageRunState::Stable(current) => assert_eq!(current.slot, 0),
        _ => panic!("expected stable state"),
    }
}

#[test]
fn no_flags_guesses_slot_zero() {
    let states = vec![
        image_state(0, 0, None, Default::default()),
        image_state(0, 1, None, Default::default()),
    ];

    match image_run_state::analyze(&states, 0) {
        ImageRunState::Unknown(Some(guess)) => assert_eq!(guess.slot, 0),
        _ => panic!("expected slot-zero guess"),
    }
}

#[test]
fn no_slots_for_image_is_unknown_without_guess() {
    let states = vec![image_state(1, 0, Some(&OTHER_HASH), STABLE)];

    assert!(matches!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Unknown(None)
    ));
}

#[test]
fn duplicate_active_flags_in_same_image_are_unknown() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), ACTIVE),
        image_state(0, 1, Some(&OLD_HASH), ACTIVE),
    ];

    assert!(matches!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Unknown(None)
    ));
}

#[test]
fn duplicate_confirmed_flags_in_same_image_are_unknown() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), CONFIRMED),
        image_state(0, 1, Some(&OLD_HASH), CONFIRMED),
    ];

    assert!(matches!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Unknown(None)
    ));
}

#[test]
fn duplicate_pending_flags_in_same_image_are_unknown() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), PENDING),
        image_state(0, 1, Some(&OLD_HASH), PENDING),
    ];

    assert!(matches!(
        image_run_state::analyze(&states, 0),
        ImageRunState::Unknown(None)
    ));
}

#[test]
fn states_from_other_image_ids_do_not_affect_analysis() {
    let states = vec![
        image_state(0, 0, Some(&TARGET_HASH), STABLE),
        image_state(1, 0, Some(&OLD_HASH), STABLE),
        image_state(1, 1, Some(&OTHER_HASH), PENDING),
    ];

    match image_run_state::analyze(&states, 0) {
        ImageRunState::Stable(current) => {
            assert_eq!(current.image, 0);
            assert_eq!(current.slot, 0);
        }
        _ => panic!("image 1 flags must not poison image 0"),
    }
}
