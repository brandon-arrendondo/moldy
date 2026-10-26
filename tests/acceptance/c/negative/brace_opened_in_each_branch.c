#ifdef CONFIG_FEATURE

#define REG_KIND_CR 0
#define REG_KIND_DR 2

static int  fits(int length)
{
#ifdef CONFIG_WIDE
    if (length < 15) {
#else
    if (length < 7) {
#endif
        return 1;
    }
    return  0;
}

#endif
