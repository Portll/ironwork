# Third-party notices

ironwork for COBOL is licensed under AGPL-3.0-or-later (see [`LICENSE`](LICENSE)). That licence
covers the project's own work. The material below is other people's, and is listed here with its
licence and where it came from.

---

## ICU code-page mapping tables — Unicode License v3

**Where:** [`crates/zarch/ucm/`](crates/zarch/ucm/), 21 files. [`crates/zarch/build.rs`](crates/zarch/build.rs)
reads them at build time and generates the code-page tables compiled into `ironwork-zarch`, so
every build of that crate, and every binary built from it, incorporates this material. Nothing
generated from them is committed.

| File | CCSID | Lines | IBM copyright in the header | SHA-256 |
|---|---|---|---|---|
| `ibm-37_P100-1999.ucm` | 037 | 373 | 1995-2007 | `8ec1b7019dfdab88bc1b607486d928f2de0ace9fa7b5726056fe46c0ce167e15` |
| `ibm-273_P100-1999.ucm` | 273 | 373 | 1995-2007 | `0a8fb7cc194d50daccc4b5b73af0892341854233865574d0f53b09567a56be49` |
| `ibm-277_P100-1999.ucm` | 277 | 373 | 1995-2007 | `8f212969d38fa5a685a85daffb2187f27dc82d10872d79908a0eadc7a5021db3` |
| `ibm-278_P100-1999.ucm` | 278 | 373 | 1995-2007 | `17ca20985ba3e18bf71c392cbbf44ec5412b54d319b1f4ae8378c6a6b3dae9dd` |
| `ibm-280_P100-1999.ucm` | 280 | 373 | 1995-2007 | `88d76b3adaf20abcb45b1b66d41bb43d3eeee7126284102afd3aefb7851376fc` |
| `ibm-284_P100-1999.ucm` | 284 | 373 | 1995-2007 | `a19fc6ff58a98ad4f656312384ef99f6c4bd9792a4b26b6496aa9c711a41c5dd` |
| `ibm-285_P100-1999.ucm` | 285 | 373 | 1995-2007 | `823ae6a766081b952757823367aee64c2376650e9d3160e19513745eac6e82fe` |
| `ibm-297_P100-1999.ucm` | 297 | 373 | 1995-2007 | `b597986b401c21cf7365a4d3801f6bb31894a4e67396a74bb17d67512e519c80` |
| `ibm-500_P100-1999.ucm` | 500 | 373 | 1995-2007 | `1370a76b4a7f6e1d85e404e1bc29be49367312c6bc5eea5d707a9dfe3626c0df` |
| `ibm-871_P100-1999.ucm` | 871 | 373 | 1995-2007 | `05ac7ae91ac8e3edcb4877d17ab0d1e2cb9a2ca4f30575ba1db951b01e4ac095` |
| `ibm-1047_P100-1995.ucm` | 1047 | 372 | 1995-2002 | `f6de10bcf4f3316a05e9bba055999c1062a0a0c1968d937a724ed0a72e0f1e55` |
| `ibm-1140_P100-1997.ucm` | 1140 | 372 | 1995-2002 | `8f95b217dc6eec1bf0c694b29e952098922e09184a8956d35b3d2061f9e82e24` |
| `ibm-1141_P100-1997.ucm` | 1141 | 372 | 1995-2002 | `20f4a0d39aac9d4533b63e01d7cb3c8a2745415218a11008cb65e21dafcb29db` |
| `ibm-1142_P100-1997.ucm` | 1142 | 372 | 1995-2002 | `7cc8cb357d427480d20e02995013bae08501fd1f88cc9d6a17c26ad1c3a35ccc` |
| `ibm-1143_P100-1997.ucm` | 1143 | 372 | 1995-2002 | `15f34e47ba48e5037e65077bae4d894c7032ff2ff9667e5390bf1dd3d239c9c6` |
| `ibm-1144_P100-1997.ucm` | 1144 | 372 | 1995-2002 | `05f90508a3ef58d322ce0f723b6b6d55b004ea4ffdb920e765fb267b93b3c811` |
| `ibm-1145_P100-1997.ucm` | 1145 | 372 | 1995-2002 | `19214813c8cda58eb02d17b3d33a9c3681265ec33be0d3f0fb884019f6ab34ca` |
| `ibm-1146_P100-1997.ucm` | 1146 | 372 | 1995-2002 | `47cdab6c14ff793a3d0b611c78a5298ed9bf9b79431ad2a4086b9537a1775fe3` |
| `ibm-1147_P100-1997.ucm` | 1147 | 372 | 1995-2002 | `a0ff0dc559e6ccc00fa1c461be3eb458b4fe23a6c2d66a13ac62eeaaeb9f54d0` |
| `ibm-1148_P100-1997.ucm` | 1148 | 372 | 1995-2002 | `f0f393fa3274ce5e1a966a3ccfb7416051427b98194f19bbb5efc79938007e9e` |
| `ibm-1149_P100-1997.ucm` | 1149 | 372 | 1995-2002 | `a9cd03b4d79ef568c8674d2dd24c7920cd33e06cb27bd84a950c9136ae460448` |

