use std::fmt;

use anyhow::{anyhow, bail, Result};
use once_cell::sync::Lazy;

const B85_CHARSET: &[u8] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{/}~";
const B85_PADDING: u32 = b'~' as u32;

static REVERSE_LOOKUP: Lazy<[i16; 256]> = Lazy::new(|| {
    let mut table = [-1_i16; 256];
    for (index, byte) in B85_CHARSET.iter().copied().enumerate() {
        table[byte as usize] = index as i16;
    }
    table
});

#[derive(Debug, Clone)]
pub enum Token {
    Sep1,
    Sep2,
    VarInt(u32),
    VarBit(u32),
    Part(Part),
    Str(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartSubType {
    None,
    Int,
    List,
}

#[derive(Debug, Clone)]
pub struct Part {
    pub index: u32,
    pub subtype: PartSubType,
    pub value: u32,
    pub values: Vec<u32>,
}

impl fmt::Display for Part {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.subtype {
            PartSubType::None => write!(f, "{{{}}}", self.index),
            PartSubType::Int => write!(f, "{{{}:{}}}", self.index, self.value),
            PartSubType::List => {
                write!(f, "{{{}:[", self.index)?;
                for (idx, val) in self.values.iter().enumerate() {
                    if idx > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{val}")?;
                }
                write!(f, "]}}")
            }
        }
    }
}

pub fn decode_base85(serial: &str) -> Result<Vec<u8>> {
    if !serial.starts_with("@U") {
        bail!("serial must start with \"@U\"");
    }

    let payload = &serial[2..];
    let mut result = Vec::with_capacity(payload.len() * 4 / 5 + 4);
    let mut idx = 0;
    let bytes = payload.as_bytes();

    while idx < bytes.len() {
        let mut value: u32 = 0;
        let mut char_count = 0;

        while idx < bytes.len() && char_count < 5 {
            let ch = bytes[idx];
            idx += 1;

            let mapped = REVERSE_LOOKUP[ch as usize];
            if mapped >= 0 {
                value = value
                    .checked_mul(85)
                    .and_then(|v| v.checked_add(mapped as u32))
                    .ok_or_else(|| anyhow!("base85 accumulator overflow"))?;
                char_count += 1;
            }
        }

        if char_count == 0 {
            break;
        }

        if char_count < 5 {
            for _ in 0..(5 - char_count) {
                value = value
                    .checked_mul(85)
                    .and_then(|v| v.checked_add(B85_PADDING))
                    .ok_or_else(|| anyhow!("base85 padding overflow"))?;
            }
        }

        let mut bytes_out = [0_u8; 4];
        bytes_out[0] = ((value >> 24) & 0xFF) as u8;
        bytes_out[1] = ((value >> 16) & 0xFF) as u8;
        bytes_out[2] = ((value >> 8) & 0xFF) as u8;
        bytes_out[3] = (value & 0xFF) as u8;

        let byte_count = if char_count < 5 { char_count - 1 } else { 4 };
        result.extend_from_slice(&bytes_out[..byte_count]);
    }

    for byte in &mut result {
        *byte = mirror_byte(*byte);
    }

    Ok(result)
}

pub fn encode_base85(data: &[u8]) -> String {
    if data.is_empty() {
        return "@U".to_string();
    }

    let mut bytes = data.to_vec();
    for byte in &mut bytes {
        *byte = mirror_byte(*byte);
    }

    let mut result = String::with_capacity(bytes.len() * 5 / 4 + 5);
    result.push_str("@U");

    let full_groups = bytes.len() / 4;
    let extra_bytes = bytes.len() % 4;

    for chunk in bytes.chunks_exact(4) {
        let mut value = ((chunk[0] as u32) << 24)
            | ((chunk[1] as u32) << 16)
            | ((chunk[2] as u32) << 8)
            | chunk[3] as u32;

        let mut digits = [0_u8; 5];
        for digit in digits.iter_mut().rev() {
            *digit = (value % 85) as u8;
            value /= 85;
        }

        for digit in digits {
            result.push(B85_CHARSET[digit as usize] as char);
        }
    }

    if extra_bytes != 0 {
        let mut value: u32 = 0;
        for &byte in &bytes[full_groups * 4..] {
            value = (value << 8) | byte as u32;
        }
        value <<= (4 - extra_bytes) * 8;

        let mut digits = [0_u8; 5];
        for digit in digits.iter_mut().rev() {
            *digit = (value % 85) as u8;
            value /= 85;
        }

        for digit in digits.iter().take(extra_bytes + 1) {
            result.push(B85_CHARSET[*digit as usize] as char);
        }
    }

    result
}

#[inline]
fn mirror_byte(byte: u8) -> u8 {
    let mut b = byte;
    b = (b & 0xF0) >> 4 | (b & 0x0F) << 4;
    b = (b & 0xCC) >> 2 | (b & 0x33) << 2;
    (b & 0xAA) >> 1 | (b & 0x55) << 1
}

#[inline]
fn mirror_bits(value: u32, count: u8) -> u32 {
    let mut out = 0_u32;
    for i in 0..count {
        if (value & (1 << i)) != 0 {
            out |= 1 << (count - 1 - i);
        }
    }
    out
}

struct BitReader {
    data: Vec<u8>,
    bit_len: usize,
    pos: usize,
}

impl BitReader {
    fn new(data: Vec<u8>) -> Self {
        let bit_len = data.len() * 8;
        Self {
            data,
            bit_len,
            pos: 0,
        }
    }

