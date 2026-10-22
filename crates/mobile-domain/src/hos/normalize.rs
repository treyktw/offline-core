use std::collections::HashSet;

use super::DutyEvent;

/// Filters superseded events, sorts them, and resolves overlaps.
pub fn normalize_events(events: &[DutyEvent]) -> Vec<DutyEvent> {
    let superseded: HashSet<&str> = events
        .iter()
        .filter_map(|event| event.edit_of.as_deref())
        .collect();
    let mut active: Vec<(usize, DutyEvent)> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| !superseded.contains(event.id.as_str()))
        .map(|(index, event)| (index, event.clone()))
        .collect();

    active.sort_by(|(left_index, left), (right_index, right)| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| right.source.priority().cmp(&left.source.priority()))
            .then_with(|| left_index.cmp(right_index))
    });

    let mut resolved = Vec::with_capacity(active.len());
    for index in 0..active.len() {
        let mut current = active[index].1.clone();
        if let (Some(end), Some((_, next))) = (current.ended_at, active.get(index + 1))
            && next.started_at < end
        {
            current.ended_at = Some(next.started_at);
        }
        if current
            .ended_at
            .is_some_and(|end| end <= current.started_at)
        {
            continue;
        }
        resolved.push(current);
    }
    resolved
}
