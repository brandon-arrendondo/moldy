def read(path):
    try:
        with open(path) as fh:
            data = fh.read()
    except FileNotFoundError:
        data = None
    except Exception as exc:
        data = exc
    return data


def sign(x):
    for i in x:
        if i > 0:
            a = 1
        # zero is the middle case
        elif i < 0:
            a = 2
        else:  # zero
            a = 3
    return a


def pairs(s, it):
    single = (s,)
    keys = [
        "a",  # first
        # second follows
        "b",
        # nothing after this
    ]
    return single, keys, zip((s,), it)