    fn read_bit(&mut self) -> Option<u8> {
        if self.pos >= self.bit_len {
            return None;
        }
        let byte = self.data[self.pos / 8];
        let bit = (byte >> (7 - (self.pos % 8))) & 1;
        self.pos += 1;
        Some(bit)
    }

    fn read_bits(&mut self, count: usize) -> Option<u32> {
        if count == 0 || count > 32 || self.pos + count > self.bit_len {
            return None;
        }
        let mut value = 0_u32;
        for _ in 0..count {
            value = (value << 1) | self.read_bit()? as u32;
        }
        Some(value)
    }

    fn read_two_bits(&mut self) -> Option<(u8, u8)> {
        let first = self.read_bit()?;
        let second = self.read_bit()?;
        Some((first, second))
    }

    fn rewind(&mut self, count: usize) {
        self.pos = self.pos.saturating_sub(count);
    }

    fn pos(&self) -> usize {
        self.pos
    }

    fn len_bits(&self) -> usize {
        self.bit_len
    }

    fn as_bit_string(&self) -> String {
        let mut br = Self {
            data: self.data.clone(),
            bit_len: self.bit_len,
            pos: 0,
        };
        let mut s = String::with_capacity(self.bit_len);
        while let Some(bit) = br.read_bit() {
            s.push(if bit == 1 { '1' } else { '0' });
        }
        s
    }
}

struct BitWriter {
    data: Vec<u8>,
    pos: usize,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            data: Vec::with_capacity(256),
            pos: 0,
        }
    }

    fn write_bit(&mut self, bit: u8) {
        if self.pos / 8 >= self.data.len() {
            self.data.push(0);
        }

        if bit & 1 == 1 {
            self.data[self.pos / 8] |= 1 << (7 - (self.pos % 8));
        } else {
            self.data[self.pos / 8] &= !(1 << (7 - (self.pos % 8)));
        }
        self.pos += 1;
    }

    fn write_bits(&mut self, bits: &[u8]) {
        for &bit in bits {
            self.write_bit(bit);
        }
    }

    fn write_n(&mut self, value: u32, count: usize) {
        for shift in (0..count).rev() {
            let bit = ((value >> shift) & 1) as u8;
            self.write_bit(bit);
        }
    }

    fn into_vec(self) -> Vec<u8> {
        self.data
    }

    fn pos(&self) -> usize {
        self.pos
    }

    fn bit_vec(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.pos);
        for i in 0..self.pos {
            let bit = (self.data[i / 8] >> (7 - (i % 8))) & 1;
            result.push(bit);
        }
        result
    }
}

