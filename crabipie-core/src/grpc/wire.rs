//! Protobuf wire format: just enough to read descriptors and to encode and
//! decode dynamic messages. No generated code, no prost.

pub const VARINT: u8 = 0;
pub const FIXED64: u8 = 1;
pub const LEN: u8 = 2;
pub const FIXED32: u8 = 5;

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }

    pub fn varint(&mut self) -> Result<u64, String> {
        let mut v: u64 = 0;
        let mut shift = 0u32;
        loop {
            let b = *self.buf.get(self.pos).ok_or("truncated varint")?;
            self.pos += 1;
            if shift >= 64 {
                return Err("varint too long".into());
            }
            v |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
        }
    }

    /// Next field header as (field number, wire type); None at the end.
    pub fn tag(&mut self) -> Result<Option<(u32, u8)>, String> {
        if self.is_empty() {
            return Ok(None);
        }
        let t = self.varint()?;
        let field = (t >> 3) as u32;
        if field == 0 {
            return Err("invalid field number 0".into());
        }
        Ok(Some((field, (t & 7) as u8)))
    }

    pub fn fixed32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn fixed64(&mut self) -> Result<u64, String> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    /// A length-delimited payload (string, bytes, sub-message, packed run).
    pub fn bytes(&mut self) -> Result<&'a [u8], String> {
        let n = self.varint()?;
        let n = usize::try_from(n).map_err(|_| "length overflow".to_string())?;
        self.take(n)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|e| *e <= self.buf.len())
            .ok_or("truncated field")?;
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    pub fn skip(&mut self, wire: u8) -> Result<(), String> {
        match wire {
            VARINT => {
                self.varint()?;
            }
            FIXED64 => {
                self.take(8)?;
            }
            LEN => {
                self.bytes()?;
            }
            FIXED32 => {
                self.take(4)?;
            }
            w => return Err(format!("unsupported wire type {w}")),
        }
        Ok(())
    }
}

pub fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(((v & 0x7f) as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

pub fn put_tag(out: &mut Vec<u8>, field: u32, wire: u8) {
    put_varint(out, ((field as u64) << 3) | wire as u64);
}

/// Tag + length + payload.
pub fn put_len(out: &mut Vec<u8>, field: u32, data: &[u8]) {
    put_tag(out, field, LEN);
    put_varint(out, data.len() as u64);
    out.extend_from_slice(data);
}

pub fn put_fixed32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub fn put_fixed64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}
