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
