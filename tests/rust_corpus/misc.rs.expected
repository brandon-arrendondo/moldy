/// Doc comment
/** Block doc */
pub struct S;

impl Display for S {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "S")
    }
}

pub async fn fetch() -> u32 {
    0
}

macro_rules! square {
    ($x:expr) => {
        $x * $x
    };
}

pub enum E {
    A = 1,
    B = 2,
}

fn slice_and_index(v: &[i32]) -> i32 {
    v[0]
}

fn tuple_expr() -> (i32, i32) {
    (1, 2)
}

fn nested_call() {
    foo(bar(1, 2), baz(3));
}

fn long_call() {
    some_function(
        first_argument,
        second_argument,
        third_argument,
    );
}
