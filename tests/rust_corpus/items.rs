#[allow(dead_code)]
pub(crate) struct Wrapper<T>(pub T);

pub struct Unit;

pub mod inner {
    pub const MAX: u32 = 100;
    static GREETING: &str = "hi";
}

pub trait Animal {
    type Sound;
    fn make_sound(&self) -> Self::Sound;
}

fn generic_fn<T: Clone + Default>(x: T) -> T
where
    T: std::fmt::Debug,
{
    x.clone()
}

fn maybe(x: Option<i32>) -> Option<i32> {
    let y = x?;
    if let Some(v) = x {
        Some(v + 1)
    } else {
        None
    }
}

fn neg(x: i32) -> i32 {
    let a = -x;
    let b = !true;
    let c = a - -x;
    a
}

fn loops() {
    loop {
        break;
    }
    'outer: for i in 0..10 {
        if i == 5 {
            break 'outer;
        }
    }
}

struct Point {
    x: i32,
    y: i32,
}

fn structs_and_updates() {
    let base = Point { x: 1, y: 2 };
    let updated = Point { x: 5, ..base };
}
