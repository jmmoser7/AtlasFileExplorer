use super::*;
use crate::presence::{
    AgentProfile, MotionDurations, MotionPhase, MotionPlan, PresenceId, PresenceRoster,
    PresenceSignal, GRAB_MIN_MS, TRAVEL_MIN_MS,
};
use std::time::Duration;

fn p(x: f64, y: f64) -> WorldPoint {
    WorldPoint::new(x, y).unwrap()
}

fn session() -> (PresenceRoster, PresenceId) {
    let mut roster = PresenceRoster::default();
    let profile_id = PresenceId::new("path-worker").unwrap();
    roster
        .register(AgentProfile {
            id: profile_id.clone(),
            name: "Path worker".into(),
            hat: None,
            retired: false,
        })
        .unwrap();
    let session_id = roster.join(&profile_id, None, p(0.0, 0.0)).unwrap();
    roster
        .session_mut(&session_id)
        .unwrap()
        .begin_request(PresenceId::new("path-request").unwrap())
        .unwrap();
    (roster, session_id)
}

fn drawing_plan(session: &crate::presence::PresenceSession, action: &str) -> MotionPlan {
    MotionPlan {
        session_id: session.id().clone(),
        request_id: session.request_id().unwrap().clone(),
        action_id: PresenceId::new(action).unwrap(),
        target_nodes: vec![PresenceId::new("stable-target").unwrap()],
        start: p(0.0, 0.0),
        end: p(0.0, 0.0),
        work_end: None,
        durations: MotionDurations {
            travel_ms: TRAVEL_MIN_MS,
            grab_ms: GRAB_MIN_MS,
            work_ms: 500,
        },
    }
}

#[test]
fn continuous_sampling_follows_arclength_around_corners_and_pen_up() {
    let path = WorkPath::new(
        vec![
            vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)],
            vec![p(20.0, 10.0), p(20.0, 20.0)],
        ],
        PathMode::Continuous,
    )
    .unwrap();
    let corner = path.position(0.25);
    assert_eq!(corner.position, p(10.0, 0.0));
    assert!(corner.pen_down);
    let gap = path.position(0.625);
    assert!(!gap.pen_down);
    assert_eq!(gap.position, p(15.0, 10.0));
    assert_ne!(gap.position, p(15.0, 5.0));
}

#[test]
fn vertex_mode_has_legible_dwell_and_keeps_travel_continuous() {
    let path = WorkPath::new(
        vec![vec![p(0.0, 0.0), p(10.0, 0.0)]],
        PathMode::VertexByVertex { dwell_ms: 100 },
    )
    .unwrap();
    let dwell = path.position(0.25);
    assert!(dwell.dwelling);
    assert_eq!(dwell.position, p(0.0, 0.0));
    let travel = path.position(0.5);
    assert!(!travel.dwelling);
    assert!(travel.position.x > 0.0 && travel.position.x < 10.0);
}

#[test]
fn mixed_strokes_keep_their_own_vertex_dwell_and_continuous_mode() {
    let path = WorkPath::new_with_modes(
        vec![
            vec![p(0.0, 0.0), p(10.0, 0.0)],
            vec![p(20.0, 0.0), p(30.0, 0.0)],
        ],
        vec![
            PathMode::Continuous,
            PathMode::VertexByVertex { dwell_ms: 100 },
        ],
    )
    .unwrap();
    assert_eq!(path.mode(), PathMode::Continuous);
    assert_eq!(path.stroke_mode(0), Some(PathMode::Continuous));
    assert_eq!(
        path.stroke_mode(1),
        Some(PathMode::VertexByVertex { dwell_ms: 100 })
    );

    // The first leg and pen-up gap are continuous. The second stroke begins
    // with a visible dwell, so a short line in a mixed drawing stays legible.
    let at_second_stroke_dwell = path.position(0.45);
    assert!(at_second_stroke_dwell.dwelling);
    assert_eq!(at_second_stroke_dwell.stroke_index, 1);
    assert_eq!(at_second_stroke_dwell.position, p(20.0, 0.0));
    let in_second_stroke_travel = path.position(0.65);
    assert!(!in_second_stroke_travel.dwelling);
    assert!(in_second_stroke_travel.position.x > 20.0);
}

#[test]
fn mixed_modes_drive_bounded_crew_costs_and_survive_cached_worker_routes() {
    let path = WorkPath::new_with_modes(
        vec![
            vec![p(0.0, 0.0), p(1.0, 0.0)],
            vec![p(10.0, 0.0), p(11.0, 0.0)],
            vec![p(20.0, 0.0), p(21.0, 0.0)],
        ],
        vec![
            PathMode::VertexByVertex { dwell_ms: 6_000 },
            PathMode::Continuous,
            PathMode::Continuous,
        ],
    )
    .unwrap();
    let crew = DrawingCrewPlan::build(&path, DRAWING_HELPER_LIMIT, 10_000);
    assert_eq!(crew.assignments().len(), 2);
    assert_eq!(crew.assignments()[0].role, WorkerRole::Parent);
    assert_eq!(crew.assignments()[1].role, WorkerRole::Helper);
    let continuous_only = WorkPath::new(path.strokes().to_vec(), PathMode::Continuous).unwrap();
    assert_eq!(
        DrawingCrewPlan::build(&continuous_only, DRAWING_HELPER_LIMIT, 10_000)
            .assignments()
            .len(),
        1
    );

    for (worker_index, assignment) in crew.assignments().iter().enumerate() {
        let route = crew.worker_path(&path, worker_index, p(-1.0, 0.0)).unwrap();
        assert_eq!(route.strokes()[0], vec![p(-1.0, 0.0)]);
        for (route_index, source_index) in assignment.stroke_indexes.iter().enumerate() {
            assert_eq!(
                route.strokes()[route_index + 1],
                path.strokes()[*source_index]
            );
            assert_eq!(
                route.stroke_mode(route_index + 1),
                path.stroke_mode(*source_index)
            );
        }
    }
}

