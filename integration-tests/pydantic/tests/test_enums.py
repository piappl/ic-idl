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
import enum

def test_color_enum(generated_modules: dict[str, ModuleType]) -> None:
    mod = generated_modules["enum_types"]
    # Default is literal-str
    assert issubclass(mod.Color, str)
    assert issubclass(mod.Color, enum.Enum)

    assert mod.Color.RED.value == "RED"
    assert mod.Color.GREEN.value == "GREEN"
    assert mod.Color.BLUE.value == "BLUE"

    assert mod.Color.RED == "RED" # String comparison works because it inherits str

def test_status_enum(generated_modules: dict[str, ModuleType]) -> None:
    mod = generated_modules["enum_types"]
    # Explicit IDL values are declared values: emitted numerically on an int base
    assert issubclass(mod.Status, int)
    assert issubclass(mod.Status, enum.Enum)

    assert mod.Status.OK.value == 0
    assert mod.Status.WARNING.value == 100
    assert mod.Status.ERROR.value == 200

def test_mixed_enum(generated_modules: dict[str, ModuleType]) -> None:
    mod = generated_modules["enum_types"]
    # A partly declared enum is int-based; implicit members take their evaluated values
    assert issubclass(mod.MixedEnum, int)

    assert mod.MixedEnum.AUTO_FIRST.value == 0
    assert mod.MixedEnum.EXPLICIT_TEN.value == 10
    assert mod.MixedEnum.AUTO_HUNDRED_ONE.value == 101
