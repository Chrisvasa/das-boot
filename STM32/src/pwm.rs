use core::sync::atomic::{self, AtomicU16};

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

use crate::parser::{create_msg, ErrorCodes, FunctionCodes, OUTBOUND};

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

#[embassy_executor::task]
pub async fn pwm_task(
    timer: Peri<'static, peripherals::TIM3>,
    pin: Peri<'static, peripherals::PB0>,
) -> ! {
    let pwm_pin = PwmPin::new(pin, OutputType::PushPull);
    let mut pwm = SimplePwm::new(
        timer,
        None,
        None,
        Some(pwm_pin),
        None,
        hz(50),
        Default::default(),
    );

    //NOTE: Only enabled CH3 for now since thats the only thing having something connected
    pwm.channel(Channel::Ch3).enable();

    loop {
        for servo in &SERVOS {
            match step(&servo) {
                ServoState::Idle => {}
                ServoState::Stepping => {
                    pwm.channel(servo.channel).set_duty_cycle_fraction(
                        servo.current.load(atomic::Ordering::Relaxed) as u32,
                        DUTY_DENOM,
                    );
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
        Timer::after_millis(20).await;
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

pub fn handle_set_servo(txn: u16, payload: &[u8]) -> Result<(), ErrorCodes> {
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

    set_servo(
        &SERVOS[servo_payload.num as usize],
        txn,
        servo_payload.target,
        Some(servo_payload.step_size),
    );

    Ok(())
}