#[test]
fn per_stroke_modes_reject_mismatched_count_and_zero_vertex_dwell() {
    assert_eq!(
        WorkPath::new_with_modes(
            vec![vec![p(0.0, 0.0)]],
            vec![PathMode::Continuous, PathMode::Continuous],
        ),
        Err(PresenceError::InvalidPath)
    );
    assert_eq!(
        WorkPath::new_with_modes(
            vec![vec![p(0.0, 0.0)]],
            vec![PathMode::VertexByVertex { dwell_ms: 0 }],
        ),
        Err(PresenceError::InvalidPath)
    );
}

#[test]
fn path_bounds_reject_empty_invalid_zero_dwell_and_oversized_inputs() {
    assert_eq!(
        WorkPath::new(vec![], PathMode::Continuous),
        Err(PresenceError::InvalidPath)
    );
    assert_eq!(
        WorkPath::new(vec![vec![]], PathMode::Continuous),
        Err(PresenceError::InvalidPath)
    );
    assert_eq!(
        WorkPath::new(
            vec![vec![p(0.0, 0.0)]],
            PathMode::VertexByVertex { dwell_ms: 0 }
        ),
        Err(PresenceError::InvalidPath)
    );
    assert_eq!(
        WorkPath::new(
            vec![vec![WorldPoint {
                x: f64::NAN,
                y: 0.0
            }]],
            PathMode::Continuous
        ),
        Err(PresenceError::InvalidPoint)
    );
    let too_many = vec![p(0.0, 0.0); WORK_PATH_POINT_LIMIT + 1];
    assert_eq!(
        WorkPath::new(vec![too_many], PathMode::Continuous),
        Err(PresenceError::PathLimit)
    );
}

#[test]
fn bounded_crew_keeps_parent_first_and_splits_strokes_deterministically() {
    let strokes = (0..12)
        .map(|n| vec![p(n as f64, 0.0), p(n as f64, 10_000.0)])
        .collect();
    let path = WorkPath::new(strokes, PathMode::Continuous).unwrap();
    let crew = DrawingCrewPlan::build(&path, 100, 20_000);
    assert_eq!(crew.budget_ms(), 10_000);
    assert_eq!(crew.assignments().len(), DRAWING_HELPER_LIMIT + 1);
    assert_eq!(crew.assignments()[0].role, WorkerRole::Parent);
    assert!(crew.assignments()[1..]
        .iter()
        .all(|worker| worker.role == WorkerRole::Helper));
    assert_eq!(crew, DrawingCrewPlan::build(&path, 100, 20_000));
    let assignment = &crew.assignments()[1];
    let route = crew.worker_path(&path, 1, p(-5.0, 0.0)).unwrap();
    assert_eq!(route.position(0.0).position, p(-5.0, 0.0));
    assert_eq!(route.strokes().len(), assignment.stroke_indexes.len() + 1);
    let source_index = *assignment.stroke_indexes.last().unwrap();
    assert_eq!(
        route.position(1.0).position,
        *path.strokes()[source_index].last().unwrap()
    );
}

#[test]
fn path_motion_cancel_settles_displayed_point_and_ready_resolves_once() {
    let (mut roster, session_id) = session();
    let presence = roster.session_mut(&session_id).unwrap();
    let plan = drawing_plan(presence, "draw-cancel");
    let path = WorkPath::new(
        vec![vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)]],
        PathMode::Continuous,
    )
    .unwrap();
    presence.enqueue_path(plan, path).unwrap();
    for _ in 0..4 {
        presence.advance(Duration::from_millis(50));
    }
    let middle = presence.advance(Duration::from_millis(50));
    assert_eq!(middle.phase, MotionPhase::Work);
    assert!(middle.progress > 0.0 && middle.progress < 1.0);
    assert_eq!(middle.position, p(2.0, 0.0));
    assert_eq!(presence.cancel_motion().len(), 1);
    assert_eq!(presence.sample().position, middle.position);
    assert_eq!(presence.sample().phase, MotionPhase::Idle);
    presence.settle_idle_position(p(0.0, 0.0)).unwrap();

    let plan = drawing_plan(presence, "draw-ready");
    let path = WorkPath::new(
        vec![vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)]],
        PathMode::Continuous,
    )
    .unwrap();
    presence.enqueue_path(plan, path).unwrap();
    for _ in 0..20 {
        presence.advance(Duration::from_millis(50));
    }
    assert_eq!(presence.sample().phase, MotionPhase::ReadyToCommit);
    assert_eq!(presence.sample().position, p(10.0, 10.0));
    assert!(matches!(
        presence.resolve_ready(|_| true),
        Some(PresenceSignal::ReadyToCommit { .. })
    ));
    assert!(presence.resolve_ready(|_| true).is_none());
}
