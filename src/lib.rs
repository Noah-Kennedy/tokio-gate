#[cfg(all(feature = "parking-lot", not(loom)))]
use parking_lot::{Mutex, MutexGuard};

#[cfg(not(any(feature = "parking-lot", loom)))]
use std::sync::{Mutex, MutexGuard};

#[cfg(loom)]
use loom::sync::{Mutex, MutexGuard};

use tokio::sync::futures::Notified;
use tokio::sync::Notify;

pub struct Gate {
    notify: Notify,
    check: Mutex<bool>,
}

impl Gate {
    pub fn new() -> Self {
        Self {
            notify: Notify::new(),
            check: Mutex::new(false),
        }
    }

    #[cfg(all(feature = "parking-lot", not(loom)))]
    pub const fn const_new() -> Self {
        Self {
            notify: Notify::const_new(),
            check: Mutex::new(false),
        }
    }

    pub async fn wait(&self) {
        if let Some(fut) = self.get_inner_stuff() {
            fut.await
        }
    }

    pub fn open_gate(&self) {
        let mut guard = lock_mutex(&self.check);

        *guard = true;
        self.notify.notify_waiters();
    }

    /// Get a notified if it isnt finished yet
    ///
    /// note: need an inner function because rustc isnt too smart about holding things across an await boundary
    fn get_inner_stuff(&self) -> Option<Notified<'_>> {
        // acquire guard first to prevent a race
        let guard = lock_mutex(&self.check);

        if !*guard {
            // if not finished
            Some(self.notify.notified())
        } else {
            // if finished
            None
        }
    }
}

#[cfg(all(feature = "parking-lot", not(loom)))]
fn lock_mutex(mtx: &Mutex<bool>) -> MutexGuard<'_, bool> {
    mtx.lock()
}

#[cfg(not(feature = "parking-lot"))]
fn lock_mutex(mtx: &Mutex<bool>) -> MutexGuard<'_, bool> {
    mtx.lock().unwrap()
}

#[cfg(all(loom, test))]
mod loom_tests {
    use super::*;
    use loom::future;
    use loom::sync::Arc;
    use loom::thread;
    use std::task::Poll;

    #[test]
    fn test_one_and_one() {
        loom::model(|| {
            let gate = Arc::new(Gate::new());

            let opener = gate.clone();

            thread::spawn(move || {
                opener.open_gate();
            });

            future::block_on(gate.wait());
        })
    }

    #[test]
    fn test_two_waits() {
        loom::model(|| {
            let gate = Arc::new(Gate::new());

            let opener = gate.clone();

            thread::spawn(move || {
                opener.open_gate();
            });

            future::block_on(gate.wait());

            future::block_on(async move {
                let new_fut = gate.wait();
                futures::pin_mut!(new_fut);

                assert_eq!(Poll::Ready(()), futures::poll!(new_fut));
            });
        })
    }

    #[test]
    fn test_many_readers() {
        loom::model(|| {
            let gate = Arc::new(Gate::new());

            let mut handles = Vec::new();

            for _ in 0..2 {
                let gate_clone = gate.clone();

                let handle = thread::spawn(move || {
                    future::block_on(gate_clone.wait());
                });

                handles.push(handle);
            }

            gate.open_gate();

            for h in handles {
                h.join().unwrap();
            }
        })
    }
}