**Origin:** unicode-org/icu-data, `charset/data/ucm/`, at commit
`8d9eb3e27e79f59dd76e278e58d68b4668835027`:
<https://github.com/unicode-org/icu-data/tree/8d9eb3e27e79f59dd76e278e58d68b4668835027/charset/data/ucm>

**Copyright:** each file's header carries IBM's notice, "Copyright (C) 1995-2002" or "1995-2007",
"International Business Machines Corporation and others. All Rights Reserved.", with the years the
table shows. The headers are kept as published. ICU is a project of Unicode, Inc.

**Licence:** Unicode License v3 (SPDX `Unicode-3.0`), reproduced in full below as ICU distributes
it.

ICU's own licence file also carries the earlier ICU License, which covers ICU 1.8.1 to 57.1 and
IBM's copyright in it from 1995 to 2016. These files' IBM copyright years, 1995 to 2007, fall
inside that range, so that notice is reproduced after the Unicode licence and travels with the
files whichever of the two applies.

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2016-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0
```

```text
ICU License - ICU 1.8.1 to ICU 57.1

COPYRIGHT AND PERMISSION NOTICE

Copyright (c) 1995-2016 International Business Machines Corporation and others
All rights reserved.

Permission is hereby granted, free of charge, to any person obtaining
a copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, and/or sell copies of the Software, and to permit persons
to whom the Software is furnished to do so, provided that the above
copyright notice(s) and this permission notice appear in all copies of
the Software and that both the above copyright notice(s) and this
permission notice appear in supporting documentation.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF THIRD PARTY RIGHTS. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
HOLDERS INCLUDED IN THIS NOTICE BE LIABLE FOR ANY CLAIM, OR ANY
SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES, OR ANY DAMAGES WHATSOEVER
RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF
CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

Except as contained in this notice, the name of a copyright holder
shall not be used in advertising or otherwise to promote the sale, use
or other dealings in this Software without prior written authorization
of the copyright holder.

All trademarks and registered trademarks mentioned herein are the
property of their respective owners.
```

## AWS CardDemo test fixtures — Apache License 2.0

**Where:** [`fixtures/cobolwork/bms/`](fixtures/cobolwork/bms/), four files copied by way of
cobolwork from [AWS CardDemo](https://github.com/aws-samples/aws-mainframe-modernization-carddemo):
the `COSGN00` and `COCRDSL` BMS maps (`app/bms/`) and the copybooks CICS generated from them
(`app/cpy-bms/`). Tests read them; nothing built from ironwork includes them. Each file is unchanged
below a header naming its source, and keeps its Amazon copyright notice and the Apache License 2.0
header, whose text is at <https://www.apache.org/licenses/LICENSE-2.0>.

## Contributor agreement

[`CLA.md`](CLA.md) is adapted from the Apache Software Foundation's Individual Contributor License
Agreement v2.0, by way of cobolwork's CLA. The adaptation is disclosed in the document.

## Trademarks

IBM, IBM Z, z/Architecture, z/OS and Enterprise COBOL are trademarks of IBM. Unicode is a
registered trademark of Unicode, Inc. They are used nominatively, to say what this software models
and where its data comes from. No affiliation or endorsement is claimed. Hercules, GnuCOBOL, GCC's
gcobol and Zowe are named only as tools a user may run alongside ironwork; none of them is included
in it. `tools/gcobol/` builds a container image from Debian's gcobol package on the user's machine;
the image is not distributed with ironwork.

---

Something missing or wrong here is a bug: report it to <john@portll.net>.
