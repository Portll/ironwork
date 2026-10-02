//! Out-of-line PERFORM as Enterprise COBOL runs it: the end of a range's last paragraph holds a
//! return point while the PERFORM is active, and control that passes that end by any path, fall
//! through or GO TO, returns to the PERFORM (Language Reference SC27-8713-03, pp. 79 and 419;
//! assumption PERFORM_RETURN_POINTS in numeric::assumptions).

use super::*;

/// A return point armed at the end of a paragraph.
#[derive(Clone, Copy, Debug)]
pub(super) struct Return {
    frame: u64,
    /// For a PERFORM that runs once and is not inside another statement: its address, and the
    /// paragraph and statement control resumes at when it returns after control left its range.
    resume: Option<(usize, (usize, usize))>,
}

/// Each paragraph's return point, and the PERFORMs running now, outermost first.
#[derive(Default)]
pub(super) struct Returns {
    armed: Vec<Option<Return>>,
    /// What each PERFORM statement that control left displaced from its paragraph's return point.
    saved: HashMap<usize, Option<Return>>,
    active: Vec<u64>,
    frames: u64,
    /// The paragraph control is in.
    pub(super) running: usize,
}

impl Returns {
    pub(super) fn new(paragraphs: usize) -> Self {
        Self { armed: vec![None; paragraphs], ..Self::default() }
    }
}

impl<'p> Machine<'p, '_, '_> {
    /// Runs the procedure from its start, or from paragraph and statement `at`.
    pub(super) fn run_from(&mut self, at: Option<(usize, usize)>) -> R<Ending> {
        let at = at.unwrap_or((self.program.report_writer.procedure_start, 0));
        self.run_at(at, declaratives::Arrival::Start)
    }

    /// Runs the procedure from paragraph `p` as a GO TO at `from` reaches it, with the return
    /// points of the PERFORMs control left still armed.
    pub(super) fn go_to(&mut self, p: usize, from: Pos) -> R<Ending> {
        self.uses.line = from;
        self.run_at((p, 0), declaratives::Arrival::GoTo)
    }

    fn run_at(&mut self, (start, skip): (usize, usize), arrival: declaratives::Arrival) -> R<Ending> {
        if self.program.paragraphs.len() <= start {
            return Ok(Ending::EndOfProgram);
        }
        self.segment = self.program.paragraphs[start].priority;
        self.uses.arrival = arrival;
        match self.run_region((start, skip), (0, self.program.paragraphs.len() - 1), 0)? {
            Flow::End(e) => Ok(e),
            _ => Ok(Ending::EndOfProgram),
        }
    }

    /// Paragraphs `from` through `to` as a procedure run under another statement: a GO TO out of
    /// the range leaves it.
    pub(super) fn run_paragraphs(&mut self, from: usize, to: usize) -> R<Flow> {
        self.perform_range(from, to, None, None)
    }

    /// Paragraphs `from` through `to` with the end of `to` armed to return here while they run. A
    /// GO TO to a paragraph in `region` stays in this call; by default that is the range, or
    /// from `from` on when `to` comes before it. `statement` is the PERFORM's address and where
    /// control resumes after it. An abend leaves the range as a GO TO out of it does, which a
    /// HANDLE ABEND LABEL continues (C236).
    pub(super) fn perform_range(&mut self, from: usize, to: usize, region: Option<(usize, usize)>, statement: Option<(usize, (usize, usize))>) -> R<Flow> {
        let last = self.program.paragraphs.len() - 1;
        let region = region.unwrap_or(if from <= to { (from, to) } else { (from, last) });
        self.returns.frames += 1;
        let frame = self.returns.frames;
        let saved = self.returns.armed[to].replace(Return { frame, resume: statement });
        self.returns.active.push(frame);
        let flow = self.run_region((from, 0), region, frame);
        self.returns.active.pop();
        match flow {
            Ok(Flow::Next) => {
                self.returns.armed[to] = saved;
                Ok(Flow::Next)
            }
            Ok(Flow::Return(f)) if f == frame => {
                self.returns.armed[to] = saved;
                Ok(Flow::Next)
            }
            left => {
                if let Some((address, _)) = statement {
                    self.returns.saved.insert(address, saved);
                }
                left
            }
        }
    }

