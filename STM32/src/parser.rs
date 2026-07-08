use crate::transport;
use crc16::*;
use defmt::{info, warn};
use embassy_futures::join::join;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::Timer;
use embedded_io_async::{Read, Write};
use heapless::Vec;

enum ReadState {
    Seeking,
    Parsing,
}

enum SeekResult {
    Found,
    NotFound,
}

enum ParseResult {
    Ok,
    InvalidCrc,
    Incomplete,
}

pub enum FunctionCodes {
    Ack = 0x06,
    Nack = 0x15,
    SetServo = 0x20,
}

impl FunctionCodes {
    fn from_u8(b: u8) -> Option<FunctionCodes> {
        match b {
            0x06 => Some(FunctionCodes::Ack),
            0x15 => Some(FunctionCodes::Nack),
            0x20 => Some(FunctionCodes::SetServo),
            _ => None,
        }
    }
}

pub enum ErrorCodes {
    InvalidFunc,
    InvalidPayload,
}

impl ErrorCodes {
    fn as_payload(&self) -> &'static [u8] {
        match self {
            ErrorCodes::InvalidFunc => &[0x05],
            ErrorCodes::InvalidPayload => &[0x06],
        }
    }
}

const SYNC: u8 = 0xAB;
const OFFSET_TXN: usize = 1;
const OFFSET_FUNC: usize = 3;
const OFFSET_LEN: usize = 4;
const OFFSET_PAYLOAD: usize = 5;
const HEADER_LEN: usize = 5;
const CRC_LEN: usize = 2;
const OVERHEAD: usize = HEADER_LEN + CRC_LEN;
const MAX_MSG_SIZE: usize = u8::MAX as usize + OVERHEAD;

static OUTBOUND: Channel<CriticalSectionRawMutex, Vec<u8, MAX_MSG_SIZE>, 8> = Channel::new();

struct Parser {
    state: ReadState,
    buff: [u8; MAX_MSG_SIZE * 2],
    buff_len: usize,
}

impl Parser {
    fn new() -> Self {
        Self {
            state: ReadState::Seeking,
            buff: [0; MAX_MSG_SIZE * 2],
            buff_len: 0,
        }
    }
}

struct MsgInfo {
    txn: u16,
    func: u8,
    len: u8,
}

impl MsgInfo {
    fn new() -> Self {
        Self {
            txn: 0,
            func: 0,
            len: 0,
        }
    }
}

#[embassy_executor::task]
pub async fn communication_task(rx: transport::Reader, tx: transport::Writer) -> ! {
    let out = write_outgoing(tx);
    let inc = read_incoming(rx);
    join(out, inc).await;
    unreachable!()
}

async fn write_outgoing<W: Write>(mut tx: W) -> ! {
    loop {
        let frame = OUTBOUND.receive().await;
        if let Err(e) = tx.write_all(&frame).await {
            warn!("Write error: {:?}", defmt::Debug2Format(&e));
        }
    }
}

pub fn create_msg(txn: u16, func: FunctionCodes, payload: Option<&[u8]>) -> Vec<u8, MAX_MSG_SIZE> {
    let payload = payload.unwrap_or(&[]);
    let mut resp = Vec::new();
    resp.push(SYNC).unwrap();
    resp.extend_from_slice(&txn.to_le_bytes()).unwrap();
    resp.push(func as u8).unwrap();
    resp.push(payload.len() as u8).unwrap();
    resp.extend_from_slice(payload).unwrap();
    let crc16 = calculate_crc(&resp);
    resp.extend_from_slice(&crc16.to_le_bytes()).unwrap();
    resp
}

async fn read_incoming<R: Read>(mut rx: R) -> ! {
    let mut parser: Parser = Parser::new();
    loop {
        match rx.read(&mut parser.buff[parser.buff_len..]).await {
            Ok(n) if n > 0 => parser.buff_len += n,
            Ok(_) => {}
            Err(e) => {
                warn!("Read error: {:?}", defmt::Debug2Format(&e));
                Timer::after_millis(100).await;
            }
        }
        loop {
            match parser.state {
                ReadState::Seeking => match seek(&mut parser) {
                    SeekResult::Found => {}
                    SeekResult::NotFound => {
                        //NOTE: Should be more efficient than to memset, since we keep track
                        //of this always
                        parser.buff_len = 0;
                        break;
                    }
                },
                ReadState::Parsing => match parse(&mut parser) {
                    (ParseResult::Ok, msg) => {
                        let (code, payload) = match FunctionCodes::from_u8(msg.func) {
                            Some(FunctionCodes::Ack) => (None, None),
                            Some(FunctionCodes::Nack) => (None, None),
                            Some(_) => (Some(FunctionCodes::Ack), None),
                            None => (
                                Some(FunctionCodes::Nack),
                                Some(ErrorCodes::InvalidFunc.as_payload()),
                            ),
                        };

                        if let Some(code) = code {
                            let _ = OUTBOUND.send(create_msg(msg.txn, code, payload)).await;
                        }

                        let total_msg_size: usize = msg.len as usize + OVERHEAD;

                        parser.buff.copy_within(total_msg_size..parser.buff_len, 0);
                        parser.state = ReadState::Seeking;
                        parser.buff_len -= total_msg_size;
                    }
                    (ParseResult::InvalidCrc, _) => {
                        parser.buff.copy_within(1..parser.buff_len, 0);
                        parser.buff_len -= 1;
                        parser.state = ReadState::Seeking
                    }
                    (ParseResult::Incomplete, _) => break,
                },
            }
        }
    }
}

fn seek(parser: &mut Parser) -> SeekResult {
    for i in 0..parser.buff_len {
        if parser.buff[i] == SYNC {
            parser.state = ReadState::Parsing;
            parser.buff.copy_within(i..parser.buff_len, 0);
            parser.buff_len -= i;
            return SeekResult::Found;
        }
    }
    return SeekResult::NotFound;
}

//TODO: instead of usize return a struct with the relevant values for queueing (txn, func, payload)
fn parse(parser: &mut Parser) -> (ParseResult, MsgInfo) {
    let mut msg = MsgInfo::new();
    if parser.buff_len < HEADER_LEN {
        return (ParseResult::Incomplete, msg);
    }
    msg.txn = u16::from_le_bytes([parser.buff[OFFSET_TXN], parser.buff[OFFSET_TXN + 1]]);
    msg.func = parser.buff[OFFSET_FUNC];
    msg.len = parser.buff[OFFSET_LEN];
    let len_check: usize = msg.len as usize + OVERHEAD;
    if parser.buff_len < len_check {
        return (ParseResult::Incomplete, msg);
    }

    let crc = u16::from_le_bytes([parser.buff[len_check - CRC_LEN], parser.buff[len_check - 1]]);

    match verify_crc(parser, 0, len_check - CRC_LEN, crc) {
        true => (ParseResult::Ok, msg),
        false => (ParseResult::InvalidCrc, msg),
    }
}

fn verify_crc(parser: &Parser, start: usize, end: usize, crc: u16) -> bool {
    State::<MODBUS>::calculate(&parser.buff[start..end]) == crc
}

fn calculate_crc(data: &[u8]) -> u16 {
    State::<MODBUS>::calculate(&data)
}
