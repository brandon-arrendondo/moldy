int parse_bool(const char *text)
{
    if (strcmp(text, "true") == 0) return 1;
    if (strcmp(text, "false") == 0) return 0;
    return strcmp(text, "default")  ==  0 ? 0 : -1;
}
