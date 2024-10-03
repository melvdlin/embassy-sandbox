use core::fmt::Display;

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
enum Packet<'a> {
    Rrq(Rwrq<'a>),
    Wrq(Rwrq<'a>),
    Data(Data<'a>),
    Ack(Ack),
    Error(Error<'a>),
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
struct Rwrq<'a> {
    filename: &'a str,
    mode: &'a str,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
struct Data<'a> {
    block_no: u16,
    data: &'a [u8],
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
struct Ack {
    block_no: u16,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
struct Error<'a> {
    error_code: u16,
    message: &'a str,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
struct MalformedPacket;

impl core::error::Error for MalformedPacket {}

impl Display for MalformedPacket {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "malformed packet")
    }
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
enum Opcode {
    Rrq = 1,
    Wrq = 2,
    Data = 3,
    Ack = 4,
    Error = 5,
}

impl TryFrom<u16> for Opcode {
    type Error = UnknownOpcode;

    fn try_from(opcode: u16) -> Result<Self, UnknownOpcode> {
        Ok(match opcode {
            | 1 => Self::Rrq,
            | 2 => Self::Wrq,
            | 3 => Self::Data,
            | 4 => Self::Ack,
            | 5 => Self::Error,
            | n => return Err(UnknownOpcode(n)),
        })
    }
}
#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub struct UnknownOpcode(pub u16);

impl core::error::Error for UnknownOpcode {}

impl Display for UnknownOpcode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown opcode ({})", self.0)
    }
}

mod parser {

    use super::*;
    use branch::*;
    use bytes::streaming::*;
    use combinator::*;
    use nom::*;
    use number::streaming::be_u16;
    use sequence::tuple;

    pub fn parse_packet<'a>(
        block_size: usize,
    ) -> impl FnMut(&'a [u8]) -> IResult<&'a [u8], Packet<'a>> {
        let opcode = |opcode| tag((opcode as u16).to_be_bytes());
        let cstr = || take_until(b"\0" as &[u8]);

        let rrq = map_res(
            tuple((opcode(Opcode::Rrq), cstr(), cstr())),
            |(_opcode, filename, mode)| {
                Ok::<_, core::str::Utf8Error>(Packet::Rrq(Rwrq {
                    filename: core::str::from_utf8(filename)?,
                    mode: core::str::from_utf8(mode)?,
                }))
            },
        );
        let wrq = map_res(
            tuple((opcode(Opcode::Wrq), cstr(), cstr())),
            |(_, filename, mode)| {
                Ok::<_, core::str::Utf8Error>(Packet::Wrq(Rwrq {
                    filename: core::str::from_utf8(filename)?,
                    mode: core::str::from_utf8(mode)?,
                }))
            },
        );
        let data = tuple((
            opcode(Opcode::Data),
            be_u16,
            take_while_m_n(0, block_size, |_| true),
        ))
        .map(|(_, block_no, data)| Packet::Data(Data { block_no, data }));
        let ack = tuple((opcode(Opcode::Ack), be_u16))
            .map(|(_, block_no)| Packet::Ack(Ack { block_no }));
        let error = map_res(
            tuple((opcode(Opcode::Error), be_u16, cstr())),
            |(_, error_code, message)| {
                Ok::<_, core::str::Utf8Error>(Packet::Error(Error {
                    error_code,
                    message: core::str::from_utf8(message)?,
                }))
            },
        );
        alt((rrq, wrq, data, ack, error))
    }
}