struct Tokenizer {
    reader: BitReader,
    split_positions: Vec<usize>,
}

impl Tokenizer {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            reader: BitReader::new(bytes),
            split_positions: Vec::new(),
        }
    }

    fn next_token(&mut self) -> Result<Option<TokenKind>> {
        self.split_positions.push(self.reader.pos());

        let (b1, b2) = match self.reader.read_two_bits() {
            Some(bits) => bits,
            None => return Ok(None),
        };

        let tok = (b1 << 1) | b2;
        match tok {
            0b00 => Ok(Some(TokenKind::Sep1)),
            0b01 => Ok(Some(TokenKind::Sep2)),
            _ => {
                let b3 = match self.reader.read_bit() {
                    Some(bit) => bit,
                    None => {
                        self.reader.rewind(3);
                        return Ok(None);
                    }
                };
                let full = (tok << 1) | b3;
                match full {
                    0b100 => Ok(Some(TokenKind::VarInt)),
                    0b110 => Ok(Some(TokenKind::VarBit)),
                    0b101 => Ok(Some(TokenKind::Part)),
                    0b111 => Ok(Some(TokenKind::String)),
                    _ => {
                        self.reader.rewind(3);
                        bail!("invalid token {:03b} at bit {}", full, self.reader.pos());
                    }
                }
            }
        }
    }

    fn expect(&mut self, expected: &[u8]) -> Result<()> {
        for &bit in expected {
            let actual = self
                .reader
                .read_bit()
                .ok_or_else(|| anyhow!("unexpected end of data"))?;
            if actual != bit {
                bail!("expected bit {bit}, got {actual}");
            }
        }
        Ok(())
    }

    fn into_reader(self) -> BitReader {
        self.reader
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Sep1,
    Sep2,
    VarInt,
    VarBit,
    Part,
    String,
}

fn read_varint(reader: &mut BitReader) -> Result<u32> {
    let mut data_read = 0_u32;
    let mut output = 0_u32;

    for _ in 0..4 {
        let block = reader
            .read_bits(4)
            .ok_or_else(|| anyhow!("unexpected end of data while reading varint block"))?;
        let mirrored = mirror_bits(block, 4);
        output |= mirrored << data_read;
        data_read += 4;

        let cont = reader
            .read_bit()
            .ok_or_else(|| anyhow!("unexpected end of data while reading varint continuation"))?;
        if cont == 0 {
            break;
        }
    }

    Ok(output)
}

fn write_varint(writer: &mut BitWriter, mut value: u32) {
    let mut bits_needed = if value == 0 {
        1
    } else {
        32 - value.leading_zeros()
    } as usize;
    if bits_needed > 16 {
        bits_needed = 16;
    }
    if bits_needed == 0 {
        bits_needed = 4;
    }

    while bits_needed > 4 {
        let mut block = 0_u8;
        for i in 0..4 {
            block |= ((value & 1) as u8) << i;
            value >>= 1;
        }
        let mirrored = mirror_bits(block as u32, 4);
        writer.write_n(mirrored, 4);
        writer.write_bit(1);
        bits_needed -= 4;
    }

    let mut block = 0_u8;
    for i in 0..4 {
        if bits_needed > 0 {
            block |= ((value & 1) as u8) << i;
            value >>= 1;
            bits_needed -= 1;
        }
    }
    let mirrored = mirror_bits(block as u32, 4);
    writer.write_n(mirrored, 4);
    writer.write_bit(0);
}

fn read_varbit(reader: &mut BitReader) -> Result<u32> {
    let length = reader
        .read_bits(5)
        .ok_or_else(|| anyhow!("unexpected end of data while reading varbit length"))?;
    let mirrored_length = mirror_bits(length, 5);
    if mirrored_length == 0 {
        return Ok(0);
    }

    let mut value = 0_u32;
    for i in 0..mirrored_length {
        let bit = reader
            .read_bit()
            .ok_or_else(|| anyhow!("unexpected end of data while reading varbit value"))?;
        value |= (bit as u32) << i;
    }
    Ok(value)
}

