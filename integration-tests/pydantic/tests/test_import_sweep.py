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
import tempfile
from collections.abc import Generator
from pathlib import Path
from types import ModuleType

import pytest

from conftest import _FailedModule

SUITE_DIR = Path(__file__).resolve().parent.parent


@pytest.fixture(scope="module")
def corpus_dir() -> Path:
    path = (SUITE_DIR.parent.parent / "codegen-tests" / "corpus").resolve()
    if not path.exists():
        pytest.fail(f"Corpus directory not found at {path}")
    return path


@pytest.fixture(scope="module")
def sweep_modules(
    corpus_dir: Path,
    idl_compiler: Path,
) -> Generator[dict[str, ModuleType | _FailedModule], None, None]:
    # One isolation cell per corpus .idl file: the cell imports only the
    # _cidl_* modules generated from that file, so a dead file cannot be
    # masked by a sibling file's imports and every emitted module is
    # force-imported, including file-scope modules that a package-tree walk
    # would never reach.
    with tempfile.TemporaryDirectory(prefix="ic-idl-sweep-pydantic-") as tmpdir:
        base_dir = Path(tmpdir)
        gen_dir = base_dir / "gen"
        gen_dir.mkdir()
        (gen_dir / "__init__.py").write_text("", encoding="utf-8")

        idl_files = sorted(corpus_dir.rglob("*.idl"))

        if not idl_files:
            pytest.fail(f"No IDL files found in corpus: {corpus_dir}")

        modules: dict[str, ModuleType | _FailedModule] = {}
        for idl_file in idl_files:
            stem = idl_file.stem
            pkg_dir = gen_dir / stem
            pkg_dir.mkdir()
            result = subprocess.run(
                [str(idl_compiler), f"--pydantic-out={pkg_dir}", str(idl_file)],
                capture_output=True,
                text=True,
                check=False,
                timeout=60,
            )
            if result.returncode != 0:
                stderr = result.stderr.strip().splitlines() or [""]
                modules[stem] = _FailedModule(stem, RuntimeError(stderr[-1]))
                continue
            sys.path.insert(0, str(base_dir))
            try:
                error: Exception | None = None
                for leaf in sorted(pkg_dir.rglob("_cidl_*.py")):
                    dotted = ".".join(leaf.relative_to(base_dir).with_suffix("").parts)
                    try:
                        importlib.import_module(dotted)
                    except Exception as exc:  # noqa: BLE001
                        error = exc
                        break
                if error is not None:
                    modules[stem] = _FailedModule(stem, error)
                else:
                    modules[stem] = importlib.import_module(f"gen.{stem}")
            finally:
                sys.path.remove(str(base_dir))

        yield modules


def test_import_sweep(sweep_modules: dict[str, ModuleType | _FailedModule]) -> None:
    dead = sorted(
        f"FAIL {name} {type(mod._error).__name__}: {(str(mod._error).splitlines() or [''])[0]}"  # noqa: SLF001
        for name, mod in sweep_modules.items()
        if isinstance(mod, _FailedModule)
    )
    report = "\n".join(
        [f"TOTAL={len(sweep_modules)} FAILURES={len(dead)}", *dead],
    )
    # Floor of 56 cells: the 51 top-level corpus .idl files plus the five
    # files under the base_typedefs and transitive include subtrees. Guards
    # against a silent skip of the sweep.
    assert len(sweep_modules) >= 56, f"cell floor not met, silent skip suspected\n{report}"
    assert not dead, report
