use core::{cell::UnsafeCell, mem::MaybeUninit, sync::atomic::AtomicUsize, usize};
pub use init::Init;

mod init;

type MemSlice<const SIZE: usize> = [u8; SIZE];

#[derive(Clone, Copy)]
struct Dropper<const SIZE: usize> {
    place: usize,
    func: fn(*mut MemSlice<SIZE>)
}

pub struct Arena<const SIZE: usize> {
    backing_store: UnsafeCell<MemSlice<SIZE>>,
    next_free_spot: AtomicUsize,
    drop_queue: UnsafeCell<[Option<Dropper<SIZE>>; SIZE]>,
    next_free_drop_spot: AtomicUsize,
}

unsafe impl<const SIZE: usize> Sync for Arena<SIZE> {}
unsafe impl<const SIZE: usize> Send for Arena<SIZE> {}

impl<const SIZE: usize> Default for Arena<SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, const SIZE: usize> Arena<SIZE> {
    #[must_use] pub const fn new() -> Self {
        Arena {
            backing_store: UnsafeCell::new([0; SIZE]),
            next_free_spot: AtomicUsize::new(0),
            drop_queue: UnsafeCell::new([None; SIZE]),
            next_free_drop_spot: AtomicUsize::new(0),
        }
    }

    fn get_ptr_place<T>(&'a self) -> Option<(usize, &mut MaybeUninit<T>)> {
        let place = self.next_free_spot.fetch_add(
            core::mem::size_of::<T>(),
            std::sync::atomic::Ordering::Release,
        );
        if place + core::mem::size_of::<T>() > SIZE {
            return None;
        }

        let ptr = unsafe { self.backing_store.get().byte_add(place) };
        let mptr = unsafe { ptr.cast::<MaybeUninit<T>>().as_mut().unwrap() };

        Some((place, mptr))
    }

    fn add_to_drop_queue<T>(&'a self, place: usize) {
        let dq = unsafe { self.drop_queue.get().as_mut() }.unwrap();
        dq[self
            .next_free_drop_spot
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)] =
            Some(Dropper{ place, func: |ptr: *mut MemSlice<SIZE>| unsafe {
                ptr.cast::<T>().drop_in_place();
            }});
    }

    pub fn aquire_init_default<T: Init>(&'a self) -> Option<&'a T>
    where
        T::InitArg: Default,
    {
        let (place, ptr) = self.get_ptr_place::<T>()?;

        T::init(ptr, T::InitArg::default());

        self.add_to_drop_queue::<T>(place);

        Some(unsafe { std::ptr::from_ref(ptr).cast::<T>().as_ref().unwrap_unchecked() })
    }
    pub fn aquire_init<T: Init>(&'a self, arg: T::InitArg) -> Option<&'a T> {
        let (place, ptr) = self.get_ptr_place::<T>()?;

        T::init(ptr, arg);

        self.add_to_drop_queue::<T>(place);

        Some(unsafe { std::ptr::from_ref(ptr).cast::<T>().as_ref().unwrap_unchecked() })
    }

    pub fn aquire_default<T: Default>(&'a self) -> Option<&'a T> {
        let (place, ptr) = self.get_ptr_place::<T>()?;

        ptr.write(T::default());

        self.add_to_drop_queue::<T>(place);

        Some(unsafe { std::ptr::from_ref(ptr).cast::<T>().as_ref().unwrap_unchecked() })
    }
    pub fn aquire<T>(&'a self, val: T) -> Option<&'a T> {
        let (place, ptr) = self.get_ptr_place::<T>()?;

        ptr.write(val);

        self.add_to_drop_queue::<T>(place);

        Some(unsafe { std::ptr::from_ref(ptr).cast::<T>().as_ref().unwrap_unchecked() })
    }
}

impl<const SIZE: usize> Drop for Arena<SIZE> {
    fn drop(&mut self) {
        for pair in self.drop_queue.get_mut() {
            let Some(Dropper{place, func}) = pair else {
                continue;
            };
            let ptr = unsafe { self.backing_store.get().byte_add(*place)};
            func(ptr);
        }
    }
}

#[cfg(test)]
mod test {
    use std::{cell::Cell, ptr, sync::atomic::AtomicBool};

    use super::*;

    static ARENA: Arena<1000> = Arena::new();

    #[test]
    fn test_aquire() {
        let two = ARENA.aquire(2).unwrap();
        assert!(*two == 2);
    }
    #[test]
    fn test_aquire_default() {
        let zero = ARENA.aquire_default::<usize>().unwrap();
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
    fn test_aquire_init() {
        let n = ARENA.aquire_init::<CdllNode<usize>>(1).unwrap();
        assert!(n.data == 1);
        assert!(n.next.get().data == 1);
    }
    #[test]
    fn test_aquire_init_default() {
        let n = ARENA.aquire_init_default::<CdllNode<usize>>().unwrap();
        assert!(n.data == 0);
        assert!(n.next.get().data == 0);
    }

    #[test]
    fn test_interlinking_reference() {
        let n = ARENA.aquire_init_default::<CdllNode<usize>>().unwrap();
        n.insert(ARENA.aquire_init::<CdllNode<usize>>(1).unwrap());

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
            TEST_DROPPED.store(true, std::sync::atomic::Ordering::Release);
        }
    }

    #[test]
    fn test_zero_size() {
        let z0 = ARENA.aquire_default::<Test>().unwrap();
        let z = ARENA.aquire_default::<Test>().unwrap();
        assert!(z.hi() == "hi");
        assert!(z0.hi() == "hi");
    }

    #[test]
    fn test_drop() {
        let arena = Arena::<1>::new();
        let _z = arena.aquire_default::<Test>().unwrap();
        drop(arena);
        assert!(TEST_DROPPED.load(std::sync::atomic::Ordering::Acquire));
    }
}
