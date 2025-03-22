use crate::util::typelevel::*;

impl<const N: usize, T> Map<T> for [T; N] {
    type Output<R> = [R; N];

    fn map<R>(self, f: impl Fn(T) -> R) -> Self::Output<R> {
        self.map_mut(f)
    }
}

impl<const N: usize, T> MapMut<T> for [T; N] {
    type Output<R> = [R; N];

    fn map_mut<R>(self, f: impl FnMut(T) -> R) -> Self::Output<R> {
        self.map(f)
    }
}

impl<T> MapOnce<T> for [T; 1] {
    type Output<R> = [R; 1];
    fn map_once<R>(self, f: impl FnOnce(T) -> R) -> Self::Output<R> {
        let [x] = self;
        [f(x)]
    }
}

impl<T> MapOnce<T> for [T; 0] {
    type Output<R> = [R; 0];
    fn map_once<R>(self, _f: impl FnOnce(T) -> R) -> Self::Output<R> {
        []
    }
}

impl<T> Flatten for [T; 1] {
    type Output = T;

    fn flatten(self) -> Self::Output {
        let [x] = self;
        x
    }
}
impl<const N: usize, T, A> Associate<A> for [T; N] {
    type Output = [A; N];
}
