//! Which bytes of run-unit memory may hold input, for `--trace-input` (docs/evidence.md §1.3). Input
//! sets the bytes it lands in; each write gives its bytes whether the running statement has read a
//! set byte; a sink asks the same. It over-approximates and never under-approximates, so a sink it
//! clears holds no input byte, unless the run did something it does not follow.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Taint {
    words: Vec<u64>,
    pending: bool,
    unfollowed: Option<&'static str>,
    writing: bool,
}

impl Taint {
    /// Whether any byte of the range may hold input.
    pub fn any(&self, offset: usize, len: usize) -> bool {
        words(offset, len, self.words.len()).any(|(w, mask)| self.words[w] & mask != 0)
    }

    pub fn set(&mut self, offset: usize, len: usize, input: bool) {
        if input && (offset + len).div_ceil(64) > self.words.len() {
            self.words.resize((offset + len).div_ceil(64), 0);
        }
        for (w, mask) in words(offset, len, self.words.len()) {
            if input { self.words[w] |= mask } else { self.words[w] &= !mask }
        }
    }

    /// A read of the range by the running statement, unless it is locating a receiver it only
    /// writes.
    pub fn read(&mut self, offset: usize, len: usize) {
        if !self.pending && !self.writing && self.any(offset, len) {
            self.pending = true;
        }
    }

    /// Whether the running statement has read a byte that may hold input.
    pub const fn pending(&self) -> bool {
        self.pending
    }

    pub const fn start_statement(&mut self) {
        self.pending = false;
    }

    /// Back in a statement after statements ran inside it, as a user-defined function's do: what
    /// it had read before still counts, with what the inner ones left.
    pub const fn resume_statement(&mut self, read_before: bool) {
        self.pending |= read_before;
    }

    /// Whether locates are of a receiver the statement only writes: MOVE's, SET's, ACCEPT's and
    /// INITIALIZE's, whose old bytes reach nothing. Subscripts evaluated meanwhile read nothing
    /// either: they choose where the bytes go, not what they are. The previous setting.
    pub const fn writing(&mut self, on: bool) -> bool {
        let was = self.writing;
        self.writing = on;
        was
    }

    /// Memory cut back to `len` bytes: what was beyond holds nothing.
    pub fn truncate(&mut self, len: usize) {
        self.words.truncate(len.div_ceil(64));
        if let Some(last) = self.words.last_mut()
            && !len.is_multiple_of(64)
        {
            *last &= (1u64 << (len % 64)) - 1;
        }
    }

    /// The run did `what`, which taint does not follow: from here no sink is cleared.
    pub fn unfollowed(&mut self, what: &'static str) {
        self.unfollowed.get_or_insert(what);
    }

    /// The first operation of the run taint did not follow.
    pub const fn not_followed(&self) -> Option<&'static str> {
        self.unfollowed
    }

    /// At a sink: Some(true) when an input byte may be in its operand, Some(false) when none is,
    /// None when the run did something taint does not follow and nothing read says so.
    pub const fn at_sink(&self) -> Option<bool> {
        match (self.pending, self.unfollowed) {
            (true, _) => Some(true),
            (false, None) => Some(false),
            (false, Some(_)) => None,
        }
    }

    /// The set bytes, trailing zero words left out, for comparing two runs.
    pub fn bits(&self) -> Vec<u64> {
        let mut words = self.words.clone();
        while words.last() == Some(&0) {
            words.pop();
        }
        words
    }
}

/// The words of `count` a byte range covers, each with the bits of the range in it.
fn words(offset: usize, len: usize, count: usize) -> impl Iterator<Item = (usize, u64)> {
    let end = (offset + len).min(count * 64);
    let (first, last) = (offset / 64, end.saturating_sub(1) / 64);
    (first..=last).filter(move |_| offset < end).map(move |w| {
        let low = if w == first { offset % 64 } else { 0 };
        let high = if w == last { (end - 1) % 64 } else { 63 };
        (w, (u64::MAX >> (63 - high)) & (u64::MAX << low))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_carries_what_its_statement_read_and_a_new_statement_starts_clean() {
        let mut t = Taint::default();
        t.set(10, 4, true);
        t.read(0, 8);
        assert_eq!(t.at_sink(), Some(false));
        t.read(12, 1);
        assert!(t.pending());
        t.set(100, 3, t.pending());
        assert!(t.any(101, 1) && !t.any(103, 60));
        t.start_statement();
        t.set(10, 4, t.pending());
        assert!(!t.any(0, 100) && t.any(100, 3));
        assert_eq!(t.bits(), vec![0, 7u64 << 36]);
    }

    #[test]
    fn truncating_clears_what_was_beyond_and_an_unfollowed_run_clears_no_sink() {
        let mut t = Taint::default();
        t.set(60, 10, true);
        t.truncate(62);
        assert!(t.any(60, 2) && !t.any(62, 8));
        t.set(62, 8, false);
        t.unfollowed("SORT");
        t.unfollowed("XML PARSE");
        assert_eq!((t.at_sink(), t.not_followed()), (None, Some("SORT")));
        t.read(60, 1);
        assert_eq!(t.at_sink(), Some(true));
    }

    #[test]
    fn a_range_across_words_is_set_and_found_at_its_edges_only() {
        let mut t = Taint::default();
        t.set(63, 66, true);
        assert!(!t.any(0, 63) && t.any(62, 2) && t.any(128, 1) && !t.any(129, 1000) && !t.any(5000, 0));
        assert_eq!(t.bits(), vec![1 << 63, u64::MAX, 1]);
        t.set(64, 64, false);
        assert_eq!(t.bits(), vec![1 << 63, 0, 1]);
    }
}
