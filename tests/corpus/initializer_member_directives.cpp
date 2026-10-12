struct S {
    int a[3] = {
#ifdef X
        1,
#endif
        2, 3}; // trailing
    int b; // other
};
