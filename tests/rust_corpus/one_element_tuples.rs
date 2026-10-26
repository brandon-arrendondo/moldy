fn tuples() {
    let t = (5,);
    let u: (u8,) = (1,);
    let (only,) = t;
    let parenthesized: (dyn Fn() + 'static) = todo!();
    takes((only,),(u.0,));
}
