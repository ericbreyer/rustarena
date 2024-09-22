//! Initialization trait for types that require a circular reference to themselves upon initialization.

use core::mem::MaybeUninit;

/// A trait for initialization of a type that is stored in an arena and
/// requires a circular reference to itself to initialize.
pub trait Init {
    type InitArg;

    fn init(me: &mut MaybeUninit<Self>, arg: Self::InitArg)
    where
        Self: Sized;
}
