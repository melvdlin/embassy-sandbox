pub trait Map<T> {
    type Output<R>;
    fn map<R>(self, f: impl Fn(T) -> R) -> Self::Output<R>;
}

pub trait MapMut<T> {
    type Output<R>;
    fn map_mut<R>(self, f: impl FnMut(T) -> R) -> Self::Output<R>;
}

pub trait MapOnce<T> {
    type Output<R>;
    fn map_once<R>(self, f: impl FnOnce(T) -> R) -> Self::Output<R>;
}
