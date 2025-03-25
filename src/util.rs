pub mod typelevel;

pub trait ByteSliceExt {
    fn trim_ascii_start_mut(&mut self) -> &mut Self;
    fn trim_ascii_end_mut(&mut self) -> &mut Self;
    fn trim_ascii_mut(&mut self) -> &mut Self;
}

impl ByteSliceExt for [u8] {
    fn trim_ascii_start_mut(&mut self) -> &mut Self {
        let len = self.trim_ascii_start().len();
        let start = self.len() - len;
        &mut self[start..]
    }

    fn trim_ascii_end_mut(&mut self) -> &mut Self {
        let len = self.trim_ascii_end().len();
        &mut self[..len]
    }

    fn trim_ascii_mut(&mut self) -> &mut Self {
        self.trim_ascii_start_mut().trim_ascii_end_mut()
    }
}

#[cfg(test)]
mod tests {
    use crate::ByteSliceExt;

    #[test]
    fn test_trim_start() {
        let mut s = *b"  lorem ipsum ";
        assert_eq!(s.trim_ascii_start_mut(), b"lorem ipsum ");
        let mut s = *b"lorem ipsum ";
        assert_eq!(s.trim_ascii_start_mut(), b"lorem ipsum ");
        let mut s = *b" ";
        assert_eq!(s.trim_ascii_start_mut(), b"");
        let mut s = *b"";
        assert_eq!(s.trim_ascii_start_mut(), b"".as_slice());
    }

    #[test]
    fn test_trim_end() {
        let mut s = *b" lorem ipsum  ";
        assert_eq!(s.trim_ascii_end_mut(), b" lorem ipsum");
        let mut s = *b" lorem ipsum";
        assert_eq!(s.trim_ascii_end_mut(), b" lorem ipsum");
        let mut s = *b" ";
        assert_eq!(s.trim_ascii_end_mut(), b"");
        let mut s = *b"";
        assert_eq!(s.trim_ascii_end_mut(), b"");
    }
}

pub async fn until(mut p: impl FnMut() -> bool) {
    while !p() {
        embassy_futures::yield_now().await;
    }
}

#[macro_export]
macro_rules! __fields {
    ($($field_id:ident),* $(,)?; $expr:expr) => {
        Self {
            $(
                $field_id: $expr,
            )*
        }
    };
    ($($field_id:ident),* $(,)?; $self:expr; $self_field:pat => $expr:expr) => {
        Self {
            $(
                $field_id: {
                    let $self_field = $self.$field_id;
                    $expr
                },
            )*
        }
    };
    ($($field_id:ident),* $(,)?; $self:expr, $other:expr; $self_field:pat, $other_field:pat => $expr:expr) => {
        Self {
            $(
                $field_id: {
                    let $self_field = $self.$field_id;
                    let $other_field = $other.$field_id;
                    $expr
                },
            )*
        }
    };
}

#[macro_export]
macro_rules! flags {
    (
        $(#[$outer:meta])*
        $vis:vis struct $id:ident {
            $(
                $(#[$inner:ident $($args:tt)*])*
                $field_vis:vis $field_id:ident: bool,
            )*
        }
    ) => {
        preinterpret::preinterpret! {
            $(#[$outer])*
            #[derive(Debug)]
            #[derive(Clone, Copy)]
            #[derive(PartialEq, Eq)]
            #[derive(Hash)]
            #[derive(Default)]
            $vis struct $id {
                $(
                    $(#[$inner $($args)*])*
                    $field_vis $field_id: bool,
                )*
            }


            #[allow(unused)]
            impl $id {
                $(
                    $field_vis const [!ident_upper_snake! $field_id]: Self = Self {
                        $field_id: true,
                        ..Self::none()
                    };
                )*

                pub const fn none() -> Self {
                    $crate::__fields!(
                        $($field_id,)*;
                        false
                    )
                }

                pub const fn all() -> Self {
                    $crate::__fields!(
                        $($field_id,)*;
                        true
                    )
                }

                pub const fn not(self) -> Self {
                    $crate::__fields!(
                        $($field_id,)*;
                        self;
                        field => !field
                    )
                }

                pub const fn and(self, other: Self) -> Self {
                    $crate::__fields!(
                        $($field_id,)*;
                        self, other;
                        field, other_field => field & other_field
                    )
                }

                pub const fn or(self, other: Self) -> Self {
                    $crate::__fields!(
                        $($field_id,)*;
                        self, other;
                        field, other_field => field | other_field
                    )
                }

                pub const fn xor(self, other: Self) -> Self {
                    $crate::__fields!(
                        $($field_id,)*;
                        self, other;
                        field, other_field => field ^ other_field
                    )
                }
            }

            impl core::ops::Not for $id {
                type Output = Self;
                fn not(self) -> Self {
                    self.not()
                }
            }

            impl core::ops::BitOr for $id {
                type Output = Self;
                fn bitor(self, other: Self) -> Self {
                    self.or(other)
                }
            }

            impl core::ops::BitAnd for $id {
                type Output = Self;
                fn bitand(self, other: Self) -> Self {
                    self.and(other)
                }
            }

            impl core::ops::BitXor for $id {
                type Output = Self;
                fn bitxor(self, other: Self) -> Self {
                    self.xor(other)
                }
            }
        }
    };

}
