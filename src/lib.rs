#![no_std]
#![no_main]

use core::{cell::OnceCell, mem::MaybeUninit};

use bagua::BaGua;
use dice::Dice;
use face::Face;
use fastrand::Rng;
use hal::i2c::I2C;
use ledc::LedControl;
use mpu6050_dmp::{
    accel::{AccelF32, AccelFullScale},
    sensor::Mpu6050,
};
use snake::SnakeGame;
use timer::Timer;
use ui::Ui;

extern crate alloc;

mod bagua;
mod battery;
mod dice;
mod face;
pub mod ledc;
mod mapping;
mod maze;
mod snake;
mod timer;
mod ui;

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

// static mut RAND: Rng = Rng::with_seed(1);
static mut RAND: OnceCell<Rng> = OnceCell::new();

pub fn init() {
    let once = OnceCell::new();
    once.get_or_init(|| Rng::with_seed(1));

    //     const HEAP_SIZE: usize = 32 * 1024;
    //     static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();
    //
    //     unsafe {
    //         ALLOCATOR.init(HEAP.as_mut_ptr() as *mut u8, HEAP_SIZE);
    //     }
}

/// 左上角为坐标原点,横x,纵y
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Position {
    x: i8,
    y: i8,
}

impl Position {
    fn new(x: i8, y: i8) -> Self {
        Position { x, y }
    }

    fn r#move(&mut self, d: Direction) {
        match d {
            Direction::Up => {
                self.y -= 1;
            }
            Direction::Right => self.x += 1,
            Direction::Down => self.y += 1,
            Direction::Left => self.x -= 1,
        }
    }

    fn next(&self, d: Direction) -> Self {
        let mut pos = *self;

        match d {
            Direction::Up => {
                pos.y -= 1;
            }
            Direction::Right => pos.x += 1,
            Direction::Down => pos.y += 1,
            Direction::Left => pos.x -= 1,
        }
        pos
    }

    fn random(x: i8, y: i8) -> Self {
        unsafe {
            Self {
                x: RAND.get_mut().unwrap().i8(0..x),
                y: RAND.get_mut().unwrap().i8(0..y),
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Up,
    Right,
    Down,
    Left,
}

impl Direction {
    fn opposite(&self) -> Self {
        match self {
            Direction::Up => Self::Down,
            Direction::Right => Self::Left,
            Direction::Down => Self::Up,
            Direction::Left => Self::Right,
        }
    }
}

/// 重力方向
#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum Gd {
    #[default]
    None,
    Up,
    Right,
    Down,
    Left,
}

/// 小方
pub struct App<'d, T>
where
    T: hal::i2c::Instance,
{
    /// 蜂鸣器开关
    buzzer: bool,
    /// 界面
    uis: [Ui; 7],
    /// 当前界面的索引
    ui_current_idx: i8,
    /// 表情
    face: Face,
    gd: Gd,

    mpu6050: Mpu6050<I2C<'d, T>>,
    ledc: LedControl<'d>,
}

impl<'d, T> App<'d, T>
where
    T: hal::i2c::Instance,
{
    fn gravity_direction(&mut self) {
        let accel = self.accel();
        let ax = accel.x();
        let ay = accel.y();

        // let ax_abs = ax.abs();
        // let ay_abs = ay.abs();
        let ax_abs = if ax <= 0.0 { 0.0 - ax } else { ax };
        let ay_abs = if ay <= 0.0 { 0.0 - ay } else { ay };
        if ax_abs > 0.5 || ay_abs > 0.5 {
            if ax_abs > ay_abs {
                if ax < -0.5 {
                    self.ledc.gd = Gd::Right;
                    self.gd = Gd::Right;
                }
                if ax > 0.5 {
                    self.ledc.gd = Gd::Left;
                    self.gd = Gd::Left;
                }
            }

            if ax_abs < ay_abs {
                if ay < -0.5 {
                    self.ledc.gd = Gd::Up;
                    self.gd = Gd::Up;
                }
                if ay > 0.5 {
                    self.ledc.gd = Gd::Down;
                    self.gd = Gd::Down;
                }
            }
        } else {
            self.ledc.gd = Gd::None;
            self.gd = Gd::None;
        }
    }

    pub fn new(mpu6050: Mpu6050<I2C<'d, T>>, mut ledc: LedControl<'d>) -> Self {
        ledc.set_intensity(0x01);

        App {
            buzzer: true,
            uis: Ui::uis(),
            ui_current_idx: 0,
            face: Face::new(),
            gd: Gd::default(),

            mpu6050,
            ledc,
        }
    }

    pub fn accel(&mut self) -> AccelF32 {
        self.mpu6050.accel().unwrap().scaled(AccelFullScale::G2)
    }

    pub fn run(mut self) -> ! {
        loop {
            // FIXME delay_ms(800);

            self.gravity_direction();
            if self.gd == Gd::default() {
                self.ledc
                    .upload_raw(self.uis[self.ui_current_idx as usize].ui());
                continue;
            }

            match self.gd {
                Gd::None => {
                    self.ledc
                        .upload_raw(self.uis[self.ui_current_idx as usize].ui());

                    self.ledc
                        .upload_raw(self.uis[self.ui_current_idx as usize].ui());
                }
                Gd::Up => {
                    // 向上进入对应的界面
                    let ui = &self.uis[self.ui_current_idx as usize];
                    match ui {
                        Ui::Timer => Timer::run(&mut self),
                        Ui::Dice => Dice::run(&mut self),
                        Ui::Snake => SnakeGame::new().run(&mut self),
                        Ui::BaGua => BaGua::run(&mut self),
                        Ui::Maze => {}
                        Ui::Temp => {}
                        Ui::Sound => {}
                    }
                }
                Gd::Right => {
                    self.ui_current_idx += 1;
                    if self.ui_current_idx >= self.uis.len() as i8 {
                        self.ui_current_idx = 0;
                    }
                    self.ledc
                        .upload_raw(self.uis[self.ui_current_idx as usize].ui());
                }
                Gd::Left => {
                    self.ui_current_idx -= 1;
                    if self.ui_current_idx < 0 {
                        self.ui_current_idx = self.uis.len() as i8 - 1;
                    }
                    self.ledc
                        .upload_raw(self.uis[self.ui_current_idx as usize].ui());
                }
                _ => {}
            }
        }
    }
}
