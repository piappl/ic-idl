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

import importlib
import subprocess
import sys
import sysconfig
import tempfile
from collections.abc import Generator
from pathlib import Path
from types import ModuleType
from typing import NoReturn

import pytest


def pytest_addoption(parser: pytest.Parser) -> None:
    exe_ext = sysconfig.get_config_var("EXE")
    parser.addoption(
        "--idl-compiler",
        action="store",
        default=f"../../target/debug/ic-idl{exe_ext}",
        help="Path to ic-idl compiler binary",
    )
    parser.addoption(
        "--corpus",
        action="store",
        default="../corpus",
        help="Path to IDL corpus directory",
    )


@pytest.fixture(scope="session")
def idl_compiler(request: pytest.FixtureRequest) -> Path:
    path = Path(request.config.getoption("--idl-compiler"))
    if not path.is_absolute():
        path = Path(__file__).parent / path
    path = path.resolve()
    if not path.exists():
        pytest.fail(f"ic-idl not found at {path}. Run 'cargo build' first.")
    return path


@pytest.fixture(scope="session")
def corpus_dir(request: pytest.FixtureRequest) -> Path:
    path = Path(request.config.getoption("--corpus"))
    if not path.is_absolute():
        path = Path(__file__).parent / path
    path = path.resolve()
    if not path.exists():
        pytest.fail(f"Corpus directory not found at {path}")
    return path


@pytest.fixture(scope="session")
def generated_modules(
    idl_compiler: Path,
    corpus_dir: Path,
) -> Generator[dict[str, ModuleType], None, None]:
    with tempfile.TemporaryDirectory(prefix="ic-idl-integ-pydantic-") as tmpdir:
        base_dir = Path(tmpdir)
        output_dir = base_dir / "generated"
        output_dir.mkdir()

        idl_files = sorted(corpus_dir.glob("*.idl"))

        if not idl_files:
            pytest.fail(f"No IDL files found in corpus: {corpus_dir}")

        for idl_file in idl_files:
            result = subprocess.run(
                [str(idl_compiler), f"--pydantic-out={output_dir}", str(idl_file)],
                capture_output=True,
                text=True,
                check=False,
                timeout=60,
            )
            if result.returncode != 0:
                pytest.fail(
                    f"Failed to generate Pydantic code for {idl_file.name}:\n{result.stderr}",
                )

        # Create an __init__.py for the root package
        (output_dir / "__init__.py").touch()

        sys.path.insert(0, str(base_dir))
        try:
            yield _import_generated_modules(output_dir, "generated")
        finally:
            sys.path.remove(str(base_dir))


class _FailedModule:
    """Placeholder for modules that failed to import."""

    def __init__(self, name: str, error: Exception) -> None:
        self._name = name
        self._error = error

    def __getattr__(self, attr: str) -> NoReturn:
        pytest.fail(f"Module {self._name} failed to import: {self._error}")


def _import_generated_modules(
    output_dir: Path,
    root_pkg: str,
) -> dict[str, ModuleType | _FailedModule]:
    """Import all generated top-level packages from output directory."""
    modules: dict[str, ModuleType | _FailedModule] = {}

    # Try importing the root package first
    try:
        importlib.import_module(root_pkg)
    except Exception as e:
        pytest.fail(f"Failed to import root package {root_pkg}: {e}")

    for item in output_dir.iterdir():
        if item.is_dir() and (item / "__init__.py").exists():
            mod_name = item.name
            full_name = f"{root_pkg}.{mod_name}"
            try:
                mod = importlib.import_module(full_name)
                modules[mod_name] = mod
            except Exception as e:  # noqa: BLE001
                modules[mod_name] = _FailedModule(mod_name, e)

    return modules
