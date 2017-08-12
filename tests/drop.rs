//! Destructors are called.

extern crate cell_gc;
#[macro_use]
extern crate cell_gc_derive;

use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};

static DROP_COUNT: AtomicUsize = ATOMIC_USIZE_INIT;

#[derive(IntoHeap)]
struct Dropper<'h> {
    which: String,
    ignore: SomethingWithLifetime<'h>,
}

#[derive(IntoHeap)]
enum SomethingWithLifetime<'h> {
    Another(DropperRef<'h>),
    Nothing,
}

use SomethingWithLifetime::Nothing;

impl<'h> Drop for DropperStorage {
    fn drop(&mut self) {
        let addr = self as *mut _;
        println!("DropperStorage::drop {} @ {:p}", self.which, addr);
        DROP_COUNT.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn drop() {
    cell_gc::with_heap(|hs| {
        let mut r = hs.alloc(Dropper {
            which: "0".into(),
            ignore: Nothing,
        });
        for i in 1..7 {
            r = hs.alloc(Dropper {
                which: i.to_string(),
                ignore: Nothing,
            });
        }

        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0);
        hs.force_gc();
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 6);
        std::mem::drop(r);
        hs.force_gc();
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 7);
    });
}
