# Third-party code

## MAME Intel 8086/80186 CPU core

`crates/icon-core/src/cpu/i86.rs` and `crates/icon-core/src/cpu/i186.rs` are a
Rust port of MAME's `src/devices/cpu/i86/` (`i86.cpp`, `i86.h`,
`i86inline.h`, `i186.cpp`, `i186.h`), <https://github.com/mamedev/mame>.

Copyright holders, as listed in those files:

- i86.cpp, i86.h, i86inline.h, i186.cpp, i186.h: Carl
  (i86.cpp credits earlier work by Bryan McPhail, Oliver Bergmann,
  Fabrice Frances and David Hedley)
- 80186 peripheral code in i186.cpp: from the rmnimbus driver by Phill
  Harvey-Smith, based on the Leland sound driver by Aaron Giles and
  Paul Leaman

The ported code is distributed under the BSD 3-Clause license:

```
Copyright (c) Carl and the MAME contributors listed above

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

## MAME TMS5220 speech synthesizer

`crates/icon-core/src/devices/tms5220.rs` is a Rust port of MAME's
`src/devices/sound/tms5220.cpp` and the TMS5220 coefficient tables in
`tms5110r.hxx`, <https://github.com/mamedev/mame>, reduced to what the ICON
uses (Speak External, status, interrupt; no speech ROM).

Copyright holders, as listed in those files: Frank Palazzolo, Aaron Giles,
Jonathan Gevaryahu, Raphael Nabet, Couriersud and Michael Zapf. It is
distributed under the same BSD 3-Clause license as the CPU core above.

## Test data (not distributed)

`scripts/fetch_cpu_tests.py` downloads the SingleStepTests 8086 and 80286
suites (<https://github.com/SingleStepTests>, MIT license, by Daniel Balsom)
into the gitignored `testdata/` directory. They are not part of this
repository.
