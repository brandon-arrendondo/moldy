fn before() {
    if ready  {
        run( 1 );
    }
}

fn add() {
    let add = |a, b,| a + b;
    run( 1 );
}

fn typed() {
    let typed = |a: u32, b: u32,| a + b;
    run( 1 );
}

fn pick() {
    let pick = |a, b,| match a { _ => b, };
    run( 1 );
}

fn chain() {
    let total = v.iter().map(|a, b,| a + b).count();
    run( 1 );
}

fn split() {
    let k = |
        first: A,
        second: B,
    | match first {
        _ => second,
    };
    run( 1 );
}

fn after() {
    if ready  {
        run( 1 );
    }
}
