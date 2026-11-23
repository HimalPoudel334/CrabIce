//! gRPC support without generated code: a hand-written protobuf layer
//! (wire format, descriptors, JSON <-> bytes) plus gRPC framing. Kept small
//! on purpose; the final exe has a 2-3 MB budget.

pub mod client;
pub mod desc;
pub mod dynmsg;
pub mod frame;
pub mod wire;
