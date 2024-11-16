#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command<'a> {
    Echo(Echo<'a>),
    Download(Download<'a>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Echo<'arg> {
    echo: &'arg [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Download<'filename> {
    filename: &'filename [u8],
}

mod parser {
    use bytes::streaming::*;
    use character::streaming::multispace0;
    use character::streaming::multispace1;
    use character::streaming::space1;
    use combinator::*;
    use nom::branch::*;
    use nom::error::Error as NomError;
    use nom::sequence::*;
    use nom::*;

    pub fn arg<'i>() -> impl FnMut(&'i [u8]) -> IResult<&'i [u8], &'i [u8]> {
        preceded(
            multispace0,
            alt((complete(tagged_delim(b"\"")), is_not(b" \t\r\n".as_slice()))),
        )
    }

    pub fn tagged_delim<'d, 'i>(
        delim: &'d [u8],
    ) -> impl 'd + Fn(&'i [u8]) -> IResult<&'i [u8], &'i [u8]> + Copy {
        move |input: &'i [u8]| {
            let incomplete = nom::Err::Incomplete(Needed::Unknown);

            let Some(delim_pos) = memchr::memmem::find(input, delim) else {
                return Err(incomplete);
            };

            let tag = &input[..delim_pos];
            let tail = &input[delim_pos + 1..];

            let Some(end_delim_pos) =
                memchr::memmem::find_iter(tail, tag).find_map(|tag_pos| {
                    let delim_pos = tag_pos.checked_sub(delim.len())?;
                    (&tail[delim_pos..tag_pos] == delim).then_some(delim_pos)
                })
            else {
                return Err(incomplete);
            };

            Ok((
                &tail[end_delim_pos + delim.len() + tag.len()..],
                &tail[..end_delim_pos],
            ))
        }
    }

    #[cfg(test)]
    mod tests {
        use character::complete::multispace0;

        use super::*;

        #[test]
        fn test_tagged_delim() {
            let parser = tagged_delim(b"\"");

            assert_eq!(
                parser(b"\" foo bar\""),
                Ok((b"".as_slice(), b" foo bar".as_slice()))
            );

            assert_eq!(
                parser(b"asdf\"lorem ipsum \"dolor sit\"asdfqwertz uiop"),
                Ok((
                    b"qwertz uiop".as_slice(),
                    b"lorem ipsum \"dolor sit".as_slice()
                ))
            );

            assert_eq!(
                parser(b"as df\" foo bar\"as df"),
                Ok((b"".as_slice(), b" foo bar".as_slice()))
            );
        }

        #[test]
        fn test_arg() {
            let mut parser = arg();

            let input = b"lorem ipsum \"dolor sit amet,\"
                          tag\"consectetur \"adipiscing\" elit!\"tag 
                          ut finibus pretium fermentum. 124e+6317.12    \t\n ";

            let (rest, arg) = parser.parse(input).unwrap();
            assert_eq!(arg, b"lorem");
            let (rest, arg) = parser.parse(rest).unwrap();
            assert_eq!(arg, b"ipsum");
            let (rest, arg) = parser.parse(rest).unwrap();
            assert_eq!(arg, b"dolor sit amet,");
            let (rest, arg) = parser.parse(rest).unwrap();
            assert_eq!(arg, b"consectetur \"adipiscing\" elit!");
            let (rest, arg) = parser.parse(rest).unwrap();
            assert_eq!(arg, b"ut");
            let (rest, arg) = parser.parse(rest).unwrap();
            assert_eq!(arg, b"finibus");
            let (rest, arg) = parser.parse(rest).unwrap();
            assert_eq!(arg, b"pretium");
            let (rest, arg) = parser.parse(rest).unwrap();
            assert_eq!(arg, b"fermentum.");
            let (rest, arg) = terminated(parser, multispace0).parse(rest).unwrap();
            assert_eq!(arg, b"124e+6317.12");
            assert_eq!(rest, b"");
        }
    }
}
