use core::sync::atomic::{self, AtomicU16};
use defmt::debug;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

use embassy_stm32::{
    gpio::OutputType,
    peripherals,
    time::hz,
    timer::{
        simple_pwm::{PwmPin, SimplePwm},
        Channel,
    },
    Peri,
};
use embassy_time::Timer;
use heapless::Vec;

use crate::parser::{create_msg, ErrorCodes, FunctionCodes, Response, OUTBOUND};

enum ServoState {
    Idle,
    Stepping,
    Done,
}

struct Servo {
    channel: Channel,
    current: AtomicU16,
    target: AtomicU16,
    step: AtomicU16,
    txn: AtomicU16,
}

impl Servo {
    const fn new(ch: Channel) -> Self {
        Self {
            channel: ch,
            current: atomic::AtomicU16::new(0),
            target: atomic::AtomicU16::new(0),
            step: atomic::AtomicU16::new(0),
            txn: atomic::AtomicU16::new(0),
        }
    }
}

const NUM_SERVOS: usize = 4;
static SERVOS: [Servo; NUM_SERVOS] = [
    const { Servo::new(Channel::Ch1) },
    const { Servo::new(Channel::Ch2) },
    const { Servo::new(Channel::Ch3) },
    const { Servo::new(Channel::Ch4) },
];
const DUTY_DENOM: u32 = 20000;

//NOTE: 3 u16 for current, target, step
const SERVO_INFO_LEN: usize = 6;
//NOTE: 1 Extra byte for the mask
const SERVO_VEC_LEN: usize = (SERVO_INFO_LEN * NUM_SERVOS) + 1;
const VALID_MASK: u8 = (1 << NUM_SERVOS) - 1;

static SERVO_SIG: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[embassy_executor::task]
pub async fn pwm_task(
    timer: Peri<'static, peripherals::TIM4>,
    ch1pin: Peri<'static, peripherals::PB6>,
    ch2pin: Peri<'static, peripherals::PB7>,
    ch3pin: Peri<'static, peripherals::PB8>,
    ch4pin: Peri<'static, peripherals::PB9>,
) -> ! {
    let mut pwm = SimplePwm::new(
        timer,
        Some(PwmPin::new(ch1pin, OutputType::PushPull)),
        Some(PwmPin::new(ch2pin, OutputType::PushPull)),
        Some(PwmPin::new(ch3pin, OutputType::PushPull)),
        Some(PwmPin::new(ch4pin, OutputType::PushPull)),
        hz(50),
        Default::default(),
    );

    for servo in &SERVOS {
        pwm.channel(servo.channel).enable();
    }

    loop {
        let mut num_stepping: u8 = 0;
        for servo in &SERVOS {
            match step(&servo) {
                ServoState::Idle => {}
                ServoState::Stepping => {
                    pwm.channel(servo.channel).set_duty_cycle_fraction(
                        servo.current.load(atomic::Ordering::Relaxed) as u32,
                        DUTY_DENOM,
                    );
                    num_stepping += 1;
                }
                ServoState::Done => {
                    pwm.channel(servo.channel).set_duty_cycle_fraction(
                        servo.current.load(atomic::Ordering::Relaxed) as u32,
                        DUTY_DENOM,
                    );
                    let msg = create_msg(
                        servo.txn.load(atomic::Ordering::Relaxed),
                        FunctionCodes::SetServo.into(),
                        Some(&servo.current.load(atomic::Ordering::Relaxed).to_le_bytes()),
                    );
                    if let Ok(frame) = msg {
                        let _ = OUTBOUND.try_send(frame);
                    }
                }
            }
        }
        if num_stepping > 0 {
            Timer::after_millis(20).await;
        } else {
            SERVO_SIG.wait().await;
        }
    }
}

fn step(servo: &Servo) -> ServoState {
    let current = servo.current.load(atomic::Ordering::Relaxed);
    let target = servo.target.load(atomic::Ordering::Relaxed);
    if current == target {
        return ServoState::Idle;
    }
    let step = servo.step.load(atomic::Ordering::Relaxed);

    let next = if step == 0 {
        target
    } else if target > current {
        current.saturating_add(step).min(target)
    } else {
        current.saturating_sub(step).max(target)
    };

    servo.current.store(next, atomic::Ordering::Relaxed);
    if next == target {
        ServoState::Done
    } else {
        ServoState::Stepping
    }
}

