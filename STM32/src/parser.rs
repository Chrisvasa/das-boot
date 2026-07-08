use crate::transport;
use crc16::*;
use defmt::{info, warn};
use embassy_time::Timer;
use embedded_io_async::{Read, Write};

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

const SYNC: u8 = 0xAB;
const OFFSET_TXN: usize = 1;
const OFFSET_FUNC: usize = 3;
const OFFSET_LEN: usize = 4;
const OFFSET_PAYLOAD: usize = 5;
const HEADER_LEN: usize = 5;
const CRC_LEN: usize = 2;
const OVERHEAD: usize = HEADER_LEN + CRC_LEN;
const MAX_MSG_SIZE: usize = u8::MAX as usize + OVERHEAD;

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

#[embassy_executor::task]
pub async fn communication_task(rx: transport::Reader, tx: transport::Writer) -> ! {
    read_incoming(rx, tx).await
}

async fn read_incoming<R: Read, W: Write>(mut rx: R, mut tx: W) -> ! {
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
                    (ParseResult::Ok, n) => {
                        info!("Ok msg!");
                        parser.buff.copy_within(n..parser.buff_len, 0);
                        parser.state = ReadState::Seeking;
                        parser.buff_len -= n;
                    }
                    (ParseResult::InvalidCrc, _) => {
                        warn!("Invalid crc");
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
fn parse(parser: &mut Parser) -> (ParseResult, usize) {
    if parser.buff_len < HEADER_LEN {
        return (ParseResult::Incomplete, 0);
    }
    let txn: u16 = u16::from_le_bytes([parser.buff[OFFSET_TXN], parser.buff[OFFSET_TXN + 1]]);
    let func_code: u8 = parser.buff[OFFSET_FUNC];
    let length: u8 = parser.buff[OFFSET_LEN];
    let len_check: usize = length as usize + OVERHEAD;
    if parser.buff_len < len_check {
        return (ParseResult::Incomplete, 0);
    }

    let crc = u16::from_le_bytes([parser.buff[len_check - CRC_LEN], parser.buff[len_check - 1]]);

    match calc_crc(parser, 0, len_check - CRC_LEN, crc) {
        true => (ParseResult::Ok, len_check),
        false => (ParseResult::InvalidCrc, 0),
    }
}

fn calc_crc(parser: &Parser, start: usize, end: usize, crc: u16) -> bool {
    State::<MODBUS>::calculate(&parser.buff[start..end]) == crc
}
