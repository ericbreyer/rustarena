#[cfg(test)]

use std::{cell::Cell, mem::MaybeUninit, ptr, sync::Arc};

use playground::{Arena,  Init};

struct CdllNode<'b, T> {
    data: T,
    next: Cell<&'b Self>,
    prev: Cell<&'b Self>,
}

impl<'b, T> CdllNode<'b, T> {
    fn insert(&'b self, other: &'b CdllNode<'b, T>) {
        self.next.get().prev.set(other);
        other.next.set(self.next.get());
        self.next.set(other);
        other.prev.set(self);
    }

    fn iter(&'b self) -> CdllIter<'b, T> {
        CdllIter::new(self)
    }
}

struct CdllIter<'b, T> {
    next: &'b CdllNode<'b, T>,
    first: &'b CdllNode<'b, T>,
    begun: bool,
}

impl<'b, T> CdllIter<'b, T> {
    fn new(start: &'b CdllNode<'b, T>) -> CdllIter<'b, T> {
        CdllIter {
            next: start,
            first: start,
            begun: false,
        }
    }
}
impl<'b, T> Iterator for CdllIter<'b, T> {
    type Item = &'b CdllNode<'b, T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.begun && ptr::from_ref(self.next) == ptr::from_ref(self.first) {
            return None;
        };

        let prev = self.next;
        self.next = prev.next.get();

        self.begun = true;

        Some(prev)
    }
}

impl<'b, T> Init for CdllNode<'b, T> {
    type InitArg = T;
    fn init(me: &mut MaybeUninit<Self>, arg: T) {
        unsafe {
            me.write(CdllNode {
                data: arg,
                next: Cell::new(
                    ptr::from_ref(me).cast::<Self>().as_ref().unwrap(),
                ),
                prev: Cell::new(
                    ptr::from_ref(me).cast::<Self>().as_ref().unwrap(),
                ),
            });
        }
    }
}

#[test]
fn test_main() {
    let arena: Arc<Arena<40000>> = Arena::new().into();
    let mut v = Vec::new();
    for _ in 0..10 {
        let arena = Arc::clone(&arena);
        v.push(std::thread::spawn(move || {
            let node: &CdllNode<usize> = arena.aquire_init(100).unwrap();

            for i in 0..100 {
                node.insert(arena.aquire_init(i).unwrap());
            }

            for (i, n) in node.iter().enumerate() {
                assert!(100 - i == n.data);
            }

            node.iter().last().unwrap().data
        }));
    }
    for h in v {
        let r = h.join();
        assert!(r.unwrap() == 0);
    }
}
