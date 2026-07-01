"""Small module used as a moldy corpus fixture."""

import os
import sys


def greet(name, greeting="Hello"):
    """Return a greeting string for name."""
    if not name:
        raise ValueError("name must not be empty")
    return f"{greeting}, {name}!"


class Greeter:
    """Greets people, tracking how many greetings it has issued."""

    def __init__(self, default_greeting="Hello"):
        self.default_greeting = default_greeting
        self.count = 0

    def greet(self, name):
        self.count += 1
        return greet(name, self.default_greeting)


def main():
    greeter = Greeter()
    for name in sys.argv[1:]:
        print(greeter.greet(name))
    return os.EX_OK


if __name__ == "__main__":
    sys.exit(main())
