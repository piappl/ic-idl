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


import enum
import importlib
import json
import subprocess
import sys
import tempfile
from collections.abc import Generator
from pathlib import Path
from types import ModuleType

import pytest

# Cross-tree corpus: this witness pins the C22 contract for the codegen corpus
# cell, so the file is fixed and not taken from the --corpus option.
_CORPUS_FILE = (
    Path(__file__).resolve().parents[3]
    / "codegen-tests"
    / "corpus"
    / "value_enums.idl"
)


@pytest.fixture(scope="session")
def value_enums_module(idl_compiler: Path) -> Generator[ModuleType, None, None]:
    with tempfile.TemporaryDirectory(
        prefix="ic-idl-integ-pydantic-value-enums-"
    ) as tmpdir:
        base_dir = Path(tmpdir)
        # The package dir must not be named "generated": the session-scoped
        # generated_modules fixture already owns that name in sys.modules.
        output_dir = base_dir / "value_enums_pkg"
        output_dir.mkdir()
        result = subprocess.run(
            [str(idl_compiler), f"--pydantic-out={output_dir}", str(_CORPUS_FILE)],
            capture_output=True,
            text=True,
            check=False,
            timeout=60,
        )
        if result.returncode != 0:
            pytest.fail(
                f"Failed to generate Pydantic code for {_CORPUS_FILE.name}:\n"
                f"{result.stderr}",
            )
        (output_dir / "__init__.py").touch()
        sys.path.insert(0, str(base_dir))
        try:
            yield importlib.import_module("value_enums_pkg.value_enums")
        finally:
            sys.path.remove(str(base_dir))


def test_value_enums_use_declared_values(
    value_enums_module: ModuleType,
) -> None:
    mod = value_enums_module
    assert issubclass(mod.Priority, enum.Enum)
    assert mod.Priority.LOW.value == -5
    assert mod.Priority.NORMAL.value == 10
    assert mod.Priority.HIGH.value == 20
    assert mod.MixedMode.SEVEN.value == 7
    assert mod.MixedMode.FORTY_TWO.value == 42

    reading = mod.Reading(level=mod.Priority.HIGH, mode=mod.MixedMode.SEVEN)
    assert reading.model_dump()["level"] == 20
    assert reading.model_dump()["mode"] == 7
    assert json.loads(reading.model_dump_json())["level"] == 20
