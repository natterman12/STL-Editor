"""Parameters: named values a project keeps, and the expressions that use them.

A feature's number can be "=width/2 + 3" instead of a number; it is worked out from the
project's parameters each time the timeline is rebuilt, so changing a parameter updates every
step that uses it. Lengths are millimetres and angles degrees. The same rules are in
web/js/expr.js (the web edition); tests check the two agree.
"""
from __future__ import annotations

import math
import re


class ExprError(ValueError):
    pass


def _deg(f):
    return lambda d: f(math.radians(d))


def _ang(f):
    return lambda x: math.degrees(f(x))


FUNCS = {
    "sqrt": math.sqrt, "abs": abs, "min": lambda *a: min(a), "max": lambda *a: max(a),
    "round": lambda x: math.floor(x + 0.5), "floor": math.floor, "ceil": math.ceil,
    "sin": _deg(math.sin), "cos": _deg(math.cos), "tan": _deg(math.tan),
    "asin": _ang(math.asin), "acos": _ang(math.acos), "atan": _ang(math.atan),
}
# how many numbers each function takes (None: one or more)
ARITY = {"min": None, "max": None, **{k: 1 for k in FUNCS if k not in ("min", "max")}}
CONSTS = {"pi": math.pi}
NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
RESERVED = set(FUNCS) | set(CONSTS)
_NUM = re.compile(r"(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?")
_ID = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


def is_expr(v):
    return isinstance(v, str) and v.strip().startswith("=")


def _tokens(text):
    s = str(text).strip()
    if s.startswith("="):
        s = s[1:]
    out, i = [], 0
    while i < len(s):
        c = s[i]
        if c.isspace():
            i += 1
            continue
        m = _NUM.match(s, i)
        if m:
            out.append(("n", float(m.group(0))))
            i = m.end()
            continue
        m = _ID.match(s, i)
        if m:
            out.append(("id", m.group(0)))
            i = m.end()
            continue
        if c in "+-*/%^(),":
            out.append((c, None))
            i += 1
            continue
        raise ExprError(f'can\'t read "{c}" in {s}')
    return out


def names_in(text):
    try:
        return {v for t, v in _tokens(text) if t == "id" and v not in RESERVED}
    except ExprError:
        return set()


def evaluate(text, vars=None):
    vars = vars or {}
    tk = _tokens(text)
    src = str(text).strip().lstrip("=")
    pos = [0]

    def peek():
        return tk[pos[0]] if pos[0] < len(tk) else None

    def take(t):
        if pos[0] < len(tk) and tk[pos[0]][0] == t:
            pos[0] += 1
            return True
        return False

    def expr():
        v = term()
        while True:
            if take("+"):
                v += term()
            elif take("-"):
                v -= term()
            else:
                return v

    def term():
        v = power()
        while True:
            if take("*"):
                v *= power()
            elif take("/"):
                d = power()
                if d == 0:
                    raise ExprError(f"division by zero in {src}")
                v /= d
            elif take("%"):
                v = math.fmod(v, power())
            else:
                return v

    def power():
        b = unary()
        if not take("^"):
            return b
        r = b ** power()
        if isinstance(r, complex):
            raise ExprError(f"{src} doesn't give a number")
        return r

    def unary():
        if take("-"):
            return -unary()
        if take("+"):
            return unary()
        return primary()

    def primary():
        k = peek()
        if k is None:
            raise ExprError(f"{src} ends too soon")
        pos[0] += 1
        t, v = k
        if t == "n":
            return v
        if t == "(":
            r = expr()
            if not take(")"):
                raise ExprError(f'a ")" is missing in {src}')
            return r
        if t == "id":
            if take("("):
                fn = FUNCS.get(v)
                if fn is None:
                    raise ExprError(f'there\'s no function "{v}"')
                args = []
                if not take(")"):
                    args.append(expr())
                    while take(","):
                        args.append(expr())
                    if not take(")"):
                        raise ExprError(f'a ")" is missing in {src}')
                if not args:
                    raise ExprError(f"{v}() needs a number")
                n = ARITY[v]
                if n is not None and len(args) != n:
                    raise ExprError(f"{v}() takes {n} number{'' if n == 1 else 's'}")
                return float(fn(*args))
            if v in CONSTS:
                return CONSTS[v]
            if v in vars:
                return float(vars[v])
            raise ExprError(f'there\'s no parameter "{v}"')
        raise ExprError(f'unexpected "{t}" in {src}')

    if not tk:
        raise ExprError("an expression is empty")
    try:
        r = expr()
    except (OverflowError, ValueError) as e:
        if isinstance(e, ExprError):
            raise
        raise ExprError(f"{src} doesn't give a number") from None
    if pos[0] < len(tk):
        t, v = tk[pos[0]]
        raise ExprError(f'unexpected "{v if v is not None else t}" in {src}')
    if not math.isfinite(r):
        raise ExprError(f"{src} doesn't give a number")
    return r


def resolve_params(lst):
    """{values: {name: number}, errors: {name: message}} for a list [{name, expr}]."""
    by_name, values, errors = {}, {}, {}
    for q in lst or []:
        n = q.get("name") or ""
        if not NAME_RE.match(n):
            errors[n or "?"] = f'"{n}" isn\'t a name (letters, digits and _; not starting with a digit)'
            continue
        if n in RESERVED:
            errors[n] = f'"{n}" is a built-in name'
            continue
        if n in by_name:
            errors[n] = f'"{n}" is there twice'
            continue
        by_name[n] = q
    busy = set()

    def get(name):
        if name in values:
            return values[name]
        if name in errors:
            raise ExprError(errors[name])
        q = by_name.get(name)
        if q is None:
            raise ExprError(f'there\'s no parameter "{name}"')
        if name in busy:
            raise ExprError(f'"{name}" depends on itself')
        busy.add(name)
        try:
            vs = {n: get(n) for n in names_in(str(q.get("expr")))}
            values[name] = evaluate(str(q.get("expr")), vs)
            return values[name]
        except ExprError as e:
            errors[name] = str(e)
            raise
        finally:
            busy.discard(name)

    for name in by_name:
        try:
            get(name)
        except ExprError:
            pass
    return {"values": values, "errors": errors}


def resolve_deep(obj, values):
    if is_expr(obj):
        vs = {}
        for n in names_in(obj):
            if n not in values:
                raise ExprError(f'there\'s no parameter "{n}" (in {obj.strip()})')
            vs[n] = values[n]
        return evaluate(obj, vs)
    if isinstance(obj, list):
        return [resolve_deep(x, values) for x in obj]
    if isinstance(obj, dict):
        return {k: resolve_deep(x, values) for k, x in obj.items()}
    return obj


def uses_expr(obj):
    if is_expr(obj):
        return True
    if isinstance(obj, list):
        return any(uses_expr(x) for x in obj)
    if isinstance(obj, dict):
        return any(uses_expr(x) for x in obj.values())
    return False
