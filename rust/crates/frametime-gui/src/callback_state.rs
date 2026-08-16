use std::cell::{Cell, RefCell};

/// Heap-owned state retained by a window while callbacks are active.
///
/// A `RefCell` rejects synchronous reentry while a callback holds mutable app
/// access. Destruction is recorded separately so the allocation can remain
/// alive until the outermost callback has unwound.
pub(crate) struct CallbackState<T> {
    app: RefCell<T>,
    callback_depth: Cell<usize>,
    destroying: Cell<bool>,
}

impl<T> CallbackState<T> {
    pub(crate) fn new(app: T) -> Self {
        Self {
            app: RefCell::new(app),
            callback_depth: Cell::new(0),
            destroying: Cell::new(false),
        }
    }

    pub(crate) fn enter(&self) -> bool {
        if self.destroying.get() {
            return false;
        }
        let Some(depth) = self.callback_depth.get().checked_add(1) else {
            return false;
        };
        self.callback_depth.set(depth);
        true
    }

    pub(crate) fn leave(&self) -> bool {
        let depth = self
            .callback_depth
            .get()
            .checked_sub(1)
            .expect("callback depth is balanced");
        self.callback_depth.set(depth);
        depth == 0 && self.destroying.get()
    }

    pub(crate) fn begin_destroy(&self) -> bool {
        !self.destroying.replace(true)
    }

    pub(crate) fn with_app<R>(&self, callback: impl FnOnce(&mut T) -> R) -> Option<R> {
        if self.destroying.get() {
            return None;
        }
        let mut app = self.app.try_borrow_mut().ok()?;
        Some(callback(&mut app))
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::CallbackState;

    #[test]
    fn synchronous_reentry_cannot_alias_mutable_app_access() {
        let state = CallbackState::new(0_u8);
        assert!(state.enter());

        assert_eq!(
            state.with_app(|app| {
                *app = 1;
                assert!(state.enter());
                assert!(state.with_app(|nested| *nested = 2).is_none());
                assert!(!state.leave());
            }),
            Some(())
        );

        assert!(!state.leave());
        assert_eq!(state.callback_depth.get(), 0);
        assert_eq!(*state.app.borrow(), 1);
    }

    #[test]
    fn nested_destroy_defers_and_releases_once_after_outer_callback() {
        struct DropProbe(Rc<Cell<usize>>);

        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let drops = Rc::new(Cell::new(0));
        let state = Box::new(CallbackState::new(DropProbe(Rc::clone(&drops))));

        assert!(state.enter());
        assert!(state.enter());
        assert!(state.begin_destroy());
        assert!(!state.begin_destroy());
        assert!(state.with_app(|_| ()).is_none());
        assert!(!state.leave());
        assert_eq!(drops.get(), 0);
        assert!(state.leave());
        drop(state);

        assert_eq!(drops.get(), 1);
    }
}
