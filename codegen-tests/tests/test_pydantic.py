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

import re
import subprocess
import sys
from pathlib import Path

import pytest

from conftest import get_corpus_files, make_output_dir, run_codegen


def _line_containing(text: str, token: str) -> str:
    lines = [line for line in text.splitlines() if token in line]
    assert lines, f"no generated line contains {token!r}"
    return lines[-1]


def _assert_constraint(line: str, key: str, value: int) -> None:
    assert re.search(rf"{key}={value}(?![0-9])", line), f"{key}={value} missing: {line}"


@pytest.fixture
def pydantic_output_dir(request: pytest.FixtureRequest) -> Path:
    return make_output_dir(request, "pydantic")


def test_pydantic_corpus_nonempty(corpus_dir: Path) -> None:
    files = get_corpus_files(corpus_dir)
    assert files, f"corpus glob collected no .idl files under {corpus_dir}"


@pytest.mark.parametrize(
    "options",
    [
        [],
        ["--no-implicit-defaults"],
    ],
)
def test_pydantic(
    idl_file: Path, idl_compiler: Path, pydantic_output_dir: Path, options: list[str]
) -> None:
    py_files = run_codegen(
        idl_compiler, idl_file, pydantic_output_dir, "pydantic-out", extra_args=options
    )
    assert py_files, f"ic-idl -l listed no output files for {idl_file}"
    assert pydantic_output_dir.is_dir(), f"output directory missing: {pydantic_output_dir}"

    generated = sorted(pydantic_output_dir.rglob("*.py"))
    assert generated, f"no .py files generated under {pydantic_output_dir}"
    total_bytes = sum(py_file.stat().st_size for py_file in generated)
    assert total_bytes > 0, f"generated output under {pydantic_output_dir} is empty"

    for py_file in py_files:
        assert py_file.is_file(), f"expected output file not written: {py_file}"

        result = subprocess.run(
            [sys.executable, "-m", "py_compile", str(py_file)],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0, f"Syntax error in {py_file}:\n{result.stderr}"


def test_pydantic_validation_constraints(
    corpus_dir: Path, idl_compiler: Path, pydantic_output_dir: Path
) -> None:
    idl_file = corpus_dir / "validation_annotations.idl"
    py_files = run_codegen(idl_compiler, idl_file, pydantic_output_dir, "pydantic-out")
    module_files = [p for p in py_files if p.name == "_cidl_validation_annotations.py"]
    assert module_files, f"module file missing from {py_files}"
    text = module_files[0].read_text()

    _assert_constraint(_line_containing(text, "level: "), "ge", 0)

    temperature = _line_containing(text, "temperature: ")
    _assert_constraint(temperature, "gt", 1)
    _assert_constraint(temperature, "lt", 100)

    _assert_constraint(_line_containing(text, "code: "), "min_length", 1)

    readings = _line_containing(text, "readings: ")
    _assert_constraint(readings, "min_length", 2)
    assert "max_length" not in readings

    assert "pattern=" not in _line_containing(text, "checksum: ")
    assert len([line for line in text.splitlines() if "_re_.compile(" in line]) == 1
    assert "pattern=_re_.compile(r'^[A-Z][A-Za-z0-9]*$')" in _line_containing(
        text, "Identifier = "
    )

    _assert_constraint(_line_containing(text, "Name = "), "min_length", 3)

    _assert_constraint(_line_containing(text, "Name = "), "max_length", 24)
    assert "max_items" not in text