fn set_servo(servo: &Servo, txn: u16, target: u16, step: Option<u16>) {
    servo.txn.store(txn, atomic::Ordering::Relaxed);
    servo.target.store(target, atomic::Ordering::Relaxed);
    servo
        .step
        .store(step.unwrap_or(0), atomic::Ordering::Relaxed);
    SERVO_SIG.signal(());
}

const SET_SERVO_LEN: [u8; 2] = [3, 5];
const NUM_OFFSET: usize = 0;
const TARGET_OFFSET: usize = 1;
const STEP_OFFSET: usize = 3;

struct ServoPayload {
    num: u8,
    target: u16,
    step_size: u16,
}

pub fn handle_set_servo(txn: u16, payload: &[u8]) -> Result<Response, ErrorCodes> {
    let len = payload.len() as u8;
    if !SET_SERVO_LEN.contains(&len) {
        return Err(ErrorCodes::InvalidPayloadLen);
    }
    let servo_payload = ServoPayload {
        num: payload[NUM_OFFSET],
        target: u16::from_le_bytes([payload[TARGET_OFFSET], payload[TARGET_OFFSET + 1]]),
        step_size: if len == 5 {
            u16::from_le_bytes([payload[STEP_OFFSET], payload[STEP_OFFSET + 1]])
        } else {
            0
        },
    };

    //TODO: Validate number, target and size.
    //This is temp for now
    if servo_payload.num as usize >= NUM_SERVOS {
        return Err(ErrorCodes::InvalidServoID);
    }
    if servo_payload.target as u32 >= DUTY_DENOM {
        return Err(ErrorCodes::InvalidServoDuty);
    }

    debug!("Servo {} at {}", servo_payload.num, servo_payload.target);

    set_servo(
        &SERVOS[servo_payload.num as usize],
        txn,
        servo_payload.target,
        Some(servo_payload.step_size),
    );
    Ok(Response::SendAck)
}

pub async fn get_servo(txn: u16, payload: &[u8]) -> Result<Response, ErrorCodes> {
    let len = payload.len() as u8;
    if len != 1 {
        return Err(ErrorCodes::InvalidPayloadLen);
    }

    let mask = payload[0];
    if mask & !VALID_MASK != 0 {
        return Err(ErrorCodes::InvalidServoID);
    }

    if mask == 0 {
        return Err(ErrorCodes::InvalidServoID);
    }

    let mut buff: Vec<u8, SERVO_VEC_LEN> = Vec::new();
    defmt::unwrap!(buff.push(mask));
    get_servo_info(mask, &mut buff);
    Ok(send_servo_info(txn, FunctionCodes::GetServo, &buff).await)
}

pub async fn get_servo_all(txn: u16) -> Result<Response, ErrorCodes> {
    let mut buff: Vec<u8, SERVO_VEC_LEN> = Vec::new();
    get_servo_info(VALID_MASK, &mut buff);
    Ok(send_servo_info(txn, FunctionCodes::GetServoAll, &buff).await)
}

fn get_servo_info(mut mask: u8, vec: &mut Vec<u8, SERVO_VEC_LEN>) {
    while mask != 0 {
        let index = mask.trailing_zeros() as usize;

        defmt::unwrap!(vec.extend_from_slice(
            &SERVOS[index]
                .current
                .load(atomic::Ordering::Relaxed)
                .to_le_bytes()
        ));
        defmt::unwrap!(vec.extend_from_slice(
            &SERVOS[index]
                .target
                .load(atomic::Ordering::Relaxed)
                .to_le_bytes()
        ));
        defmt::unwrap!(vec.extend_from_slice(
            &SERVOS[index]
                .step
                .load(atomic::Ordering::Relaxed)
                .to_le_bytes()
        ));

        mask &= !(1 << index);
    }
}

async fn send_servo_info(txn: u16, func: FunctionCodes, buff: &[u8]) -> Response {
    let frame = defmt::unwrap!(create_msg(txn, func.into(), Some(buff)));
    OUTBOUND.send(frame).await;
    Response::NoAck
}
