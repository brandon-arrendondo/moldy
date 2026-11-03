/* A member without a value keeps the members around it in one group,
   whatever its trailing comment holds. */
enum e {
    A = 1, // a
    B, // b = c
    CCCC = 3, // d
    /* x = y */
    DD = 4,
};
