import xospy


def test_binding_works():
    assert isinstance(xospy.version(), str)