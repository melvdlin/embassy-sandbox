use super::Associate;
use super::Flatten;
use super::Map;
use super::MapMut;
use super::MapOnce;

pub struct Some<T>(pub T);
pub struct None;

impl<T> Map<T> for Some<T> {
    type Output<R> = <Self as MapMut<T>>::Output<R>;
    fn map<R>(self, f: impl Fn(T) -> R) -> Self::Output<R> {
        self.map_mut(f)
    }
}

impl<T> MapMut<T> for Some<T> {
    type Output<R> = <Self as MapOnce<T>>::Output<R>;
    fn map_mut<R>(self, f: impl FnMut(T) -> R) -> Self::Output<R> {
        self.map_once(f)
    }
}

impl<T> MapOnce<T> for Some<T> {
    type Output<R> = Some<R>;
    fn map_once<R>(self, f: impl FnOnce(T) -> R) -> Self::Output<R> {
        Some(f(self.0))
    }
}

impl<T> Map<T> for None {
    type Output<R> = <Self as MapMut<T>>::Output<R>;
    fn map<R>(self, f: impl Fn(T) -> R) -> Self::Output<R> {
        self.map_mut(f)
    }
}

impl<T> MapMut<T> for None {
    type Output<R> = <Self as MapOnce<T>>::Output<R>;
    fn map_mut<R>(self, f: impl FnMut(T) -> R) -> Self::Output<R> {
        self.map_once(f)
    }
}

impl<T> MapOnce<T> for None {
    type Output<R> = None;
    fn map_once<R>(self, _f: impl FnOnce(T) -> R) -> Self::Output<R> {
        None
    }
}

impl<T> Flatten for Some<Some<T>> {
    type Output = Some<T>;
    fn flatten(self) -> Some<T> {
        self.0
    }
}

impl Flatten for Some<None> {
    type Output = None;
    fn flatten(self) -> Self::Output {
        self.0
    }
}

impl<T> core::ops::Not for Some<T> {
    type Output = None;

    fn not(self) -> Self::Output {
        None
    }
}

impl<T, A> Associate<A> for Some<T> {
    type Output = Some<A>;
}

impl<A> Associate<A> for None {
    type Output = None;
}
