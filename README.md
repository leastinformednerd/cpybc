This is intended to be a version agnostic crate for handling CPython Bytecode, both in parsing and emitting it. It is not close to being that right now

Planned features (for static analysis of existing bytecode):
  - [ ] Arbitrary unmarshalling into Rust
  - [ ] `.pyc` file format / magic number support
  - [ ] Abstract interpretation of code objects for Python 3.14
    - [ ] Expanding this to cover older Python versions (cutoff to be determined)

The intention is that this should be used in a rewrite of [the decompiler](https://github.com/leastinformednerd/python-bytecode-playground/tree/main/decompiler), and from the start for any future Python static analysis that I do (e.g. equality saturation program equivalence)

Tentative features (as codegen backend):
  - [ ] Creation of the most recent version of `.pyc` files
  - [ ] Code objects for the most recent version of the CPython interpreter

I will probably only make this is if I rewrite [the compiler](https://github.com/leastinformednerd/python-bytecode-playground/tree/main/compiler) in rust, which is not seeming super likely
