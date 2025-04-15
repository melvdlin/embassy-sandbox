use core::ops::Range;

use super::AsciiRangeMap;
use crate::graphics::gui::format::A8;

// TODO: support entire ascii range
pub static FIRA_MONO_16: AsciiRangeMap<'static, A8> =
    parse_pgm(include_bytes!("font/firamono_16.pgm"), 16, 32..127);
pub static FIRA_MONO_18: AsciiRangeMap<'static, A8> =
    parse_pgm(include_bytes!("font/firamono_18.pgm"), 18, 32..127);
pub static FIRA_MONO_24: AsciiRangeMap<'static, A8> =
    parse_pgm(include_bytes!("font/firamono_24.pgm"), 24, 32..127);
pub static FIRA_MONO_28: AsciiRangeMap<'static, A8> =
    parse_pgm(include_bytes!("font/firamono_28.pgm"), 28, 32..127);
pub static FIRA_MONO_32: AsciiRangeMap<'static, A8> =
    parse_pgm(include_bytes!("font/firamono_32.pgm"), 32, 32..127);
pub static FIRA_MONO_40: AsciiRangeMap<'static, A8> =
    parse_pgm(include_bytes!("font/firamono_40.pgm"), 40, 0..128);
pub static FIRA_MONO_48: AsciiRangeMap<'static, A8> =
    parse_pgm(include_bytes!("font/firamono_48.pgm"), 48, 32..127);
pub static FIRA_MONO_56: AsciiRangeMap<'static, A8> =
    parse_pgm(include_bytes!("font/firamono_56.pgm"), 56, 32..127);
pub static FIRA_MONO_64: AsciiRangeMap<'static, A8> =
    parse_pgm(include_bytes!("font/firamono_64.pgm"), 64, 32..127);

const fn parse_pgm(
    data: &[u8],
    char_height: u16,
    mapped_chars: Range<u8>,
) -> AsciiRangeMap<'_, A8> {
    let [b'P', b'5', data @ ..] = data else {
        panic!("magic number doesn't match");
    };
    let Some((width, data)) = next_u32(data) else {
        panic!("failed to parse width");
    };
    let Some((height, data)) = next_u32(data) else {
        panic!("failed to parse width");
    };
    let Some((max_alpha, data)) = next_u32(data) else {
        panic!("failed to parse max gray");
    };
    let data = match next_item(data) {
        | Some(data) => data,
        | None => data,
    };
    assert!(width <= u16::MAX as _);
    assert!(height <= u16::MAX as _);
    assert!(max_alpha <= u8::MAX as _);
    let width = width as u16;
    let height = height as u16;
    let char_size = (width * char_height) as usize;
    assert!(height % char_height == 0);

    let len = mapped_chars.end.saturating_sub(mapped_chars.start) as usize;
    assert!(char_size * (len + 1) == data.len());
    let (chars, fallback) = data.split_at(char_size * len);

    AsciiRangeMap::new(mapped_chars, width, char_height, chars, fallback)
}

const fn next_u32(data: &[u8]) -> Option<(u32, &[u8])> {
    let Some(data) = next_item(data) else {
        return None;
    };
    let Some((n, data)) = split_whitespace(data) else {
        return None;
    };
    let Ok(n) = core::str::from_utf8(n) else {
        return None;
    };
    let Ok(n) = u32::from_str_radix(n, 10) else {
        return None;
    };
    Some((n, data))
}

const fn next_item(data: &[u8]) -> Option<&[u8]> {
    let mut tail = data;
    loop {
        if let Some(trimmed) = consume_whitespace(tail) {
            tail = trimmed;
            continue;
        }
        if let Some(trimmed) = consume_comment(tail) {
            tail = trimmed;
            continue;
        }
        break;
    }
    if tail.len() != data.len() {
        Some(tail)
    } else {
        None
    }
}

const fn consume_whitespace(data: &[u8]) -> Option<&[u8]> {
    let trimmed = data.trim_ascii_start();
    if trimmed.len() != data.len() {
        Some(trimmed)
    } else {
        None
    }
}

const fn consume_comment(data: &[u8]) -> Option<&[u8]> {
    if let [b'#', tail @ ..] = data {
        if let Some((_, next)) = split_line(tail) {
            Some(next)
        } else {
            Some(&[])
        }
    } else {
        None
    }
}

const fn split_whitespace(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut i = 0;
    let mut haystack = data;
    while let [head, tail @ ..] = haystack {
        if head.is_ascii_whitespace() {
            return Some(data.split_at(i));
        }
        haystack = tail;
        i += 1;
    }
    None
}

const fn split_line(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let Some(line_break) = find(data, b"\n") else {
        return None;
    };
    Some(data.split_at(line_break + 1))
}

const fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let mut offset = 0;
    while let Some((_, window, _)) = try_slice(haystack, offset..offset + needle.len()) {
        if eq(window, needle) {
            return Some(offset);
        }
        offset += 1;
    }
    None
}

#[allow(dead_code)]
const fn contains(mut haystack: &[u8], needle: u8) -> bool {
    while let [head, tail @ ..] = haystack {
        if *head == needle {
            return true;
        }
        haystack = tail;
    }
    false
}

#[allow(dead_code)]
const fn find_any(mut haystack: &[u8], needles: &[u8]) -> Option<usize> {
    let mut i = 0;
    while let [head, tail @ ..] = haystack {
        if contains(needles, *head) {
            return Some(i);
        }
        haystack = tail;
        i += 1;
    }
    None
}

const fn eq(mut haystack: &[u8], mut needle: &[u8]) -> bool {
    if haystack.len() != needle.len() {
        return false;
    }
    while let ([x, xs @ ..], [y, ys @ ..]) = (haystack, needle) {
        if *x != *y {
            return false;
        }
        haystack = xs;
        needle = ys;
    }
    true
}

const fn try_slice<T>(slice: &[T], range: Range<usize>) -> Option<(&[T], &[T], &[T])> {
    let Range { start, end } = range;
    let Some((head, tail)) = slice.split_at_checked(end) else {
        return None;
    };
    let Some((head, mid)) = head.split_at_checked(start) else {
        return None;
    };
    Some((head, mid, tail))
}

#[allow(dead_code)]
const fn slice<T>(slice: &[T], range: Range<usize>) -> (&[T], &[T], &[T]) {
    let Range { start, end } = range;
    let (head, tail) = slice.split_at(end);
    let (head, mid) = head.split_at(start);
    (head, mid, tail)
}
