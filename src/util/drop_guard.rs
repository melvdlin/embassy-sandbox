use core::hash::Hash;
use core::mem::ManuallyDrop;

use crate::forget_destructure_tuple;

#[derive(Debug)]
#[derive(Clone)]
#[derive(PartialEq, Eq)]
#[derive(PartialOrd, Ord)]
#[derive(Hash)]
#[derive(Default)]
#[repr(transparent)]
pub struct DropGuard<F: FnOnce()>(ManuallyDrop<F>);

impl<F> DropGuard<F>
where
    F: FnOnce(),
{
    pub const fn new(f: F) -> Self {
        Self(ManuallyDrop::new(f))
    }

    pub fn inner(&self) -> &F {
        &self.0
    }

    pub fn inner_mut(&mut self) -> &F {
        // Safety: always init as per precondition
        &self.0
    }

    pub fn into_inner(self) -> F {
        forget_destructure_tuple!(Self { inner } = self);
        ManuallyDrop::into_inner(inner)
    }
}

impl<F: FnOnce()> Drop for DropGuard<F> {
    fn drop(&mut self) {
        // Safety: we're in the destructor,
        // therefore the original value will not be used or moved after this
        unsafe { ManuallyDrop::take(&mut self.0)() }
    }
}
