"""Fixture covering decorators, comprehensions, and control flow."""

from functools import wraps


def logged(func):
    @wraps(func)
    def wrapper(*args, **kwargs):
        print(f"calling {func.__name__}")
        return func(*args, **kwargs)

    return wrapper


@logged
def squares(values):
    return [v * v for v in values if v >= 0]


@logged
def bucket(values):
    result = {}
    for v in values:
        key = "even" if v % 2 == 0 else "odd"
        result.setdefault(key, []).append(v)
    return result


def first_negative(values):
    for v in values:
        if v < 0:
            return v
    return None
