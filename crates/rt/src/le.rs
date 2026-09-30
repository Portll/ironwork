//! What a run unit keeps for Language Environment's callable services.

/// What a run keeps for the services: heap storage, and which output DDs it has started.
#[derive(Debug, Default)]
pub struct State {
    /// Each CEEGTST block: where it starts, its length, and whether CEEFRST has freed it.
    pub heap: Vec<(usize, usize, bool)>,
    pub written: Vec<String>,
}

impl State {
    /// The end of the highest heap block: run-unit storage below it is not released.
    pub fn heap_end(&self) -> usize {
        self.heap.iter().map(|&(at, len, _)| at + len).max().unwrap_or(0)
    }
}
