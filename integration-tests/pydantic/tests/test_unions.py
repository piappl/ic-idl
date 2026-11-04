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

def test_int_or_string(generated_modules: dict[str, ModuleType]) -> None:
    mod = generated_modules["union_types"]

    v1 = mod.IntOrStringIntVal(int_val=123)
    assert v1.int_val == 123
    assert v1.discriminator == 1

    v2 = mod.IntOrStringStrVal(str_val="test")
    assert v2.str_val == "test"
    assert v2.discriminator == 2

def test_typed_value(generated_modules: dict[str, ModuleType]) -> None:
    mod = generated_modules["union_types"]

    # Enum-based discriminator
    # Default Enum generation uses string values equal to names.
    v1 = mod.TypedValueIntValue(int_value=42)
    assert v1.discriminator == "INT_KIND"

    v2 = mod.TypedValueFloatValue(float_value=3.14)
    assert v2.discriminator == "FLOAT_KIND"

    # Verify Enum integration
    assert mod.ValueKind.INT_KIND == "INT_KIND"

    # Using alias for discriminator injection
    v3 = mod.TypedValueIntValue(int_value=10, **{"$discriminator": mod.ValueKind.INT_KIND})
    assert v3.int_value == 10

def test_bool_switch(generated_modules: dict[str, ModuleType]) -> None:
    mod = generated_modules["union_types"]

    v1 = mod.BoolSwitchTrueVal(true_val=1)
    assert v1.discriminator is True

    v2 = mod.BoolSwitchFalseVal(false_val="no")
    assert v2.discriminator is False

def test_multi_case(generated_modules: dict[str, ModuleType]) -> None:
    mod = generated_modules["union_types"]

    # MultiCaseSmallVal handles cases 1, 2, 3
    # Default discriminator for MultiCaseSmallVal is likely the first label (1).

    v1 = mod.MultiCaseSmallVal(small_val=10)
    assert v1.discriminator == 1

    # We can override discriminator using the alias
    v2 = mod.MultiCaseSmallVal(small_val=10, **{"$discriminator": 2})
    assert v2.discriminator == 2

    v3 = mod.MultiCaseTextVal(text_val="txt")
    assert v3.discriminator == 10
