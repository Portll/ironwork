//! LINAGE's logical page at run time: where the printer is in the page body, and how far each
//! WRITE moves the paper past the footing area and the margins
//! ([`numeric::assumptions::LINAGE_PAGE_MOVEMENT`]).

/// One logical page, in lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub body: u64,
    /// The first line of the footing area. Without FOOTING only a page overflow raises the
    /// end-of-page condition (Language Reference SC27-8713-03, p. 475).
    pub footing: Option<u64>,
    pub top: u64,
    pub bottom: u64,
}

impl Geometry {
    /// The page the clause's values give, or why they give none: the page body is at least a line
    /// and the footing starts within it (Language Reference SC27-8713-03, p. 189).
    pub fn new(body: i64, footing: Option<i64>, top: i64, bottom: i64) -> Result<Self, String> {
        if body < 1 {
            return Err(format!("LINAGE gives a page body of {body} lines, and it needs at least 1"));
        }
        if let Some(f) = footing.filter(|f| !(1..=body).contains(f)) {
            return Err(format!("LINAGE puts the footing at line {f}, outside the page body of {body} lines"));
        }
        if top < 0 || bottom < 0 {
            return Err(format!("LINAGE gives margins of {top} and {bottom} lines"));
        }
        Ok(Self { body: body as u64, footing: footing.map(|f| f as u64), top: top as u64, bottom: bottom as u64 })
    }
}

/// Where the printer is on a LINAGE file's current page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    pub geometry: Geometry,
    /// LINAGE-COUNTER: the line of the page body the printer is at.
    pub counter: u64,
    /// Lines the paper has yet to move to reach that line: the first page's top margin, until the
    /// first WRITE.
    owed: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Lines(u64),
    Page,
}

/// What one WRITE does: the lines the paper moves before the record's line and after it, and
/// whether the end-of-page condition holds once it is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub ahead: u64,
    pub behind: u64,
    pub end_of_page: bool,
}

impl Page {
    pub fn opened(geometry: Geometry) -> Self {
        Self { geometry, counter: 1, owed: geometry.top }
    }

    /// A WRITE BEFORE (`before`) or AFTER ADVANCING `motion`. A WRITE that would pass the page
    /// body, or that advances a page, goes to the first line of the next page, whose geometry
    /// `next` gives (Language Reference SC27-8713-03, pp. 474-475).
    pub fn write<E>(&mut self, before: bool, motion: Motion, next: impl FnOnce() -> Result<Geometry, E>) -> Result<Step, E> {
        let current = self.geometry;
        let (moved, overflow) = match motion {
            Motion::Lines(n) if self.counter.saturating_add(n) <= current.body => {
                self.counter += n;
                (n, false)
            }
            _ => {
                let following = next()?;
                let rest = current.body.saturating_sub(self.counter).saturating_add(current.bottom);
                self.geometry = following;
                self.counter = 1;
                (rest.saturating_add(following.top).saturating_add(1), motion != Motion::Page)
            }
        };
        let end_of_page = overflow || self.geometry.footing.is_some_and(|f| self.counter >= f);
        let owed = std::mem::take(&mut self.owed);
        Ok(if before { Step { ahead: owed, behind: moved, end_of_page } } else { Step { ahead: owed.saturating_add(moved), behind: 0, end_of_page } })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(body: i64, footing: Option<i64>, top: i64, bottom: i64) -> Page {
        Page::opened(Geometry::new(body, footing, top, bottom).unwrap())
    }

    fn step(p: &mut Page, before: bool, motion: Motion, next: Geometry) -> (u64, u64, bool, u64) {
        let s = p.write(before, motion, || Ok::<_, ()>(next)).unwrap();
        (s.ahead, s.behind, s.end_of_page, p.counter)
    }

    #[test]
    fn the_first_write_moves_past_the_top_margin_and_counts_from_line_one() {
        let g = Geometry::new(5, Some(4), 2, 3).unwrap();
        let mut p = Page::opened(g);
        assert_eq!(p.counter, 1);
        assert_eq!(step(&mut p, false, Motion::Lines(1), g), (3, 0, false, 2));
        assert_eq!(step(&mut p, false, Motion::Lines(1), g), (1, 0, false, 3));
        assert_eq!(step(&mut p, false, Motion::Lines(1), g), (1, 0, true, 4));
        assert_eq!(step(&mut p, false, Motion::Lines(0), g), (0, 0, true, 4));
        assert_eq!(step(&mut p, false, Motion::Lines(2), g), (1 + 3 + 2 + 1, 0, true, 1));
    }

    #[test]
    fn before_advancing_prints_then_moves_and_overflow_leaves_the_printer_on_the_next_page() {
        let g = Geometry::new(3, Some(3), 1, 1).unwrap();
        let mut p = Page::opened(g);
        assert_eq!(step(&mut p, true, Motion::Lines(1), g), (1, 1, false, 2));
        assert_eq!(step(&mut p, true, Motion::Lines(1), g), (0, 1, true, 3));
        assert_eq!(step(&mut p, true, Motion::Lines(1), g), (0, 1 + 1 + 1, true, 1));
    }

    #[test]
    fn without_footing_only_an_overflow_is_the_end_of_the_page() {
        let g = Geometry::new(2, None, 0, 0).unwrap();
        let mut p = Page::opened(g);
        assert_eq!(step(&mut p, false, Motion::Lines(1), g), (1, 0, false, 2));
        assert_eq!(step(&mut p, false, Motion::Lines(0), g), (0, 0, false, 2));
        assert_eq!(step(&mut p, false, Motion::Lines(1), g), (1, 0, true, 1));
        assert_eq!(step(&mut p, false, Motion::Page, g), (2, 0, false, 1));
    }

    #[test]
    fn advancing_page_takes_the_next_geometry_and_raises_end_of_page_only_at_a_first_line_footing() {
        let mut p = page(10, Some(8), 0, 0);
        let next = Geometry::new(4, Some(2), 3, 0).unwrap();
        assert_eq!(step(&mut p, false, Motion::Lines(5), next), (5, 0, false, 6));
        assert_eq!(step(&mut p, false, Motion::Page, next), (4 + 3 + 1, 0, false, 1));
        assert_eq!(p.geometry, next);
        let at_one = Geometry::new(4, Some(1), 0, 0).unwrap();
        assert_eq!(step(&mut p, true, Motion::Page, at_one), (0, 3 + 1, true, 1));
    }

    #[test]
    fn a_page_the_values_cannot_make_is_refused() {
        assert!(Geometry::new(0, None, 0, 0).is_err());
        assert!(Geometry::new(5, Some(6), 0, 0).is_err());
        assert!(Geometry::new(5, Some(0), 0, 0).is_err());
        assert!(Geometry::new(5, None, -1, 0).is_err());
        assert_eq!(Geometry::new(5, None, 0, 0).unwrap().footing, None);
    }
}
