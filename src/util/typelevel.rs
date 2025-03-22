pub mod associate;
pub mod flatten;
pub mod map;

pub mod option;

pub use associate::Associate;
pub use flatten::Flatten;
pub use map::Map;
pub use map::MapMut;
pub use map::MapOnce;
pub use option::None;
pub use option::Some;

mod impls;

pub mod eval {
    pub type Map<T, R> = <T as super::Map<T>>::Output<R>;
    pub type MapMut<T, R> = <T as super::MapMut<T>>::Output<R>;
    pub type MapOnce<T, R> = <T as super::MapOnce<T>>::Output<R>;
    pub type Associate<T, A> = <T as super::Associate<A>>::Output;
}
