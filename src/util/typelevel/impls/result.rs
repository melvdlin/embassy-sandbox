use crate::util::typelevel::*;

impl<T, E> Map<T> for Result<T, E> {
    type Output<R> = Result<R, E>;

    fn map<R>(self, f: impl Fn(T) -> R) -> Self::Output<R> {
        self.map_mut(f)
    }
}

impl<T, E> MapMut<T> for Result<T, E> {
    type Output<R> = Result<R, E>;

    fn map_mut<R>(self, f: impl FnMut(T) -> R) -> Self::Output<R> {
        self.map_once(f)
    }
}

impl<T, E> MapOnce<T> for Result<T, E> {
    type Output<R> = Result<R, E>;
    fn map_once<R>(self, f: impl FnOnce(T) -> R) -> Self::Output<R> {
        self.map(f)
    }
}

impl<T, E> Flatten for Result<Result<T, E>, E> {
    type Output = Result<T, E>;

    fn flatten(self) -> Self::Output {
        self.and_then(core::convert::identity)
    }
}

impl<T, E, A> Associate<A> for Result<T, E> {
    type Output = Result<A, E>;
}
