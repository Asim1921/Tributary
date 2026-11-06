//! Tracks which bytes of a file are done, in progress or still free. No I/O happens here.
//!
//! Each part keeps two positions. `fetched` counts bytes received from the network,
//! which may still be sitting in a buffer. `written` counts bytes that reached the
//! file. Splitting uses `fetched`, so a split never takes bytes a connection already
//! holds. Saving uses `written`, so a checkpoint never claims bytes that aren't there.

/// A saved part of a file: `start..end`, of which the first `written` bytes are done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub written: u64,
}

/// Work handed to one connection: fetch `from..end`, unless `end` later shrinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claim {
    pub index: usize,
    pub from: u64,
    pub end: u64,
}

#[derive(Debug)]
struct Part {
    start: u64,
    end: u64,
    fetched: u64,
    written: u64,
    owned: bool,
}

impl Part {
    fn new(start: u64, end: u64, written: u64) -> Self {
        Self {
            start,
            end,
            fetched: written,
            written,
            owned: false,
        }
    }

    fn length(&self) -> u64 {
        self.end - self.start
    }

    fn is_done(&self) -> bool {
        self.written >= self.length()
    }

    /// Bytes not yet received.
    fn unfetched(&self) -> u64 {
        self.length() - self.fetched
    }
}

#[derive(Debug)]
pub struct SegmentMap {
    parts: Vec<Part>,
    min_split: u64,
}

impl SegmentMap {
    /// Divides a file evenly, using fewer parts than `connections` if it's small.
    pub fn new(size: u64, connections: usize, min_split: u64) -> Self {
        let by_size = size.div_ceil(min_split.max(1)).max(1);
        let count = (connections.max(1) as u64).min(by_size);
        let base = size / count;

        let parts = (0..count)
            .map(|index| {
                let end = if index + 1 == count {
                    size
                } else {
                    (index + 1) * base
                };
                Part::new(index * base, end, 0)
            })
            .collect();

        Self { parts, min_split }
    }

    /// Rebuilds the map from saved segments, with nothing in progress.
    pub fn from_saved(segments: &[Segment], min_split: u64) -> Self {
        let parts = segments
            .iter()
            .map(|segment| Part::new(segment.start, segment.end, segment.written))
            .collect();
        Self { parts, min_split }
    }

    /// Hands out free work. When none is left, splits the part with the most
    /// remaining, so a finished connection helps a slower one.
    pub fn claim(&mut self) -> Option<Claim> {
        if let Some(index) = self
            .parts
            .iter()
            .position(|part| !part.owned && !part.is_done())
        {
            let part = &mut self.parts[index];
            part.owned = true;
            part.fetched = part.written;
            return Some(Claim {
                index,
                from: part.start + part.written,
                end: part.end,
            });
        }

        let (index, remaining) = self
            .parts
            .iter()
            .enumerate()
            .filter(|(_, part)| part.owned)
            .map(|(index, part)| (index, part.unfetched()))
            .max_by_key(|&(_, remaining)| remaining)?;
        if remaining < self.min_split.saturating_mul(2) {
            return None;
        }

        let part = &mut self.parts[index];
        let old_end = part.end;
        let middle = part.start + part.fetched + remaining / 2;
        part.end = middle;

        let mut new_part = Part::new(middle, old_end, 0);
        new_part.owned = true;
        self.parts.push(new_part);

        Some(Claim {
            index: self.parts.len() - 1,
            from: middle,
            end: old_end,
        })
    }

    /// Records bytes received and returns how many belong to this part. The rest
    /// now belong to another part, because this one was split.
    pub fn accept(&mut self, index: usize, received: usize) -> usize {
        let part = &mut self.parts[index];
        let taken = (received as u64).min(part.unfetched());
        part.fetched += taken;
        taken as usize
    }

    /// Records bytes that reached the file.
    pub fn commit(&mut self, index: usize, count: u64) {
        self.parts[index].written += count;
    }

    /// Ends a connection's ownership. Any bytes received but not written are forgotten.
    pub fn release(&mut self, index: usize) {
        let part = &mut self.parts[index];
        part.owned = false;
        part.fetched = part.written;
    }

    /// Bytes this part hasn't received yet.
    pub fn unfetched(&self, index: usize) -> u64 {
        self.parts[index].unfetched()
    }

    pub fn written(&self, index: usize) -> u64 {
        self.parts[index].written
    }

    /// Whether any connection is working on this file.
    pub fn has_owners(&self) -> bool {
        self.parts.iter().any(|part| part.owned)
    }

    pub fn is_complete(&self) -> bool {
        self.parts.iter().all(Part::is_done)
    }

    pub fn total_written(&self) -> u64 {
        self.parts.iter().map(|part| part.written).sum()
    }