fn write_varbit(writer: &mut BitWriter, mut value: u32) {
    let mut bits = if value == 0 {
        1
    } else {
        32 - value.leading_zeros()
    } as usize;
    if bits > 31 {
        bits = 31;
    }
    let mirrored_length = mirror_bits(bits as u32, 5);
    writer.write_n(mirrored_length, 5);
    for _ in 0..bits {
        writer.write_bit((value & 1) as u8);
        value >>= 1;
    }
}

fn read_string(reader: &mut BitReader) -> Result<String> {
    let length = read_varint(reader)?;
    let mut buf = Vec::with_capacity(length as usize);
    for _ in 0..length {
        let bits = reader
            .read_bits(7)
            .ok_or_else(|| anyhow!("unexpected end of data while reading string char"))?;
        let ch = mirror_bits(bits, 7) as u8;
        buf.push(ch);
    }
    Ok(String::from_utf8(buf)?)
}

fn write_string(writer: &mut BitWriter, value: &str) {
    write_varint(writer, value.len() as u32);
    for byte in value.as_bytes() {
        let mirrored = mirror_bits(*byte as u32, 7);
        writer.write_n(mirrored, 7);
    }
}

fn read_part(tokenizer: &mut Tokenizer) -> Result<Part> {
    let index = read_varint(&mut tokenizer.reader)?;
    let first_flag = tokenizer
        .reader
        .read_bit()
        .ok_or_else(|| anyhow!("unexpected end of data while reading part flag"))?;

    if first_flag == 1 {
        let value = read_varint(&mut tokenizer.reader)?;
        tokenizer.expect(&[0, 0, 0])?;
        return Ok(Part {
            index,
            subtype: PartSubType::Int,
            value,
            values: Vec::new(),
        });
    }

    let second_flag = tokenizer
        .reader
        .read_bits(2)
        .ok_or_else(|| anyhow!("unexpected end of data while reading part flag 2"))?;

    match second_flag {
        0b10 => Ok(Part {
            index,
            subtype: PartSubType::None,
            value: 0,
            values: Vec::new(),
        }),
        0b01 => {
            let token = tokenizer
                .next_token()?
                .ok_or_else(|| anyhow!("expected TOK_SEP2 at start of part list"))?;
            if token != TokenKind::Sep2 {
                bail!("expected TOK_SEP2 at start of part list, got {:?}", token);
            }

            let mut values = Vec::new();
            loop {
                match tokenizer.next_token()? {
                    Some(TokenKind::Sep1) => {
                        return Ok(Part {
                            index,
                            subtype: PartSubType::List,
                            value: 0,
                            values,
                        });
                    }
                    Some(TokenKind::VarInt) => {
                        let value = read_varint(&mut tokenizer.reader)?;
                        values.push(value);
                    }
                    Some(TokenKind::VarBit) => {
                        let value = read_varbit(&mut tokenizer.reader)?;
                        values.push(value);
                    }
                    Some(other) => {
                        bail!("unexpected token {:?} inside part list", other);
                    }
                    None => bail!("unexpected end of data inside part list"),
                }
            }
        }
        _ => bail!("unknown part subtype flag {:02b}", second_flag),
    }
}

