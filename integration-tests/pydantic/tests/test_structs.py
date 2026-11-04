# Copyright 2026 KONGSBERG
#
# Redistribution and use in source and binary forms, with or without
# modification, are permitted provided that the following conditions are met:
#
# 1. Redistributions of source code must retain the above copyright notice,
#    this list of conditions and the following disclaimer.
#
# 2. Redistributions in binary form must reproduce the above copyright notice,
#    this list of conditions and the following disclaimer in the documentation
#    and/or other materials provided with the distribution.
#
# 3. Neither the name of the copyright holder nor the names of its contributors
#    may be used to endorse or promote products derived from this software
#    without specific prior written permission.
#
# THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
# ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
# WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
# DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
# FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
# DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
# SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
# CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
# OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
# OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

from types import ModuleType


def test_point_instantiation(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    p = core.Point(x=10, y=20)
    assert p.x == 10
    assert p.y == 20


def test_point_defaults(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    p = core.Point()
    assert p.x == 0
    assert p.y == 0


def test_point_field_modification(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    p = core.Point(x=5, y=10)
    p.x = 100
    p.y = 200
    assert p.x == 100
    assert p.y == 200


def test_point3d_inheritance(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    p3d = core.Point3D(x=1, y=2, z=3)
    assert p3d.x == 1
    assert p3d.y == 2
    assert p3d.z == 3
    assert isinstance(p3d, core.Point)


def test_nested_struct(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    tl = core.Point(x=0, y=0)
    br = core.Point(x=100, y=100)
    rect = core.Rectangle(top_left=tl, bottom_right=br)
    assert rect.top_left.x == 0
    assert rect.bottom_right.y == 100


def test_all_primitives(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    p = core.AllPrimitives(
        bool_val=True,
        byte_val=255,
        short_val=-100,
        ushort_val=1000,
        long_val=-50000,
        ulong_val=100000,
        longlong_val=-9999999999,
        ulonglong_val=9999999999,
        float_val=3.14,
        double_val=2.71828,
        string_val="hello",
    )
    assert p.bool_val is True
    assert p.byte_val == 255
    assert p.short_val == -100
    assert p.string_val == "hello"


def test_struct_with_sequence(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    s = core.WithSequence(numbers=[1, 2, 3], names=["a", "b"])
    assert s.numbers == [1, 2, 3]
    assert s.names == ["a", "b"]


def test_struct_with_array(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    s = core.WithArray(fixed_numbers=[1, 2, 3, 4, 5])
    assert len(s.fixed_numbers) == 5
    assert s.fixed_numbers[0] == 1


def test_struct_with_map(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    s = core.WithMap(string_to_int={"one": 1, "two": 2})
    assert s.string_to_int["one"] == 1
    assert s.string_to_int["two"] == 2


def test_multi_level_inheritance(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    p4d = core.Point4D(x=1, y=2, z=3, w=4)
    assert p4d.x == 1
    assert p4d.y == 2
    assert p4d.z == 3
    assert p4d.w == 4
    assert isinstance(p4d, core.Point3D)
    assert isinstance(p4d, core.Point)


def test_empty_struct(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    e = core.Empty()
    assert e is not None


def test_point_field_types(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    annotations = core.Point.__annotations__
    assert annotations["x"] == "int"
    assert annotations["y"] == "int"


def test_all_primitives_field_types(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    annotations = core.AllPrimitives.__annotations__
    assert annotations["bool_val"] == "bool"
    assert annotations["byte_val"] == "int"
    assert annotations["short_val"] == "int"
    assert annotations["ushort_val"] == "int"
    assert annotations["long_val"] == "int"
    assert annotations["ulong_val"] == "int"
    assert annotations["longlong_val"] == "int"
    assert annotations["ulonglong_val"] == "int"
    assert annotations["float_val"] == "float"
    assert annotations["double_val"] == "float"
    assert annotations["string_val"] == "str"


def test_sequence_field_types(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    annotations = core.WithSequence.__annotations__
    assert annotations["numbers"] == "_typing_.List[int]"
    assert annotations["names"] == "_typing_.List[str]"


def test_array_field_types(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    annotations = core.WithArray.__annotations__
    assert annotations["fixed_numbers"] == "_typing_.List[int]"


def test_map_field_types(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    annotations = core.WithMap.__annotations__
    assert annotations["string_to_int"] == "_typing_.Dict[str, int]"


def test_nested_struct_field_types(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    annotations = core.Rectangle.__annotations__
    assert annotations["top_left"] == "Point"
    assert annotations["bottom_right"] == "Point"


def test_all_primitives_defaults(generated_modules: dict[str, ModuleType]) -> None:
    core = generated_modules["struct_types"]
    p = core.AllPrimitives()
    assert p.bool_val is False
    assert p.byte_val == 0
    assert p.short_val == 0
    assert p.ushort_val == 0
    assert p.long_val == 0
    assert p.ulong_val == 0
    assert p.longlong_val == 0
    assert p.ulonglong_val == 0
    assert p.float_val == 0.0
    assert p.double_val == 0.0
    assert p.string_val == ""


def test_primitive_types_are_correct_python_types(
    generated_modules: dict[str, ModuleType],
) -> None:
    core = generated_modules["struct_types"]
    p = core.AllPrimitives(
        bool_val=True,
        byte_val=255,
        short_val=-100,
        ushort_val=1000,
        long_val=-50000,
        ulong_val=100000,
        longlong_val=-9999999999,
        ulonglong_val=9999999999,
        float_val=3.14,
        double_val=2.71828,
        string_val="hello",
    )
    assert isinstance(p.bool_val, bool)
    assert isinstance(p.byte_val, int)
    assert isinstance(p.short_val, int)
    assert isinstance(p.ushort_val, int)
    assert isinstance(p.long_val, int)
    assert isinstance(p.ulong_val, int)
    assert isinstance(p.longlong_val, int)
    assert isinstance(p.ulonglong_val, int)
    assert isinstance(p.float_val, float)
    assert isinstance(p.double_val, float)
    assert isinstance(p.string_val, str)
