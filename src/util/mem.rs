use core::borrow::Borrow;
use core::borrow::BorrowMut;
use core::ops::Deref;
use core::ops::DerefMut;

/// Commits the content of `buf` by volatilely re-writing it in place.
pub fn flush<T>(mut buf: &mut [T]) {
    while let Some((head, tail)) = core::mem::take(&mut buf).split_first_mut() {
        buf = tail;
        // Safety: head is a valid &mut T
        unsafe {
            let head = head as *mut T;
            let val = head.read();
            head.write_volatile(val);
        }
    }
}

/// A `&mut [T]` wrapper that calls `flush` after every modification.
#[derive(Debug)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
pub struct VolatileBuf<'a, T>(&'a mut T)
where
    T: ?Sized;

impl<'a, T> VolatileBuf<'a, T>
where
    T: ?Sized,
{
    pub const fn new(buf: &'a mut T) -> Self {
        Self(buf)
    }

    pub const fn into_inner(self) -> &'a mut T {
        self.0
    }
}

impl<T> VolatileBuf<'_, [T]> {
    /// Modify the contained slice and flush afterwards.
    pub fn modify<R>(&mut self, f: impl for<'b> FnOnce(&'b mut [T]) -> R) -> R {
        let result = f(self.0);
        flush(self.0);
        result
    }
}

/// A `&mut [T]` wrapper that calls `flush` on drop.
#[derive(Debug)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
pub struct FlushGuard<'a, T>(pub &'a mut [T]);

impl<T> Borrow<[T]> for FlushGuard<'_, T> {
    #[inline(always)]
    fn borrow(&self) -> &[T] {
        self.0
    }
}

impl<T> BorrowMut<[T]> for FlushGuard<'_, T> {
    #[inline(always)]
    fn borrow_mut(&mut self) -> &mut [T] {
        self.0
    }
}

impl<T> Deref for FlushGuard<'_, T> {
    type Target = [T];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.borrow()
    }
}

impl<T> DerefMut for FlushGuard<'_, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.borrow_mut()
    }
}

impl<T> AsRef<[T]> for FlushGuard<'_, T> {
    #[inline(always)]
    fn as_ref(&self) -> &[T] {
        self
    }
}

impl<T> AsMut<[T]> for FlushGuard<'_, T> {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut [T] {
        self
    }
}

impl<T> Drop for FlushGuard<'_, T> {
    #[inline(always)]
    fn drop(&mut self) {
        flush(self);
    }
}
