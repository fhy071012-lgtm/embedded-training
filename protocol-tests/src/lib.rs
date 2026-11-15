//! 纯 host 侧的协议测试。
//!
//! 通过 `include!` 直接复用固件里的 `src/host_protocol.rs`，
//! 源码只有一份，固件和测试始终一致。
//!
//! 协议模块不依赖任何外部 crate（只用 core 和 `#[test]`），
//! 所以可以在 host 上正常编译。

#![allow(dead_code)]

// 相对路径：protocol-tests/src/lib.rs → ../../src/host_protocol.rs
include!("../../src/host_protocol.rs");
