use crate::util::typelevel::*;

impl<T> Map<T> for Option<T> {
    type Output<R> = Option<R>;

    fn map<R>(self, f: impl Fn(T) -> R) -> Self::Output<R> {
        self.map_mut(f)
    }
}

impl<T> MapMut<T> for Option<T> {
    type Output<R> = Option<R>;

    fn map_mut<R>(self, f: impl FnMut(T) -> R) -> Self::Output<R> {
        self.map_once(f)
    }
}

impl<T> MapOnce<T> for Option<T> {
    type Output<R> = Option<R>;
    fn map_once<R>(self, f: impl FnOnce(T) -> R) -> Self::Output<R> {
        self.map(f)
    }
}

impl<T> Flatten for Option<Option<T>> {
    type Output = Option<T>;

    fn flatten(self) -> Self::Output {
        self.flatten()
    }
}

impl<T, E> Flatten for Option<Result<T, E>> {
    type Output = Option<T>;

    fn flatten(self) -> Self::Output {
        self.and_then(Result::ok)
    }
}

impl<T, A> Associate<A> for Option<T> {
    type Output = Option<A>;
}
