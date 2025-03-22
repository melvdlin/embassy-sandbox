pub trait Flatten {
    type Output;
    fn flatten(self) -> Self::Output;
}