    /// Where control resumes after statement `s` when it is one of the running paragraph's own.
    pub(super) fn after(&self, s: &Stmt) -> Option<(usize, (usize, usize))> {
        let i = self.returns.running;
        let stmts = &self.program.paragraphs.get(i)?.statements;
        let address = std::ptr::from_ref(s) as usize;
        let k = address.checked_sub(stmts.as_ptr() as usize)? / std::mem::size_of::<Stmt>();
        (k < stmts.len() && std::ptr::eq(&stmts[k], s)).then_some((address, (i, k + 1)))
    }

    /// Runs from paragraph `i`, statement `skip`, for PERFORM `frame`, following GO TOs to
    /// paragraphs in `region`, until control passes the end of a paragraph armed to return to this
    /// PERFORM or to one outside it, or leaves the region. An altered paragraph goes where its
    /// ALTER said.
    fn run_region(&mut self, (mut i, mut skip): (usize, usize), (lo, hi): (usize, usize), frame: u64) -> R<Flow> {
        let program = self.program;
        let (segment, running) = (self.segment, self.returns.running);
        let mut arrival = std::mem::take(&mut self.uses.arrival);
        let within = |t: usize| (lo..=hi).contains(&t);
        let flow = loop {
            if i > hi {
                break if i < program.paragraphs.len() { Flow::GoTo(i) } else { Flow::End(Ending::EndOfProgram) };
            }
            self.returns.running = i;
            self.enter_segment(program.paragraphs[i].priority);
            if skip == 0 {
                self.unit.notify(rt::unit::Event::Paragraph { program: &program.id, name: &program.paragraphs[i].name, index: i });
            }
            if !self.declaratives.triggers.is_empty() {
                if skip == 0
                    && let Some(flow) = self.debug_before(i, arrival)?
                {
                    break flow;
                }
                if program.paragraphs[i].is_section {
                    self.uses.line = program.paragraphs[i].pos;
                }
                arrival = declaratives::Arrival::FallThrough;
            }
            let altered = self.unit.programs[self.me].altered.get(i).copied().flatten();
            let statements = &program.paragraphs[i].statements;
            let flow = match altered {
                Some(t) => Flow::GoTo(t),
                None => self.run_sentences(&statements[skip.min(statements.len())..])?,
            };
            skip = 0;
            let end = match flow {
                Flow::Next | Flow::ExitParagraph | Flow::ExitPerform | Flow::ExitPerformCycle => i,
                Flow::ExitSection => crate::section_end(program, i),
                Flow::GoTo(t) if within(t) => {
                    i = t;
                    arrival = declaratives::Arrival::GoTo;
                    continue;
                }
                Flow::Resume(p, s) if within(p) => {
                    (i, skip) = (p, s);
                    self.segment = program.paragraphs[p].priority;
                    continue;
                }
                other => break other,
            };
            match self.returns.armed[end] {
                None => i = end + 1,
                Some(r) if r.frame == frame => break Flow::Next,
                Some(r) if self.returns.active.binary_search(&r.frame).is_ok() => break Flow::Return(r.frame),
                Some(Return { resume: Some((address, (p, s))), .. }) => {
                    self.returns.armed[end] = self.returns.saved.get(&address).copied().flatten();
                    if !within(p) {
                        break Flow::Resume(p, s);
                    }
                    (i, skip) = (p, s);
                    self.segment = program.paragraphs[p].priority;
                }
                Some(_) => {
                    return Err(Abend::ironwork(
                        format!(
                            "control passed the end of {}, which is armed to return to a PERFORM that control left by GO TO; ironwork returns there only to a PERFORM that runs once and is not inside another statement",
                            program.paragraphs[end].name
                        ),
                        program.paragraphs[end].pos,
                    ));
                }
            }
        };
        if matches!(flow, Flow::Next | Flow::Return(_)) {
            self.segment = segment;
        }
        self.returns.running = running;
        Ok(flow)
    }
}
