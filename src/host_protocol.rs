//! 方向 1：主机 <-> CAN 桥接协议。
//!
//! 帧格式（小端）:
//!   HEADER0(1) HEADER1(1) ID(4) DLC(1) DATA(DLC) XOR(1)
//! 帧边界识别：0xAA55 帧头 + DLC 长度字段 + 逐字节状态机。
//! 校验：从 HEADER0 到最后一个 DATA 字节的 XOR。

pub const HEADER0: u8 = 0xAA;
pub const HEADER1: u8 = 0x55;
pub const MAX_PAYLOAD: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostFrame {
    pub id: u32,
    pub dlc: u8,
    pub data: [u8; MAX_PAYLOAD],
}

impl HostFrame {
    pub fn new(id: u32, data: &[u8]) -> Self {
        let mut buf = [0u8; MAX_PAYLOAD];
        let n = data.len().min(MAX_PAYLOAD);
        buf[..n].copy_from_slice(&data[..n]);
        Self {
            id,
            dlc: n as u8,
            data: buf,
        }
    }

    /// 编码为 UART 二进制帧，返回写入字节数（8 + dlc）。
    pub fn encode(&self, out: &mut [u8]) -> usize {
        assert!(out.len() >= 8 + self.dlc as usize);
        let n = self.dlc as usize;
        out[0] = HEADER0;
        out[1] = HEADER1;
        out[2..6].copy_from_slice(&self.id.to_le_bytes());
        out[6] = self.dlc;
        out[7..7 + n].copy_from_slice(&self.data[..n]);
        let mut xor = 0u8;
        for &b in &out[..7 + n] {
            xor ^= b;
        }
        out[7 + n] = xor;
        8 + n
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ParserState {
    WaitHead0,
    WaitHead1,
    Id,
    Dlc,
    Data,
    Checksum,
}

pub struct HostFrameParser {
    state: ParserState,
    buf: [u8; 16],
    pos: u8,
    xor: u8,
}

impl HostFrameParser {
    pub const fn new() -> Self {
        Self {
            state: ParserState::WaitHead0,
            buf: [0; 16],
            pos: 0,
            xor: 0,
        }
    }

    /// 喂入一个字节。返回 Some(frame) 表示收齐并校验通过一帧。
    pub fn feed(&mut self, byte: u8) -> Option<HostFrame> {
        match self.state {
            ParserState::WaitHead0 => {
                if byte == HEADER0 {
                    self.buf[0] = byte;
                    self.xor = byte;
                    self.pos = 1;
                    self.state = ParserState::WaitHead1;
                }
            }
            ParserState::WaitHead1 => {
                if byte == HEADER1 {
                    self.buf[1] = byte;
                    self.xor ^= byte;
                    self.pos = 2;
                    self.state = ParserState::Id;
                } else if byte == HEADER0 {
                    self.buf[0] = byte;
                    self.xor = byte;
                    self.pos = 1;
                } else {
                    self.reset();
                }
            }
            ParserState::Id => {
                self.buf[self.pos as usize] = byte;
                self.xor ^= byte;
                self.pos += 1;
                if self.pos == 6 {
                    self.state = ParserState::Dlc;
                }
            }
            ParserState::Dlc => {
                self.buf[6] = byte;
                self.xor ^= byte;
                self.pos = 7;
                if byte > MAX_PAYLOAD as u8 {
                    self.reset();
                } else if byte == 0 {
                    self.state = ParserState::Checksum;
                } else {
                    self.state = ParserState::Data;
                }
            }
            ParserState::Data => {
                self.buf[self.pos as usize] = byte;
                self.xor ^= byte;
                self.pos += 1;
                if self.pos as usize == 7 + self.buf[6] as usize {
                    self.state = ParserState::Checksum;
                }
            }
            ParserState::Checksum => {
                if byte == self.xor {
                    let n = self.buf[6] as usize;
                    let mut data = [0u8; MAX_PAYLOAD];
                    data[..n].copy_from_slice(&self.buf[7..7 + n]);
                    let frame = HostFrame {
                        id: u32::from_le_bytes([
                            self.buf[2],
                            self.buf[3],
                            self.buf[4],
                            self.buf[5],
                        ]),
                        dlc: self.buf[6],
                        data,
                    };
                    self.reset();
                    return Some(frame);
                } else {
                    self.reset();
                }
            }
        }
        None
    }

    fn reset(&mut self) {
        self.state = ParserState::WaitHead0;
        self.pos = 0;
        self.xor = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_layout() {
        let f = HostFrame::new(0x123, &[0xAA, 0xBB]);
        let mut buf = [0u8; 16];
        let n = f.encode(&mut buf);
        assert_eq!(n, 10);
        assert_eq!(buf[0], 0xAA);
        assert_eq!(buf[1], 0x55);
        assert_eq!(buf[2..6], 0x123u32.to_le_bytes());
        assert_eq!(buf[6], 2);
        assert_eq!(buf[7], 0xAA);
        assert_eq!(buf[8], 0xBB);
        let mut xor = 0u8;
        for &b in &buf[..9] {
            xor ^= b;
        }
        assert_eq!(buf[9], xor);
    }

    #[test]
    fn parser_byte_by_byte() {
        let f = HostFrame::new(0x123, &[0xAA, 0xBB]);
        let mut buf = [0u8; 16];
        let n = f.encode(&mut buf);
        let mut p = HostFrameParser::new();
        let mut got = None;
        for &b in &buf[..n] {
            if let Some(g) = p.feed(b) {
                got = Some(g);
            }
        }
        let g = got.unwrap();
        assert_eq!(g.id, 0x123);
        assert_eq!(g.dlc, 2);
        assert_eq!(&g.data[..2], &[0xAA, 0xBB]);
    }

    #[test]
    fn parser_two_frames_back_to_back() {
        let f1 = HostFrame::new(0x100, &[1, 2, 3]);
        let f2 = HostFrame::new(0x200, &[4, 5]);
        let mut b1 = [0u8; 16];
        let mut b2 = [0u8; 16];
        let n1 = f1.encode(&mut b1);
        let n2 = f2.encode(&mut b2);
        let mut p = HostFrameParser::new();
        let mut frames = [None, None];
        let mut count = 0;
        for &b in b1[..n1].iter().chain(b2[..n2].iter()) {
            if let Some(g) = p.feed(b) {
                frames[count] = Some(g);
                count += 1;
            }
        }
        assert_eq!(count, 2);
        assert_eq!(frames[0].unwrap().id, 0x100);
        assert_eq!(frames[1].unwrap().id, 0x200);
    }

    #[test]
    fn parser_bad_checksum_rejected() {
        let f = HostFrame::new(0x10, &[1]);
        let mut buf = [0u8; 16];
        let n = f.encode(&mut buf);
        buf[n - 1] ^= 0xFF;
        let mut p = HostFrameParser::new();
        for &b in &buf[..n] {
            assert!(p.feed(b).is_none());
        }
    }

    #[test]
    fn parser_zero_dlc() {
        let f = HostFrame::new(0x7FF, &[]);
        let mut buf = [0u8; 16];
        let n = f.encode(&mut buf);
        assert_eq!(n, 8);
        let mut p = HostFrameParser::new();
        let mut got = None;
        for &b in &buf[..n] {
            if let Some(g) = p.feed(b) {
                got = Some(g);
            }
        }
        let g = got.unwrap();
        assert_eq!(g.id, 0x7FF);
        assert_eq!(g.dlc, 0);
    }
}
