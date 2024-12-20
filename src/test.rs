use core::{cell::Cell, ptr, sync::atomic::AtomicBool};

use super::*;

static ARENA: Arena<1000> = Arena::new();

#[test]
fn test_acquire() {
    let two = ARENA.acquire(2).unwrap();
    assert!(*two == 2);
}
#[test]
fn test_acquire_default() {
    let zero = ARENA.acquire_default::<usize>().unwrap();
    assert!(*zero == 0);
}
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
}

impl<'b, T> Init for CdllNode<'b, T> {
    type InitArg = T;
    fn init(me: &mut MaybeUninit<Self>, arg: T) {
        unsafe {
            me.write(CdllNode {
                data: arg,
                next: Cell::new(ptr::from_ref(me).cast::<Self>().as_ref().unwrap()),
                prev: Cell::new(ptr::from_ref(me).cast::<Self>().as_ref().unwrap()),
            });
        }
    }
}

#[test]
fn test_acquire_init() {
    let n = ARENA.acquire_init::<CdllNode<usize>>(1).unwrap();
    assert!(n.data == 1);
    assert!(n.next.get().data == 1);
}
#[test]
fn test_acquire_init_default() {
    let n = ARENA.acquire_init_default::<CdllNode<usize>>().unwrap();
    assert!(n.data == 0);
    assert!(n.next.get().data == 0);
}

#[test]
fn test_interlinking_reference() {
    let n = ARENA.acquire_init_default::<CdllNode<usize>>().unwrap();
    n.insert(ARENA.acquire_init::<CdllNode<usize>>(1).unwrap());

    assert!(n.data == 0);
    assert!(n.next.get().data == 1);
    assert!(n.next.get().next.get().data == 0);
    assert!(n.next.get().next.get().prev.get().data == 1);
}

static TEST_DROPPED: AtomicBool = AtomicBool::new(false);
#[derive(Default)]
struct Test {}

impl Test {
    fn hi(&self) -> &str {
        "hi"
    }
}

impl Drop for Test {
    fn drop(&mut self) {
        TEST_DROPPED.store(true, Ordering::Release);
    }
}

#[test]
fn test_zero_size() {
    let z0 = ARENA.acquire_default::<Test>().unwrap();
    let z = ARENA.acquire_default::<Test>().unwrap();
    assert!(z.hi() == "hi");
    assert!(z0.hi() == "hi");
}

#[test]
fn test_drop() {
    let arena = Arena::<1>::new();
    let _z = arena.acquire_default::<Test>().unwrap();
    drop(arena);
    assert!(TEST_DROPPED.load(Ordering::Acquire));
}
