//! Limits on the work of one render, checked while it runs.
//!
//! [`run`] sets a [`Budget`] for the current thread. The stages check it as they go: styling
//! and layout check the deadline, documents are checked as they are parsed, and pagination
//! checks the page count. A check that fails unwinds to [`run`] with [`resume_unwind`], which
//! skips the panic hook, and [`run`] returns the limit that was exceeded. Outside [`run`]
//! every check passes.
//!
//! Unwinding leaves the thread's caches and scoped state as they were mid-render, so the
//! thread should not render again afterwards.

use std::cell::{Cell, RefCell};
use std::panic::{self, AssertUnwindSafe, resume_unwind};
use std::time::Instant;

use crate::Document;

/// The limits of one render.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    pub deadline: Option<Instant>,
    /// DOM nodes, counted across the document and its `iframe` documents.
    pub max_nodes: usize,
    /// How deeply elements nest in any one document.
    pub max_depth: usize,
    pub max_pages: usize,
}

/// The limit a render exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exceeded {
    Deadline,
    Nodes,
    Depth,
    Pages,
}

struct Running {
    budget: Budget,
    nodes: Cell<usize>,
}

thread_local! {
    static RUNNING: RefCell<Option<Running>> = const { RefCell::new(None) };
}

/// Runs `f` within `budget`, or returns the limit it exceeded.
pub fn run<T>(budget: Budget, f: impl FnOnce() -> T) -> Result<T, Exceeded> {
    let previous = RUNNING.with(|running| {
        running.replace(Some(Running {
            budget,
            nodes: Cell::new(0),
        }))
    });
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    RUNNING.with(|running| *running.borrow_mut() = previous);
    result.map_err(|payload| match payload.downcast::<Exceeded>() {
        Ok(exceeded) => *exceeded,
        Err(payload) => resume_unwind(payload),
    })
}

fn exceed(limit: Exceeded) -> ! {
    resume_unwind(Box::new(limit))
}

fn with_budget(f: impl FnOnce(&Running)) {
    RUNNING.with(|running| {
        if let Some(running) = &*running.borrow() {
            f(running);
        }
    });
}

/// Stops the render if its deadline has passed.
pub fn check_deadline() {
    let mut passed = false;
    with_budget(|running| {
        passed = running.budget.deadline.is_some_and(|d| Instant::now() >= d);
    });
    if passed {
        exceed(Exceeded::Deadline);
    }
}

/// Stops the render if `document` takes the node count over the limit, or nests too deeply.
pub fn check_document(document: &Document) {
    let mut exceeded = None;
    with_budget(|running| {
        let nodes = running.nodes.get() + document.node_count();
        running.nodes.set(nodes);
        if nodes > running.budget.max_nodes {
            exceeded = Some(Exceeded::Nodes);
        } else if document.element_depth() > running.budget.max_depth {
            exceeded = Some(Exceeded::Depth);
        }
    });
    if let Some(limit) = exceeded {
        exceed(limit);
    }
}

/// Stops the render if it has laid out more than the page limit.
pub fn check_pages(pages: usize) {
    let mut over = false;
    with_budget(|running| over = pages > running.budget.max_pages);
    if over {
        exceed(Exceeded::Pages);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const UNLIMITED: Budget = Budget {
        deadline: None,
        max_nodes: usize::MAX,
        max_depth: usize::MAX,
        max_pages: usize::MAX,
    };

    #[test]
    fn checks_pass_outside_a_budget() {
        check_deadline();
        check_pages(usize::MAX);
        check_document(&Document::parse(&"<div>".repeat(100)));
    }

    #[test]
    fn exceeded_limits_end_the_run() {
        let budget = Budget {
            max_pages: 3,
            ..UNLIMITED
        };
        assert_eq!(run(budget, || check_pages(3)), Ok(()));
        assert_eq!(run(budget, || check_pages(4)), Err(Exceeded::Pages));
        let past = Budget {
            deadline: Some(Instant::now() - Duration::from_millis(1)),
            ..UNLIMITED
        };
        assert_eq!(run(past, check_deadline), Err(Exceeded::Deadline));
        // The budget ends with the run.
        check_pages(4);
    }

    #[test]
    fn nodes_count_across_documents() {
        let document = Document::parse("<p>a</p><p>b</p>");
        let budget = Budget {
            max_nodes: document.node_count() * 2,
            ..UNLIMITED
        };
        assert_eq!(
            run(budget, || {
                check_document(&document);
                check_document(&document);
            }),
            Ok(())
        );
        assert_eq!(
            run(budget, || {
                check_document(&document);
                check_document(&document);
                check_document(&document);
            }),
            Err(Exceeded::Nodes)
        );
    }

    #[test]
    fn depth_counts_elements() {
        // `html` and `body` are two of the levels.
        let document = Document::parse(&format!("{}text", "<div>".repeat(8)));
        assert_eq!(document.element_depth(), 10);
        let budget = |max_depth| Budget {
            max_depth,
            ..UNLIMITED
        };
        assert_eq!(run(budget(10), || check_document(&document)), Ok(()));
        assert_eq!(
            run(budget(9), || check_document(&document)),
            Err(Exceeded::Depth)
        );
    }

    #[test]
    fn other_panics_pass_through() {
        let result = panic::catch_unwind(|| {
            run(UNLIMITED, || {
                resume_unwind(Box::new("bug"));
            })
        });
        assert!(result.is_err());
    }
}
