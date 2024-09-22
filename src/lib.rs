#![no_std]
//! # Rust arena allocator
//!
//! ## Description
//! A small, thread-safe, no-std, arena allocator with a static backing store and ability to allocate arbitrary types.
//!
//! ## Examples
//! ### Simple Types
//!
//! ```
//! use arena_alloc::Arena;
//! static ARENA: Arena<1000> = Arena::new();
//!
//! fn main() {
//!     let two = ARENA.acquire(2).unwrap();
//!     let zero = ARENA.acquire_default::<usize>().unwrap();
//!
//!     assert_eq!(*two, 2);
//!     assert_eq!(*zero, 0);
//! }
//! ```
//! 
//! ### Self Referential Types
//! 
//! ```
//! use arena_alloc::{Arena, Init};
//! use std::cell::Cell;
//! use std::ptr;
//! use std::mem::MaybeUninit;
//!     
//! static ARENA: Arena<1000> = Arena::new();
//! 
//! struct CllNode<'b, T> {
//!     data: T,
//!     next: Cell<&'b Self>,
//! }
//! 
//! impl<'b, T> CllNode<'b, T> {
//!     fn cons(&'b self, other: &'b CllNode<'b, T>) {
//!         self.next.set(other);
//!     }
//! }
//! 
//! impl<'b, T> Init for CllNode<'b, T> {
//!     type InitArg = T;
//!     fn init(me: &mut MaybeUninit<Self>, arg: T) {
//!         unsafe {
//!             me.write(CllNode {
//!                 data: arg,
//!                 next: Cell::new(ptr::from_ref(me).cast::<Self>().as_ref().unwrap()),
//!             });
//!         }
//!     }
//! }
//! 
//! fn main() {
//!     let n0 = ARENA.acquire_init_default::<CllNode<usize>>().unwrap();
//!     let n1 = ARENA.acquire_init::<CllNode<_>>(1).unwrap();
//!     let n2 = ARENA.acquire_init::<CllNode<_>>(2).unwrap();
//! 
//!     n0.cons(n1);
//!     n1.cons(n2);
//!     n2.cons(n0);
//!     assert_eq!(n0.next.get().data, 1);
//!     assert_eq!(n1.next.get().data, 2);
//!     assert_eq!(n2.next.get().data, 0);
//! }
//! ```
//!

use core::{cell::UnsafeCell, mem::MaybeUninit, ptr, sync::atomic::{AtomicUsize, Ordering}, usize};
pub use init::Init;

mod init;

type MemSlice<const SIZE: usize> = [u8; SIZE];

#[derive(Clone, Copy)]
struct Dropper<const SIZE: usize> {
    place: usize,
    drop_func: fn(*mut MemSlice<SIZE>),
}

/// A fixed size arena that can be used to allocate memory for arbitrary types.
pub struct Arena<const SIZE: usize> {
    backing_store: UnsafeCell<MemSlice<SIZE>>,
    next_free_store_spot: AtomicUsize,
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
    /// Create a new arena with a fixed size buffer of SIZE bytes.
    #[must_use]
    pub const fn new() -> Self {
        Arena {
            backing_store: UnsafeCell::new([0; SIZE]),
            next_free_store_spot: AtomicUsize::new(0),
            drop_queue: UnsafeCell::new([None; SIZE]),
            next_free_drop_spot: AtomicUsize::new(0),
        }
    }

    /// Get a pointer to a place in the backing store where a value of type T can be placed.
    fn get_ptr_place<T>(&'a self) -> Option<(usize, &mut MaybeUninit<T>)> {
        let place = self.next_free_store_spot.fetch_add(
            core::mem::size_of::<T>(),
            Ordering::Release,
        );
        if place + core::mem::size_of::<T>() > SIZE {
            return None;
        }

        let ptr = unsafe {
            self.backing_store
                .get()
                .byte_add(place)
                .cast::<MaybeUninit<T>>()
                .as_mut()
                .unwrap()
        };

        Some((place, ptr))
    }

    /// Add a dropper function for type T at the given place to the drop queue.
    fn add_to_drop_queue<T>(&'a self, place: usize) {
        let dq = unsafe { self.drop_queue.get().as_mut() }.unwrap();
        dq[self
            .next_free_drop_spot
            .fetch_add(1, Ordering::Relaxed)] = Some(Dropper {
            place,
            drop_func: |ptr: *mut MemSlice<SIZE>| unsafe {
                ptr.cast::<T>().drop_in_place();
            },
        });
    }

    /// acquire a reference to a value of type T that can be initialized with
    /// the Init trait, using the default value of the InitArg.
    /// This is useful for types that require initialization and the init arg is Default.
    pub fn acquire_init_default<T: Init>(&'a self) -> Option<&'a T>
    where
        T::InitArg: Default,
    {
        let (place, ptr) = self.get_ptr_place::<T>()?;

        T::init(ptr, T::InitArg::default());

        self.add_to_drop_queue::<T>(place);

        Some(unsafe {
            ptr::from_ref(ptr)
                .cast::<T>()
                .as_ref()
                .unwrap_unchecked()
        })
    }

    /// acquire a reference to a value of type T that can be initialized with
    /// the Init trait, using a given InitArg.
    /// This is useful for types that require initialization.
    pub fn acquire_init<T: Init>(&'a self, arg: T::InitArg) -> Option<&'a T> {
        let (place, ptr) = self.get_ptr_place::<T>()?;

        T::init(ptr, arg);

        self.add_to_drop_queue::<T>(place);

        Some(unsafe {
            ptr::from_ref(ptr)
                .cast::<T>()
                .as_ref()
                .unwrap_unchecked()
        })
    }

    /// acquire a reference to a value of type T that is initialized with it's default value.
    /// This is useful for types that do not require initialization.
    pub fn acquire_default<T: Default>(&'a self) -> Option<&'a T> {
        let (place, ptr) = self.get_ptr_place::<T>()?;

        ptr.write(T::default());

        self.add_to_drop_queue::<T>(place);

        Some(unsafe {
            ptr::from_ref(ptr)
                .cast::<T>()
                .as_ref()
                .unwrap_unchecked()
        })
    }

    /// acquire a reference to a value of type T that is initialized with the given value.
    /// This is useful for types that do not require initialization.
    pub fn acquire<T>(&'a self, val: T) -> Option<&'a T> {
        let (place, ptr) = self.get_ptr_place::<T>()?;

        ptr.write(val);

        self.add_to_drop_queue::<T>(place);

        Some(unsafe {
            ptr::from_ref(ptr)
                .cast::<T>()
                .as_ref()
                .unwrap_unchecked()
        })
    }
}

impl<const SIZE: usize> Drop for Arena<SIZE> {
    fn drop(&mut self) {
        for pair in self.drop_queue.get_mut() {
            let Some(Dropper { place, drop_func }) = pair else {
                break;
            };
            let ptr = unsafe { self.backing_store.get().byte_add(*place) };
            drop_func(ptr);
        }
    }
}

#[cfg(test)]
mod test;
