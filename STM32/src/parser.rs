use crate::{
    servo_handler::{get_servo, handle_set_servo},
    transport,
};
use crc16::*;
use defmt::warn;
use embassy_futures::join::join;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::Timer;
use embedded_io_async::{Read, Write};
use heapless::Vec;

enum ReadState {
    Seeking,
    Parsing,
}

enum ParseResult {
    Ok(MsgInfo),
    InvalidCrc,
    Incomplete,
}

pub enum FrameError {
    TooLong,
}

#[derive(num_enum::TryFromPrimitive, num_enum::IntoPrimitive)]
#[repr(u8)]
pub enum ControlCodes {
    Ack = 0x06,
    Nack = 0x10,
}

#[derive(num_enum::TryFromPrimitive, num_enum::IntoPrimitive)]
#[repr(u8)]
pub enum FunctionCodes {
    Ping = 0x15,
    SetServo = 0x20,
    GetServo = 0x21, // payload: mask for which servos to get info from
    GetServoAll = 0x22,
}

#[allow(dead_code)]
#[repr(u8)]
pub enum ErrorCodes {
    InvalidFunc = 0x05,
    InvalidPayload = 0x06,
    InvalidPayloadLen = 0x07,
    InvalidServoID = 0x08,
    InvalidServoDuty = 0x09,
    VectorError = 0x10,
}

#[repr(u8)]
pub enum Response {
    SendAck,
    NoAck,
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

pub static OUTBOUND: Channel<CriticalSectionRawMutex, Vec<u8, MAX_MSG_SIZE>, 8> = Channel::new();

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

pub fn create_msg(
    txn: u16,
    func: u8,
    payload: Option<&[u8]>,
) -> Result<Vec<u8, MAX_MSG_SIZE>, FrameError> {
    let payload = payload.unwrap_or(&[]);
    if payload.len() > u8::MAX as usize {
        return Err(FrameError::TooLong);
    }
    let mut msg = Vec::new();
    defmt::unwrap!(msg.push(SYNC));
    defmt::unwrap!(msg.extend_from_slice(&txn.to_le_bytes()));
    defmt::unwrap!(msg.push(func));
    defmt::unwrap!(msg.push(payload.len() as u8));
    defmt::unwrap!(msg.extend_from_slice(payload));
    let crc16 = calculate_crc(&msg);
    defmt::unwrap!(msg.extend_from_slice(&crc16.to_le_bytes()));
    Ok(msg)
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
        'parseloop: loop {
            match parser.state {
                ReadState::Seeking => {
                    if !seek(&mut parser) {
                        parser.buff_len = 0;
                        break 'parseloop;
                    }
                }
                ReadState::Parsing => match parse(&mut parser) {
                    ParseResult::Ok(msg) => {
                        let payload =
                            &parser.buff[OFFSET_PAYLOAD..OFFSET_PAYLOAD + msg.len as usize];
                        validate_dispatch(&msg, payload).await;
                        let total_msg_size: usize = msg.len as usize + OVERHEAD;
                        parser.buff.copy_within(total_msg_size..parser.buff_len, 0);
                        parser.state = ReadState::Seeking;
                        parser.buff_len -= total_msg_size;
                    }
                    ParseResult::InvalidCrc => {
                        parser.buff.copy_within(1..parser.buff_len, 0);
                        parser.buff_len -= 1;
                        parser.state = ReadState::Seeking
                    }
                    ParseResult::Incomplete => break 'parseloop,
                },
            }
        }
    }
}

async fn validate_dispatch(msg: &MsgInfo, payload: &[u8]) {
    //NOTE: Dont respond to control codes
    if ControlCodes::try_from(msg.func).is_ok() {
        return;
    }

    let result = match FunctionCodes::try_from(msg.func) {
        Ok(FunctionCodes::Ping) => Ok(Response::SendAck),
        Ok(FunctionCodes::SetServo) => handle_set_servo(msg.txn, payload),
        Ok(FunctionCodes::GetServo) => get_servo(msg.txn, payload).await,
        Ok(FunctionCodes::GetServoAll) => Ok(Response::NoAck),
        Err(_) => Err(ErrorCodes::InvalidFunc),
    };

    if let Ok(Response::NoAck) = result {
        return;
    }

    let reply = match result {
        Ok(_) => create_msg(msg.txn, ControlCodes::Ack.into(), None),
        Err(reason) => create_msg(msg.txn, ControlCodes::Nack.into(), Some(&[reason as u8])),
    };

    if let Ok(frame) = reply {
        OUTBOUND.send(frame).await;
    }
}

fn seek(parser: &mut Parser) -> bool {
    for i in 0..parser.buff_len {
        if parser.buff[i] == SYNC {
            parser.state = ReadState::Parsing;
            parser.buff.copy_within(i..parser.buff_len, 0);
            parser.buff_len -= i;
            return true;
        }
    }
    false
}

fn parse(parser: &mut Parser) -> ParseResult {
    if parser.buff_len < HEADER_LEN {
        return ParseResult::Incomplete;
    }
    let len_check: usize = parser.buff[OFFSET_LEN] as usize + OVERHEAD;
    if parser.buff_len < len_check {
        return ParseResult::Incomplete;
    }

    let msg = MsgInfo {
        txn: u16::from_le_bytes([parser.buff[OFFSET_TXN], parser.buff[OFFSET_TXN + 1]]),
        func: parser.buff[OFFSET_FUNC],
        len: parser.buff[OFFSET_LEN],
    };

    let crc = u16::from_le_bytes([parser.buff[len_check - CRC_LEN], parser.buff[len_check - 1]]);

    if calculate_crc(&parser.buff[..len_check - CRC_LEN]) == crc {
        return ParseResult::Ok(msg);
    }
    ParseResult::InvalidCrc
}

fn calculate_crc(data: &[u8]) -> u16 {
    State::<MODBUS>::calculate(data)
}
