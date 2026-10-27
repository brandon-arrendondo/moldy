typedef enum {
    MODE_A = 0,
    /* before the second mode */
    MODE_B,
    MODE_C
    /* after the last mode */
} mode_e;

int f(void)
{
#ifdef FEATURE
    return 1;
#endif
    /* before the return */
    return 0;
}

int g(int a, int b)
{
    return a ||
        /* why b */
        b;
}