    /// The state to save.
    pub fn snapshot(&self) -> Vec<Segment> {
        self.parts
            .iter()
            .map(|part| Segment {
                start: part.start,
                end: part.end,
                written: part.written,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1 << 20;

    fn ranges(map: &SegmentMap) -> Vec<(u64, u64)> {
        map.snapshot().iter().map(|s| (s.start, s.end)).collect()
    }

    /// Fetches and writes `count` bytes for a claim, as a connection would.
    fn advance(map: &mut SegmentMap, index: usize, count: usize) {
        let taken = map.accept(index, count);
        map.commit(index, taken as u64);
    }

    #[test]
    fn divides_a_file_evenly_and_gives_the_remainder_to_the_last_part() {
        let map = SegmentMap::new(10 * MIB + 3, 4, MIB);
        let parts = ranges(&map);

        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0].0, 0);
        assert_eq!(parts[3].1, 10 * MIB + 3);
        for pair in parts.windows(2) {
            assert_eq!(pair[0].1, pair[1].0, "parts must touch");
        }
    }

    #[test]
    fn small_files_use_fewer_connections() {
        assert_eq!(SegmentMap::new(MIB / 2, 8, MIB).snapshot().len(), 1);
        assert_eq!(SegmentMap::new(3 * MIB, 8, MIB).snapshot().len(), 3);
    }

    #[test]
    fn an_empty_file_is_already_complete() {
        let mut map = SegmentMap::new(0, 8, MIB);
        assert!(map.is_complete());
        assert_eq!(map.claim(), None);
    }

    #[test]
    fn hands_out_each_free_part_once() {
        let mut map = SegmentMap::new(8 * MIB, 2, MIB);
        let first = map.claim().unwrap();
        let second = map.claim().unwrap();

        assert_eq!((first.from, first.end), (0, 4 * MIB));
        assert_eq!((second.from, second.end), (4 * MIB, 8 * MIB));
    }

    #[test]
    fn a_finished_connection_takes_half_of_the_largest_remaining_part() {
        let mut map = SegmentMap::new(8 * MIB, 2, MIB);
        let first = map.claim().unwrap();
        let second = map.claim().unwrap();

        advance(&mut map, first.index, (4 * MIB) as usize);
        map.release(first.index);
        advance(&mut map, second.index, MIB as usize);

        let split = map.claim().unwrap();
        assert_eq!(split.from, 6 * MIB + MIB / 2);
        assert_eq!(split.end, 8 * MIB);
    }

    #[test]
    fn splits_never_take_bytes_a_connection_already_received() {
        let mut map = SegmentMap::new(4 * MIB, 1, MIB);
        let claim = map.claim().unwrap();

        map.accept(claim.index, (2 * MIB) as usize);
        let split = map.claim().unwrap();

        assert_eq!(split.from, 3 * MIB);
        assert_eq!(map.written(claim.index), 0);
    }

    #[test]
    fn does_not_split_below_the_minimum_size() {
        let mut map = SegmentMap::new(2 * MIB - 1, 1, MIB);
        map.claim().unwrap();
        assert_eq!(map.claim(), None);
    }

    #[test]
    fn a_split_part_stops_accepting_at_its_new_end() {
        let mut map = SegmentMap::new(4 * MIB, 1, MIB);
        let claim = map.claim().unwrap();
        map.claim().unwrap();

        let taken = map.accept(claim.index, (3 * MIB) as usize);
        assert_eq!(taken as u64, 2 * MIB);
        assert_eq!(map.unfetched(claim.index), 0);
    }

    #[test]
    fn a_released_part_restarts_from_what_was_written() {
        let mut map = SegmentMap::new(4 * MIB, 1, MIB);
        let claim = map.claim().unwrap();

        map.accept(claim.index, 1000);
        map.commit(claim.index, 600);
        map.release(claim.index);

        assert_eq!(map.claim().unwrap().from, 600);
    }

    #[test]
    fn saved_progress_resumes_where_it_stopped() {
        let saved = [
            Segment {
                start: 0,
                end: 100,
                written: 100,
            },
            Segment {
                start: 100,
                end: 200,
                written: 40,
            },
        ];
        let mut map = SegmentMap::from_saved(&saved, 10);

        assert_eq!(map.total_written(), 140);
        assert_eq!(
            map.claim(),
            Some(Claim {
                index: 1,
                from: 140,
                end: 200
            })
        );
    }

    #[test]
    fn tracks_ownership_and_completion() {
        let mut map = SegmentMap::new(100, 1, 10);
        assert!(!map.has_owners());

        let claim = map.claim().unwrap();
        assert!(map.has_owners());

        advance(&mut map, claim.index, 100);
        map.release(claim.index);
        assert!(!map.has_owners());
        assert!(map.is_complete());
    }

    #[test]
    fn every_byte_is_covered_exactly_once_through_repeated_splits() {
        let size = 64 * MIB + 12_345;
        let mut map = SegmentMap::new(size, 4, MIB / 4);
        let mut active: Vec<Claim> = (0..4).map(|_| map.claim().unwrap()).collect();
        let mut turn = 0;

        while !active.is_empty() {
            let slot = turn % active.len();
            turn += 1;
            let claim = active[slot];

            // The first connection runs far faster than the rest.
            let step = if slot == 0 { 4 << 20 } else { 100_003 };
            advance(&mut map, claim.index, step);

            if map.unfetched(claim.index) == 0 {
                map.release(claim.index);
                match map.claim() {
                    Some(next) => active[slot] = next,
                    None => {
                        active.remove(slot);
                    }
                }
            }
        }

        assert!(map.is_complete());
        assert_eq!(map.total_written(), size);

        let mut parts = ranges(&map);
        parts.sort_unstable();
        assert_eq!(parts[0].0, 0);
        assert_eq!(parts.last().unwrap().1, size);
        for pair in parts.windows(2) {
            assert_eq!(pair[0].1, pair[1].0, "no gaps or overlaps");
        }
        assert!(
            parts.len() > 4,
            "the fast connection should have split work"
        );
    }
}
