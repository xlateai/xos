import xos


def test_binding_works():
    assert isinstance(xos.version(), str)