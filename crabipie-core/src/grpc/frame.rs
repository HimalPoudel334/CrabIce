//! gRPC message framing: 1 flag byte (0 = plain, 1 = compressed) + 4-byte
//! big-endian length + payload. Messages may be split across or packed into
//! HTTP/2 DATA chunks, so reading goes through a `Deframer`.

use std::io::Read;

/// Refuse absurd messages instead of allocating for them.
pub const MAX_MESSAGE: usize = 64 * 1024 * 1024;

pub fn frame(msg: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(msg.len() + 5);
    out.push(0);
    out.extend_from_slice(&(msg.len() as u32).to_be_bytes());
    out.extend_from_slice(msg);
    out
}

#[derive(Default)]
pub struct Deframer {
    buf: Vec<u8>,
}

impl Deframer {
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// The next complete message as (compressed flag, payload), if one has arrived.
    pub fn pop(&mut self) -> Result<Option<(bool, Vec<u8>)>, String> {
        if self.buf.len() < 5 {
            return Ok(None);
        }
        let flag = self.buf[0];
        if flag > 1 {
            return Err(format!("bad gRPC frame flag {flag}"));
        }
        let len = u32::from_be_bytes([self.buf[1], self.buf[2], self.buf[3], self.buf[4]]) as usize;
        if len > MAX_MESSAGE {
            return Err(format!("gRPC message of {len} bytes exceeds the {MAX_MESSAGE} byte limit"));
        }
        if self.buf.len() < 5 + len {
            return Ok(None);
        }
        let payload = self.buf[5..5 + len].to_vec();
        self.buf.drain(..5 + len);
        Ok(Some((flag == 1, payload)))
    }

    /// True while a message is half-received (the stream ended too early if so).
    pub fn has_partial(&self) -> bool {
        !self.buf.is_empty()
    }
}

/// Decompress a gzip payload (`grpc-encoding: gzip`).
pub fn gunzip(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(data).read_to_end(&mut out).map_err(|e| format!("gzip: {e}"))?;
    Ok(out)
}

pub fn status_name(code: i32) -> &'static str {
    match code {
        0 => "OK",
        1 => "CANCELLED",
        2 => "UNKNOWN",
        3 => "INVALID_ARGUMENT",
        4 => "DEADLINE_EXCEEDED",
        5 => "NOT_FOUND",
        6 => "ALREADY_EXISTS",
        7 => "PERMISSION_DENIED",
        8 => "RESOURCE_EXHAUSTED",
        9 => "FAILED_PRECONDITION",
        10 => "ABORTED",
        11 => "OUT_OF_RANGE",
        12 => "UNIMPLEMENTED",
        13 => "INTERNAL",
        14 => "UNAVAILABLE",
        15 => "DATA_LOSS",
        16 => "UNAUTHENTICATED",
        _ => "UNKNOWN_STATUS",
    }
}
