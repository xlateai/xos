"""Print an exhaustive, colorized tree of the public `xos` API (no window)."""

import xos

# Minecraft-style & codes (see xos.print_color / xos.colorize)
C_MODULE = "&a"   # green (lime-ish)
C_CONST = "&8"    # dark gray
C_CLASS = "&9"    # blue
C_FUNC = "&d"     # light purple
C_BRANCH = "&8"
C_RESET = "&r"

ORDER = ("module", "constant", "class", "function")

_FUNCTION_TYPENAMES = (
    "function",
    "builtin_function_or_method",
    "method",
    "builtin_method",
    "classmethod",
    "staticmethod",
)


def _out(text, end="\n"):
    xos.print_color(text, end=end)


def _is_public(name):
    return not name.startswith("_")


def _is_constant_name(name):
    if "_" not in name:
        return False
    if not name.replace("_", "").isalnum():
        return False
    return name.upper() == name and any(ch.isalpha() for ch in name)


def _typename(obj):
    return type(obj).__name__


def _classify(name, obj):
    tn = _typename(obj)
    if tn == "module":
        return "module"
    if tn == "type":
        return "class"
    if _is_constant_name(name):
        return "constant"
    if tn in _FUNCTION_TYPENAMES:
        return "function"
    if callable(obj):
        return "function"
    return None


def _bucket_entries(mod):
    buckets = {
        "module": [],
        "constant": [],
        "class": [],
        "function": [],
    }
    for name in sorted(dir(mod), key=str.lower):
        if not _is_public(name):
            continue
        try:
            obj = getattr(mod, name)
        except Exception:
            continue
        kind = _classify(name, obj)
        if kind is None:
            continue
        buckets[kind].append((name, obj))
    return buckets


def _flat_sorted(mod):
    buckets = _bucket_entries(mod)
    out = []
    for kind in ORDER:
        for name, obj in buckets[kind]:
            out.append((kind, name, obj))
    return out


def _styled(kind, name):
    if kind == "module":
        return C_MODULE + name + "/" + C_RESET
    if kind == "constant":
        return C_CONST + name + C_RESET
    if kind == "class":
        return C_CLASS + name + C_RESET
    return C_FUNC + name + "()" + C_RESET


def _render_children(mod, prefix, visited):
    items = _flat_sorted(mod)
    for index, (kind, name, obj) in enumerate(items):
        last = index == len(items) - 1
        branch = "└── " if last else "├── "
        _out(prefix + branch + _styled(kind, name))

        if kind != "module":
            continue

        sub = obj
        sub_id = id(sub)
        if sub_id in visited:
            cont = "    " if last else "│   "
            _out(prefix + cont + "└── " + C_BRANCH + "(cycle)" + C_RESET)
            continue

        visited.add(sub_id)
        child_prefix = prefix + ("    " if last else "│   ")
        _render_children(sub, child_prefix, visited)


def print_module_tree(root=None):
    """Recursively print public `xos` members as a directory-style tree."""
    mod = root if root is not None else xos
    root_name = getattr(mod, "__name__", "xos")
    _out(C_MODULE + root_name + C_RESET)
    visited = {id(mod)}
    _render_children(mod, "", visited)


if __name__ == "__main__":
    print_module_tree()
