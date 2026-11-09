use rand::{seq::SliceRandom, thread_rng};

use crate::config::WAVE_COVERAGE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: usize,
    pub value: i64,
}

impl Item {
    fn key(self) -> (i64, usize) {
        (self.value, self.id)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AlgorithmStats {
    pub waves: usize,
    pub elements_recruited: usize,
    pub sort_moves: usize,
    pub merge_moves: usize,
    pub comparisons: usize,
    pub wave_checks: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlgorithmEvent {
    WaveStarted {
        wave: usize,
        percent: usize,
        target: usize,
        recruited: usize,
    },
    Recruited {
        wave: usize,
        item: Item,
    },
    BatchMove {
        wave: usize,
        item: Item,
    },
    BatchSorted {
        wave: usize,
        count: usize,
    },
    Merged {
        wave: usize,
        item: Item,
        position: usize,
    },
    WaveCompleted {
        wave: usize,
        sorted: usize,
        total: usize,
    },
    InvariantFailure {
        wave: usize,
    },
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Ready,
    Recruiting,
    Sorting,
    Merging,
    Finished,
    Failed,
}

/// A bottom-up stable merge sort which emits one actual item move per call.
/// This keeps the mobilized group's sorting visible without blocking the TUI.
struct IncrementalMergeSort {
    source: Vec<Item>,
    destination: Vec<Option<Item>>,
    moved: Vec<bool>,
    width: usize,
    run_start: usize,
    left: usize,
    middle: usize,
    right: usize,
    left_cursor: usize,
    right_cursor: usize,
    output_cursor: usize,
    run_active: bool,
    done: bool,
    comparisons: usize,
}

#[derive(Clone, Copy)]
struct SortMove {
    item: Item,
}

impl IncrementalMergeSort {
    fn new(items: Vec<Item>) -> Self {
        let len = items.len();
        Self {
            source: items,
            destination: vec![None; len],
            moved: vec![false; len],
            width: 1,
            run_start: 0,
            left: 0,
            middle: 0,
            right: 0,
            left_cursor: 0,
            right_cursor: 0,
            output_cursor: 0,
            run_active: false,
            done: len < 2,
            comparisons: 0,
        }
    }

    fn step(&mut self) -> Option<SortMove> {
        let len = self.source.len();
        if self.done || len < 2 {
            self.done = true;
            return None;
        }

        loop {
            if self.width >= len {
                self.done = true;
                return None;
            }

            if !self.run_active {
                if self.run_start >= len {
                    let mut next_source = Vec::with_capacity(len);
                    for slot in &mut self.destination {
                        if let Some(item) = slot.take() {
                            next_source.push(item);
                        }
                    }
                    if next_source.len() != len {
                        self.done = true;
                        return None;
                    }
                    self.source = next_source;
                    self.destination = vec![None; len];
                    self.moved = vec![false; len];
                    self.width = self.width.saturating_mul(2);
                    self.run_start = 0;
                    if self.width >= len {
                        self.done = true;
                        return None;
                    }
                }

                self.left = self.run_start;
                self.middle = self.left.saturating_add(self.width).min(len);
                self.right = self.left.saturating_add(self.width.saturating_mul(2)).min(len);
                self.left_cursor = self.left;
                self.right_cursor = self.middle;
                self.output_cursor = self.left;
                self.run_active = true;
            }

            if self.output_cursor >= self.right {
                self.run_start = self.right;
                self.run_active = false;
                continue;
            }

            let source_index = if self.left_cursor < self.middle
                && (self.right_cursor >= self.right
                    || self.source[self.left_cursor].key() <= self.source[self.right_cursor].key())
            {
                if self.right_cursor < self.right {
                    self.comparisons += 1;
                }
                let index = self.left_cursor;
                self.left_cursor += 1;
                index
            } else {
                if self.left_cursor < self.middle && self.right_cursor < self.right {
                    self.comparisons += 1;
                }
                let index = self.right_cursor;
                self.right_cursor += 1;
                index
            };

            let item = self.source[source_index];
            self.destination[self.output_cursor] = Some(item);
            self.moved[source_index] = true;
            self.output_cursor += 1;
            return Some(SortMove { item });
        }
    }

    fn into_sorted(self) -> Vec<Item> {
        self.source
    }

    fn visual_items(&self) -> Vec<Option<Item>> {
        let mut items = Vec::with_capacity(self.source.len());
        items.extend(self.destination.iter().filter_map(|item| *item).map(Some));
        items.extend(
            self.source
                .iter()
                .enumerate()
                .filter(|(index, _)| !self.moved[*index])
                .map(|(_, item)| Some(*item)),
        );
        items
    }
}

struct MergeState {
    left: Vec<Item>,
    right: Vec<Item>,
    left_cursor: usize,
    right_cursor: usize,
    output: Vec<Item>,
    comparisons: usize,
}

impl MergeState {
    fn new(left: Vec<Item>, right: Vec<Item>) -> Self {
        let capacity = left.len().saturating_add(right.len());
        Self {
            left,
            right,
            left_cursor: 0,
            right_cursor: 0,
            output: Vec::with_capacity(capacity),
            comparisons: 0,
        }
    }

    fn step(&mut self) -> Option<Item> {
        let item = if self.left_cursor < self.left.len()
            && self.right_cursor < self.right.len()
        {
            self.comparisons += 1;
            if self.left[self.left_cursor].key() <= self.right[self.right_cursor].key() {
                let item = self.left[self.left_cursor];
                self.left_cursor += 1;
                item
            } else {
                let item = self.right[self.right_cursor];
                self.right_cursor += 1;
                item
            }
        } else if self.left_cursor < self.left.len() {
            let item = self.left[self.left_cursor];
            self.left_cursor += 1;
            item
        } else if self.right_cursor < self.right.len() {
            let item = self.right[self.right_cursor];
            self.right_cursor += 1;
            item
        } else {
            return None;
        };

        self.output.push(item);
        Some(item)
    }

    fn remaining_items(&self) -> Vec<Option<Item>> {
        self.left[self.left_cursor..]
            .iter()
            .chain(self.right[self.right_cursor..].iter())
            .copied()
            .map(Some)
            .collect()
    }
}

/// Partial Mobilization Sort.
///
/// The percentages are cumulative coverage targets of the original input. Each wave
/// recruits only new items from reserve, sorts that batch, then merges it into the
/// already sorted formation. Every mutation is exposed as a step/event for animation.
pub struct MobilizationSort {
    original: Vec<i64>,
    reserve: Vec<Option<Item>>,
    formation: Vec<Item>,
    batch: Vec<Item>,
    selected_positions: Vec<usize>,
    selection_cursor: usize,
    sorter: Option<IncrementalMergeSort>,
    merger: Option<MergeState>,
    phase: Phase,
    wave_index: usize,
    current_wave: usize,
    current_target: usize,
    stats: AlgorithmStats,
}

impl MobilizationSort {
    pub fn new(values: Vec<i64>) -> Self {
        let original = values;
        let reserve = original
            .iter()
            .enumerate()
            .map(|(id, value)| Some(Item { id, value: *value }))
            .collect();
        Self {
            original,
            reserve,
            formation: Vec::new(),
            batch: Vec::new(),
            selected_positions: Vec::new(),
            selection_cursor: 0,
            sorter: None,
            merger: None,
            phase: Phase::Ready,
            wave_index: 0,
            current_wave: 0,
            current_target: 0,
            stats: AlgorithmStats::default(),
        }
    }

    pub fn original_values(&self) -> &[i64] {
        &self.original
    }

    pub fn reserve_view(&self) -> &[Option<Item>] {
        &self.reserve
    }

    pub fn formation_view(&self) -> &[Item] {
        self.merger
            .as_ref()
            .map_or(self.formation.as_slice(), |merge| merge.output.as_slice())
    }

    pub fn batch_view(&self) -> Vec<Option<Item>> {
        if let Some(sorter) = &self.sorter {
            return sorter.visual_items();
        }
        if let Some(merge) = &self.merger {
            return merge.remaining_items();
        }
        self.batch.iter().copied().map(Some).collect()
    }

    pub fn stats(&self) -> AlgorithmStats {
        self.stats
    }

    pub fn total(&self) -> usize {
        self.original.len()
    }

    pub fn remaining(&self) -> usize {
        self.reserve.iter().filter(|item| item.is_some()).count()
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.phase, Phase::Finished | Phase::Failed)
    }

    pub fn is_sorted(&self) -> bool {
        self.phase == Phase::Finished
            && self.remaining() == 0
            && self.formation.len() == self.original.len()
            && self
                .formation
                .windows(2)
                .all(|pair| pair[0].key() <= pair[1].key())
    }

    pub fn result_values(&self) -> Vec<i64> {
        self.formation.iter().map(|item| item.value).collect()
    }

    /// Performs one visible state transition. Work that does not move an element
    /// (such as skipping a duplicate rounded coverage target) is folded into this call.
    pub fn step(&mut self) -> AlgorithmEvent {
        loop {
            match self.phase {
                Phase::Ready => {
                    let next = self.prepare_wave();
                    let Some((wave, percent, target, recruited, positions)) = next else {
                        self.phase = Phase::Finished;
                        return AlgorithmEvent::Finished;
                    };
                    self.current_wave = wave;
                    self.current_target = target;
                    self.selected_positions = positions;
                    self.selection_cursor = 0;
                    self.stats.waves += 1;
                    self.phase = Phase::Recruiting;
                    return AlgorithmEvent::WaveStarted {
                        wave,
                        percent,
                        target,
                        recruited,
                    };
                }
                Phase::Recruiting => {
                    if self.selection_cursor >= self.selected_positions.len() {
                        let items = std::mem::take(&mut self.batch);
                        self.sorter = Some(IncrementalMergeSort::new(items));
                        self.phase = Phase::Sorting;
                        continue;
                    }

                    let position = self.selected_positions[self.selection_cursor];
                    self.selection_cursor += 1;
                    let Some(item) = self.reserve.get_mut(position).and_then(Option::take) else {
                        continue;
                    };
                    self.batch.push(item);
                    self.stats.elements_recruited += 1;
                    return AlgorithmEvent::Recruited {
                        wave: self.current_wave,
                        item,
                    };
                }
                Phase::Sorting => {
                    let movement = self.sorter.as_mut().and_then(IncrementalMergeSort::step);
                    if let Some(movement) = movement {
                        self.stats.sort_moves += 1;
                        return AlgorithmEvent::BatchMove {
                            wave: self.current_wave,
                            item: movement.item,
                        };
                    }

                    if let Some(sorter) = self.sorter.take() {
                        self.stats.comparisons += sorter.comparisons;
                        self.batch = sorter.into_sorted();
                    }
                    let batch_count = self.batch.len();
                    let left = std::mem::take(&mut self.formation);
                    let right = std::mem::take(&mut self.batch);
                    self.merger = Some(MergeState::new(left, right));
                    self.phase = Phase::Merging;
                    return AlgorithmEvent::BatchSorted {
                        wave: self.current_wave,
                        count: batch_count,
                    };
                }
                Phase::Merging => {
                    let movement = self.merger.as_mut().and_then(MergeState::step);
                    if let Some(item) = movement {
                        let position = self.merger.as_ref().map_or(0, |merge| merge.output.len() - 1);
                        self.stats.merge_moves += 1;
                        return AlgorithmEvent::Merged {
                            wave: self.current_wave,
                            item,
                            position,
                        };
                    }

                    let Some(merge) = self.merger.take() else {
                        self.phase = Phase::Ready;
                        continue;
                    };
                    self.stats.comparisons += merge.comparisons;
                    self.formation = merge.output;
                    let wave = self.current_wave;
                    self.stats.wave_checks += 1;
                    if !self.validate_current_wave() {
                        self.phase = Phase::Failed;
                        return AlgorithmEvent::InvariantFailure { wave };
                    }
                    let sorted = self.formation.len();
                    let total = self.original.len();
                    self.wave_index += 1;
                    self.phase = Phase::Ready;
                    return AlgorithmEvent::WaveCompleted { wave, sorted, total };
                }
                Phase::Finished => return AlgorithmEvent::Finished,
                Phase::Failed => return AlgorithmEvent::InvariantFailure {
                    wave: self.current_wave,
                },
            }
        }
    }

    fn validate_current_wave(&self) -> bool {
        if self.formation.len() != self.current_target
            || self
                .formation
                .windows(2)
                .any(|pair| pair[0].key() > pair[1].key())
        {
            return false;
        }

        let mut seen = vec![false; self.original.len()];
        for item in self
            .formation
            .iter()
            .chain(self.reserve.iter().filter_map(Option::as_ref))
        {
            if item.id >= self.original.len()
                || seen[item.id]
                || self.original[item.id] != item.value
            {
                return false;
            }
            seen[item.id] = true;
        }
        seen.into_iter().all(|present| present)
    }

    fn prepare_wave(&mut self) -> Option<(usize, usize, usize, usize, Vec<usize>)> {
        while self.wave_index < WAVE_COVERAGE.len() {
            let percent = WAVE_COVERAGE[self.wave_index];
            let target = coverage_target(self.original.len(), percent);
            if target <= self.formation.len() {
                // Small arrays can round adjacent cumulative targets to the same count.
                self.wave_index += 1;
                continue;
            }

            let available = self.remaining();
            let recruited = target.saturating_sub(self.formation.len()).min(available);
            if recruited == 0 {
                self.wave_index += 1;
                continue;
            }

            let mut positions: Vec<usize> = self
                .reserve
                .iter()
                .enumerate()
                .filter_map(|(index, item)| item.as_ref().map(|_| index))
                .collect();
            positions.shuffle(&mut thread_rng());
            positions.truncate(recruited);
            return Some((
                self.wave_index + 1,
                percent,
                target,
                recruited,
                positions,
            ));
        }
        None
    }
}

fn coverage_target(total: usize, percent: usize) -> usize {
    let numerator = (total as u128) * (percent as u128);
    ((numerator + 99) / 100).min(total as u128) as usize
}

#[cfg(test)]
mod tests {
    use super::{AlgorithmEvent, MobilizationSort};

    fn finish(values: Vec<i64>) -> MobilizationSort {
        let mut algorithm = MobilizationSort::new(values);
        for _ in 0..200_000 {
            if algorithm.is_finished() {
                break;
            }
            algorithm.step();
        }
        assert!(algorithm.is_finished(), "algorithm did not terminate");
        assert!(algorithm.is_sorted(), "output is not sorted");
        algorithm
    }

    #[test]
    fn handles_empty_singleton_duplicates_and_negative_values() {
        assert_eq!(finish(Vec::new()).result_values(), Vec::<i64>::new());
        assert_eq!(finish(vec![i64::MIN]).result_values(), vec![i64::MIN]);
        assert_eq!(
            finish(vec![4, -3, 4, 0, -3, i64::MAX]).result_values(),
            vec![-3, -3, 0, 4, 4, i64::MAX]
        );
    }

    #[test]
    fn waves_reach_the_cumulative_coverage_targets() {
        let mut algorithm = MobilizationSort::new((0..50).rev().collect());
        let mut starts = Vec::new();
        while !algorithm.is_finished() {
            if let AlgorithmEvent::WaveStarted { target, recruited, .. } = algorithm.step() {
                starts.push((target, recruited));
            }
        }
        assert_eq!(starts, vec![(5, 5), (13, 8), (25, 12), (38, 13), (50, 12)]);
        assert_eq!(algorithm.stats().wave_checks, 5);
        assert!(algorithm.is_sorted());
    }

    #[test]
    fn sorts_a_large_input_without_losing_elements() {
        let values: Vec<i64> = (0..3_000)
            .map(|index| ((index * 7_919) % 4_003) as i64 - 2_001)
            .collect();
        let expected_count = values.len();
        let algorithm = finish(values);
        assert_eq!(algorithm.result_values().len(), expected_count);
        assert_eq!(algorithm.stats().elements_recruited, expected_count);
    }
}
