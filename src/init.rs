use core::mem::MaybeUninit;

pub trait Init {
    type InitArg;

    fn init(me: &mut MaybeUninit<Self>, arg: Self::InitArg)
    where
        Self: Sized;
}