fn write_part(writer: &mut BitWriter, part: &Part) {
    write_varint(writer, part.index);
    match part.subtype {
        PartSubType::None => {
            writer.write_bits(&[0, 1, 0]);
        }
        PartSubType::Int => {
            writer.write_bit(1);
            write_varint(writer, part.value);
            writer.write_bits(&[0, 0, 0]);
        }
        PartSubType::List => {
            writer.write_bits(&[0, 0, 1]);
            writer.write_bits(&[0, 1]);
            for value in &part.values {
                let mut tmp_writer = BitWriter::new();
                write_varint(&mut tmp_writer, *value);
                let varint_bits = tmp_writer.pos();

                let mut tmp_writer_varbit = BitWriter::new();
                write_varbit(&mut tmp_writer_varbit, *value);
                let varbit_bits = tmp_writer_varbit.pos();

                if varint_bits <= varbit_bits {
                    writer.write_bits(&[1, 0, 0]);
                    writer.write_bits(&tmp_writer.bit_vec());
                } else {
                    writer.write_bits(&[1, 1, 0]);
                    writer.write_bits(&tmp_writer_varbit.bit_vec());
                }
            }
            writer.write_bits(&[0, 0]);
        }
    }
}

pub fn deserialize(serial: &str) -> Result<(Vec<Token>, String)> {
    let bytes = decode_base85(serial)?;
    let mut tokenizer = Tokenizer::new(bytes);

    tokenizer.expect(&[0, 0, 1, 0, 0, 0, 0])?;

    let mut tokens = Vec::new();
    let mut trailing_terminators = 0_usize;

    loop {
        let kind = match tokenizer.next_token()? {
            Some(kind) => kind,
            None => break,
        };

        let token = match kind {
            TokenKind::Sep1 => {
                trailing_terminators += 1;
                Token::Sep1
            }
            TokenKind::Sep2 => {
                trailing_terminators = 0;
                Token::Sep2
            }
            TokenKind::VarInt => {
                trailing_terminators = 0;
                let value = read_varint(&mut tokenizer.reader)?;
                Token::VarInt(value)
            }
            TokenKind::VarBit => {
                trailing_terminators = 0;
                let value = read_varbit(&mut tokenizer.reader)?;
                Token::VarBit(value)
            }
            TokenKind::Part => {
                trailing_terminators = 0;
                let part = read_part(&mut tokenizer)?;
                Token::Part(part)
            }
            TokenKind::String => {
                trailing_terminators = 0;
                let value = read_string(&mut tokenizer.reader)?;
                Token::Str(value)
            }
        };

        tokens.push(token);
    }

    if trailing_terminators > 1 {
        let len = tokens.len().saturating_sub(trailing_terminators - 1);
        tokens.truncate(len);
    }

    let bits = tokenizer.reader.as_bit_string();

    Ok((tokens, bits))
}

pub fn serialize(tokens: &[Token]) -> Result<String> {
    let mut writer = BitWriter::new();
    writer.write_bits(&[0, 0, 1, 0, 0, 0, 0]);

    for token in tokens {
        match token {
            Token::Sep1 => writer.write_bits(&[0, 0]),
            Token::Sep2 => writer.write_bits(&[0, 1]),
            Token::VarInt(value) => {
                writer.write_bits(&[1, 0, 0]);
                write_varint(&mut writer, *value);
            }
            Token::VarBit(value) => {
                writer.write_bits(&[1, 1, 0]);
                write_varbit(&mut writer, *value);
            }
            Token::Part(part) => {
                writer.write_bits(&[1, 0, 1]);
                write_part(&mut writer, part);
            }
            Token::Str(value) => {
                writer.write_bits(&[1, 1, 1]);
                write_string(&mut writer, value);
            }
        }
    }

    let data = writer.into_vec();
    Ok(encode_base85(&data))
}

pub fn tokens_to_string(tokens: &[Token]) -> String {
    let mut output = String::new();
    for (idx, token) in tokens.iter().enumerate() {
        match token {
            Token::Sep1 => output.push('|'),
            Token::Sep2 => output.push(','),
            Token::VarInt(value) | Token::VarBit(value) => {
                if idx > 0 {
                    output.push(' ');
                }
                output.push_str(&value.to_string());
            }
            Token::Part(part) => {
                if idx > 0 {
                    output.push(' ');
                }
                output.push_str(&part.to_string());
            }
            Token::Str(value) => {
                if idx > 0 {
                    output.push(' ');
                }
                let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
                output.push('"');
                output.push_str(&escaped);
                output.push('"');
            }
        }
    }
    output
}
