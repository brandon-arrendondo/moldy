#define COLORS C(red) C(green)
static const char *color_names[] = {
#define C(name) #name, "dark" #name,
COLORS
#undef C
};
