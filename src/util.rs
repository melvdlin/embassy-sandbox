pub trait ByteSliceExt {
    fn trim_ascii_start_mut(&mut self) -> &mut Self;
    fn trim_ascii_end_mut(&mut self) -> &mut Self;
    fn trim_ascii_mut(&mut self) -> &mut Self;
}

impl ByteSliceExt for [u8] {
    fn trim_ascii_start_mut(&mut self) -> &mut Self {
        let start =
            self.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(self.len());
        &mut self[start..]
    }

    fn trim_ascii_end_mut(&mut self) -> &mut Self {
        let end = self.len()
            - self
                .iter()
                .rev()
                .position(|c| !c.is_ascii_whitespace())
                .unwrap_or(self.len());
        &mut self[..end]
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
