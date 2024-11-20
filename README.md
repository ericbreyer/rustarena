# Rust arena allocator

## Description

A small, thread-safe, no-std, arena allocator with a static backing store and ability to allocate arbitrary types.

## Examples

### Simple Types

```rust
use arena_alloc::Arena;
static ARENA: Arena<1000> = Arena::new();

fn main() {
    let two = ARENA.acquire(2).unwrap();
    let zero = ARENA.acquire_default::<usize>().unwrap();

    assert_eq!(*two, 2);
    assert_eq!(*zero, 0);
}
```

### Self Referential Types

```rust
use arena_alloc::{Arena, Init};
use std::cell::Cell;
use std::ptr;
use std::mem::MaybeUninit;

static ARENA: Arena<1000> = Arena::new();

struct CllNode<'b, T> {
    data: T,
    next: Cell<&'b Self>,
}

impl<'b, T> CllNode<'b, T> {
    fn cons(&'b self, other: &'b CllNode<'b, T>) {
        self.next.set(other);
    }
}

impl<'b, T> Init for CllNode<'b, T> {
    type InitArg = T;
    fn init(me: &mut MaybeUninit<Self>, arg: T) {
        unsafe {
            me.write(CllNode {
                data: arg,
                next: Cell::new(ptr::from_ref(me).cast::<Self>().as_ref().unwrap()),
            });
        }
    }
}

fn main() {
    let n0 = ARENA.acquire_init_default::<CllNode<usize>>().unwrap();
    let n1 = ARENA.acquire_init::<CllNode<_>>(1).unwrap();
    let n2 = ARENA.acquire_init::<CllNode<_>>(2).unwrap();

    n0.cons(n1);
    n1.cons(n2);
    n2.cons(n0);
    assert_eq!(n0.next.get().data, 1);
    assert_eq!(n1.next.get().data, 2);
    assert_eq!(n2.next.get().data, 0);
}
```
