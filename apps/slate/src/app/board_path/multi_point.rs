//! One collector for MultiPoint gestures; adapters own coordinates and outcomes.

#[derive(Clone, Copy)]
pub(crate) struct Policy {
    pub min_finish: usize,
    pub min_close: usize,
    pub append_close: bool,
}

pub(crate) const POLYLINE: Policy = Policy {
    min_finish: 2,
    min_close: 3,
    append_close: true,
};
pub(crate) const LASSO: Policy = Policy {
    min_finish: 3,
    min_close: 3,
    append_close: false,
};

pub(crate) enum Event<T> {
    Pick { point: T, close: bool, finish: bool },
    Remove,
    Finish,
    Cancel,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Transition {
    Added,
    Removed,
    Finish,
    Cancelled,
    Unchanged,
}

pub(crate) fn collect<T: Copy + PartialEq>(
    points: &mut Vec<T>,
    event: Event<T>,
    policy: Policy,
) -> Transition {
    match event {
        Event::Pick {
            point,
            close,
            finish,
        } => {
            if finish {
                return collect(points, Event::Finish, policy);
            }
            if close && points.len() >= policy.min_close {
                if policy.append_close && points.last() != points.first() {
                    points.push(points[0]);
                }
                return Transition::Finish;
            }
            if points.last() == Some(&point) {
                return Transition::Unchanged;
            }
            points.push(point);
            Transition::Added
        }
        Event::Remove => {
            if points.pop().is_some() {
                Transition::Removed
            } else {
                Transition::Unchanged
            }
        }
        Event::Finish => {
            if points.len() >= policy.min_finish {
                Transition::Finish
            } else {
                Transition::Unchanged
            }
        }
        Event::Cancel => {
            points.clear();
            Transition::Cancelled
        }
    }
}

use super::{
    bezier_anchors_to_bezpath, bezpath_to_path_data, points_to_path_data, BoardPathDraft, DrawnTips,
};
use crate::app::SlateApp;
use slate_doc::StrokeTool;

impl SlateApp {
    pub(crate) fn cancel_path_draft(&mut self) {
        if let Some(BoardPathDraft::Polyline { points, .. }) = &mut self.board_path_draft {
            collect(points, Event::Cancel, POLYLINE);
        }
        self.board_path_draft = None;
    }

    pub(crate) fn finish_path_draft(&mut self) -> bool {
        let Some(draft) = self.board_path_draft.take() else {
            return false;
        };
        let (tool, rect, path_data, closed, tips) = match draft {
            BoardPathDraft::Polyline {
                mut points,
                mut tips,
            } => {
                if collect(&mut points, Event::Finish, POLYLINE) != Transition::Finish {
                    return false;
                }
                let closed = points.len() >= 4 && points.first() == points.last();
                if closed {
                    points.pop();
                    tips.truncate(points.len());
                }
                let (r, d) = points_to_path_data(&points, closed);
                (StrokeTool::Polyline, r, d, closed, tips)
            }
            BoardPathDraft::Bezier { anchors, tips, .. } => {
                if anchors.len() < 2 {
                    return false;
                }
                let bez = bezier_anchors_to_bezpath(&anchors);
                let (r, d) = bezpath_to_path_data(&bez, false);
                (StrokeTool::Bezier, r, d, false, tips)
            }
            BoardPathDraft::Arc { .. } => return false,
        };
        if path_data.is_empty() {
            return false;
        }
        self.commit_drawn_path(tool, rect, path_data, closed, DrawnTips::Grips(&tips));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polyline_closure_retains_authored_end_point() {
        let mut points = vec![1, 2, 3];
        assert_eq!(
            collect(
                &mut points,
                Event::Pick {
                    point: 1,
                    close: true,
                    finish: false
                },
                POLYLINE
            ),
            Transition::Finish
        );
        assert_eq!(points, [1, 2, 3, 1]);
    }

    #[test]
    fn selection_closure_and_finish_do_not_add_phantom_vertices() {
        let mut points = vec![[0.0, 0.0], [1.0, 0.0]];
        assert_eq!(
            collect(&mut points, Event::Finish, LASSO),
            Transition::Unchanged
        );
        collect(
            &mut points,
            Event::Pick {
                point: [1.0, 1.0],
                close: false,
                finish: false,
            },
            LASSO,
        );
        assert_eq!(
            collect(
                &mut points,
                Event::Pick {
                    point: [0.01, 0.0],
                    close: true,
                    finish: false
                },
                LASSO
            ),
            Transition::Finish
        );
        assert_eq!(points.len(), 3);
        collect(&mut points, Event::Remove, LASSO);
        assert_eq!(points.len(), 2);
        assert_eq!(
            collect(&mut points, Event::Cancel, LASSO),
            Transition::Cancelled
        );
        assert!(points.is_empty());
    }
}
