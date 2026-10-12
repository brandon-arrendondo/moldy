struct Nested { int pair[2]; int values[2]; };
static const struct Nested records[] = {
    /* First record */ {
        { 1, 2 }, /* Pair */
        { 3, 4 } /* Values */
    }, /* First end */
    /* Second record */ {
        /* Pair */ { 5, 6 },
        /* Values */ { 7, 8 },
    } /* Second end */
};
static const struct Nested records_with_comma[] = {
    {
        { 9, 10 },
        { 11, 12 },
    }, /* Last record */
};
