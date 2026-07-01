#include <stdio.h>

int classify(int x)
{
    if (x < 0) {
        return -1;
    } else if (x == 0) {
        return 0;
    } else {
        return 1;
    }
}

int sum_to(int n)
{
    int total = 0;
    for (int i = 0; i <= n; i++) {
        total += i;
    }
    return total;
}

void countdown(int n)
{
    while (n > 0) {
        printf("%d\n", n);
        n--;
    }
}

void run_at_least_once(int n)
{
    int i = 0;
    do {
        printf("%d\n", i);
        i++;
    } while (i < n);
}

const char *day_name(int day)
{
    switch (day) {
        case 0:
            return "Sunday";
        case 1:
        case 2:
        case 3:
        case 4:
        case 5:
            return "Weekday";
        case 6:
            return "Saturday";
        default:
            return "Unknown";
    }
}

int find_first_negative(int *arr, int len)
{
    for (int i = 0; i < len; i++) {
        if (arr[i] < 0) {
            goto found;
        }
    }
    return -1;

found:
    return arr[0];
}

int clamp(int x, int lo, int hi)
{
    return x < lo ? lo : (x > hi ? hi : x);
}

void skip_evens(int *arr, int len)
{
    for (int i = 0; i < len; i++) {
        if (arr[i] % 2 == 0) {
            continue;
        }
        if (arr[i] < 0) {
            break;
        }
        printf("%d\n", arr[i]);
    }
}
