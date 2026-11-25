{
  lib,
  fetchPypi,
  python3Packages,
}:
python3Packages.buildPythonApplication rec {
  pname = "hermes-dec";
  version = "0.1.8";
  pyproject = true;

  src = fetchPypi {
    pname = "hermes_dec";
    inherit version;
    hash = "sha256-X36EUWmmC+YxkRkyjSZWOg4sobPnPfbPm4m3DZREyUQ=";
  };

  build-system = with python3Packages; [
    setuptools
    wheel
  ];

  pythonImportsCheck = [ "hermes_dec.decompilation.hbc_decompiler" ];

  meta = {
    description = "Decompiler and disassembler for Hermes bytecode";
    homepage = "https://github.com/P1sec/hermes-dec";
    license = lib.licenses.agpl3Plus;
    mainProgram = "hbc-decompiler";
  };
}
