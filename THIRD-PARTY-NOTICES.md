Apple Pkl is distributed under the Apache License 2.0.

Nex release artifacts may include an unmodified Pkl command-line binary under
libexec/nex/pkl for Nex-private evaluation of canonical .pkl definitions.

Bundled Pkl is third-party software from Apple Inc. It is not authored by
Styrene Labs. See:

- share/doc/nex/third-party/pkl/LICENSE.txt
- share/doc/nex/third-party/pkl/THIRD-PARTY-NOTICES.txt

Nex selects Pkl evaluators in this order:

1. NEX_PKL override
2. bundled libexec/nex/pkl
3. ambient pkl on PATH
4. nix shell nixpkgs#pkl -c pkl fallback
