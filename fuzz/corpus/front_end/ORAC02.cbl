       CBL TRUNC(OPT),NUMPROC(PFD),ARITH(EXTEND)
       IDENTIFICATION DIVISION.
       PROGRAM-ID. ORAC02.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  HEX-DIGITS PIC X(16) VALUE '0123456789ABCDEF'.
       01  HEX-IN     PIC X(512).
       01  HEX-LEN    PIC 9(4) COMP.
       01  HEX-I      PIC 9(4) COMP.
       01  HEX-J      PIC 9(4) COMP.
       01  HEX-N      PIC 9(4) COMP.
       01  HEX-HI     PIC 9(4) COMP.
       01  HEX-LO     PIC 9(4) COMP.
       01  HEX-OFF    PIC 9(4).
       01  HEX-LINE   PIC X(64).
       01  HEX-ID     PIC X(24).
       01  ALL-BYTES  PIC X(256).
       01  G-0001.
           05 A-0001 PIC X(1).
           05 B-0001 PIC X(1).
           05 R-0001 PIC X.
       01  G-0002.
           05 A-0002 PIC X(1).
           05 B-0002 PIC X(1).
           05 R-0002 PIC X.
       01  G-0003.
           05 A-0003 PIC X(1).
           05 B-0003 PIC X(3).
           05 R-0003 PIC X.
       01  G-0004.
           05 A-0004 PIC X(1).
           05 B-0004 PIC X(2).
           05 R-0004 PIC X.
       01  G-0005.
           05 A-0005 PIC X(1).
           05 B-0005 PIC X(1).
           05 R-0005 PIC X.
       01  G-0006.
           05 S-0006 PIC S9(3) COMP-3.
           05 SX-0006 REDEFINES S-0006 PIC X(2).
           05 R-0006 PIC S9(3) COMP-3.
       01  G-0007.
           05 S-0007 PIC S9(3) COMP-3.
           05 SX-0007 REDEFINES S-0007 PIC X(2).
           05 R-0007 PIC S9(3) COMP-3.
       01  G-0008.
           05 S-0008 PIC S9(3) COMP-3.
           05 SX-0008 REDEFINES S-0008 PIC X(2).
           05 R-0008 PIC S9(3) COMP-3.
       01  G-0009.
           05 S-0009 PIC S9(3) COMP-3.
           05 SX-0009 REDEFINES S-0009 PIC X(2).
           05 R-0009 PIC S9(3) COMP-3.
       01  G-0010.
           05 S-0010 PIC S9(3) COMP-3.
           05 SX-0010 REDEFINES S-0010 PIC X(2).
           05 R-0010 PIC S9(3) COMP-3.
       01  G-0011.
           05 S-0011 PIC S9(3) COMP-3.
           05 SX-0011 REDEFINES S-0011 PIC X(2).
           05 R-0011 PIC S9(3) COMP-3.
       01  G-0012.
           05 S-0012 PIC S9 COMP-3.
           05 SX-0012 REDEFINES S-0012 PIC X.
           05 T-0012 PIC S9 COMP-3.
           05 TX-0012 REDEFINES T-0012 PIC X.
           05 R-0012 PIC X.
       01  G-0013.
           05 S-0013 PIC S9 COMP-3.
           05 SX-0013 REDEFINES S-0013 PIC X.
           05 T-0013 PIC S9 COMP-3.
           05 TX-0013 REDEFINES T-0013 PIC X.
           05 R-0013 PIC X.
       01  G-0014.
           05 S-0014 PIC S9 COMP-3.
           05 SX-0014 REDEFINES S-0014 PIC X.
           05 T-0014 PIC S9 COMP-3.
           05 TX-0014 REDEFINES T-0014 PIC X.
           05 R-0014 PIC X.
       01  G-0015.
           05 S-0015 PIC S9 COMP-3.
           05 SX-0015 REDEFINES S-0015 PIC X.
           05 T-0015 PIC S9 COMP-3.
           05 TX-0015 REDEFINES T-0015 PIC X.
           05 R-0015 PIC X.
       01  G-0016.
           05 U-0016 PIC 9(3) COMP-3.
           05 UX-0016 REDEFINES U-0016 PIC X(2).
           05 V-0016 PIC S9(3) COMP-3.
           05 VX-0016 REDEFINES V-0016 PIC X(2).
       01  G-0017.
           05 Z-0017 PIC 9(3).
           05 ZX-0017 REDEFINES Z-0017 PIC X(3).
       01  G-0018.
           05 Z-0018 PIC 9(3).
           05 ZX-0018 REDEFINES Z-0018 PIC X(3).
       01  G-0019.
           05 B-0019 PIC 9(4) COMP.
           05 V-0019 PIC S9(5) COMP-3 VALUE 12345.
       01  G-0020.
           05 B-0020 PIC 9(4) COMP.
           05 V-0020 PIC S9(5) COMP-3 VALUE 9999.
       01  G-0021.
           05 B-0021 PIC 9(4) COMP.
           05 V-0021 PIC S9(5) COMP-3 VALUE 70000.
       01  G-0022.
           05 B-0022 PIC S9(4) COMP.
           05 V-0022 PIC S9(5) COMP-3 VALUE 40000.
       01  G-0023.
           05 B-0023 PIC S9(4) COMP.
           05 V-0023 PIC S9(5) COMP-3 VALUE -12345.
       01  G-0024.
           05 B-0024 PIC 9(4) COMP.
           05 V-0024 PIC S9(5) COMP-3 VALUE 12345.
       01  O-0025.
           05 A-0025 PIC S9(18) COMP-3 VALUE 999999999999999999.
           05 B-0025 PIC S9(18) COMP-3 VALUE 999999999999999999.
           05 C-0025 PIC S9(18) COMP-3 VALUE 999999999999999999.
       01  G-0025.
           05 R-0025 PIC S9(18) COMP-3.
       01  O-0026.
           05 A-0026 PIC S9(10)V9(8) COMP-3 VALUE 9999999999.99999999.
           05 B-0026 PIC S9(10)V9(8) COMP-3 VALUE 9999999999.99999999.
           05 C-0026 PIC S9(10)V9(8) COMP-3 VALUE 9999999999.99999999.
       01  G-0026.
           05 R-0026 PIC S9(10)V9(8) COMP-3.
       01  O-0027.
           05 A-0027 COMP-2.
           05 AX-0027 REDEFINES A-0027 PIC X(8).
           05 B-0027 COMP-2.
           05 BX-0027 REDEFINES B-0027 PIC X(8).
       01  G-0027.
           05 D-0027 COMP-2.
       01  O-0028.
           05 A-0028 COMP-2.
           05 AX-0028 REDEFINES A-0028 PIC X(8).
           05 B-0028 COMP-2.
           05 BX-0028 REDEFINES B-0028 PIC X(8).
       01  G-0028.
           05 D-0028 COMP-2.
       01  O-0029.
           05 A-0029 COMP-2.
           05 AX-0029 REDEFINES A-0029 PIC X(8).
           05 B-0029 COMP-2.
           05 BX-0029 REDEFINES B-0029 PIC X(8).
       01  G-0029.
           05 F-0029 COMP-1.
       01  O-0030.
           05 P-0030 PIC S9V9 COMP-3 VALUE 0.1.
       01  G-0030.
           05 D-0030 COMP-2.
       01  O-0031.
           05 A-0031 COMP-2.
           05 AX-0031 REDEFINES A-0031 PIC X(8).
           05 B-0031 COMP-2.
           05 BX-0031 REDEFINES B-0031 PIC X(8).
       01  G-0031.
           05 Q-0031 PIC S9V99 COMP-3.
       01  O-0032.
           05 A-0032 COMP-2.
           05 AX-0032 REDEFINES A-0032 PIC X(8).
           05 B-0032 COMP-2.
           05 BX-0032 REDEFINES B-0032 PIC X(8).
       01  G-0032.
           05 Q-0032 PIC S9V99 COMP-3.
       PROCEDURE DIVISION.
       MAIN-LINE.
           PERFORM FILL-ALL-BYTES
           PERFORM CASE-0001
           PERFORM CASE-0002
           PERFORM CASE-0003
           PERFORM CASE-0004
           PERFORM CASE-0005
           PERFORM CASE-0006
           PERFORM CASE-0007
           PERFORM CASE-0008
           PERFORM CASE-0009
           PERFORM CASE-0010
           PERFORM CASE-0011
           PERFORM CASE-0012
           PERFORM CASE-0013
           PERFORM CASE-0014
           PERFORM CASE-0015
           PERFORM CASE-0016
           PERFORM CASE-0017
           PERFORM CASE-0018
           PERFORM CASE-0019
           PERFORM CASE-0020
           PERFORM CASE-0021
           PERFORM CASE-0022
           PERFORM CASE-0023
           PERFORM CASE-0024
           PERFORM CASE-0025
           PERFORM CASE-0026
           PERFORM CASE-0027
           PERFORM CASE-0028
           PERFORM CASE-0029
           PERFORM CASE-0030
           PERFORM CASE-0031
           PERFORM CASE-0032
           GOBACK.
       CASE-0001.
           MOVE X'C1' TO A-0001
           MOVE X'81' TO B-0001
           IF A-0001 < B-0001
           MOVE 'L' TO R-0001
           ELSE
           IF A-0001 = B-0001
           MOVE 'E' TO R-0001
           ELSE
           MOVE 'G' TO R-0001
           END-IF
           END-IF
           MOVE 'ORAC02.col.0' TO HEX-ID
           MOVE G-0001 TO HEX-IN
           MOVE LENGTH OF G-0001 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0002.
           MOVE X'E9' TO A-0002
           MOVE X'F0' TO B-0002
           IF A-0002 < B-0002
           MOVE 'L' TO R-0002
           ELSE
           IF A-0002 = B-0002
           MOVE 'E' TO R-0002
           ELSE
           MOVE 'G' TO R-0002
           END-IF
           END-IF
           MOVE 'ORAC02.col.1' TO HEX-ID
           MOVE G-0002 TO HEX-IN
           MOVE LENGTH OF G-0002 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0003.
           MOVE X'C1' TO A-0003
           MOVE X'C14040' TO B-0003
           IF A-0003 < B-0003
           MOVE 'L' TO R-0003
           ELSE
           IF A-0003 = B-0003
           MOVE 'E' TO R-0003
           ELSE
           MOVE 'G' TO R-0003
           END-IF
           END-IF
           MOVE 'ORAC02.col.2' TO HEX-ID
           MOVE G-0003 TO HEX-IN
           MOVE LENGTH OF G-0003 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0004.
           MOVE X'C1' TO A-0004
           MOVE X'C100' TO B-0004
           IF A-0004 < B-0004
           MOVE 'L' TO R-0004
           ELSE
           IF A-0004 = B-0004
           MOVE 'E' TO R-0004
           ELSE
           MOVE 'G' TO R-0004
           END-IF
           END-IF
           MOVE 'ORAC02.col.3' TO HEX-ID
           MOVE G-0004 TO HEX-IN
           MOVE LENGTH OF G-0004 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0005.
           MOVE X'40' TO A-0005
           MOVE X'00' TO B-0005
           IF A-0005 < B-0005
           MOVE 'L' TO R-0005
           ELSE
           IF A-0005 = B-0005
           MOVE 'E' TO R-0005
           ELSE
           MOVE 'G' TO R-0005
           END-IF
           END-IF
           MOVE 'ORAC02.col.4' TO HEX-ID
           MOVE G-0005 TO HEX-IN
           MOVE LENGTH OF G-0005 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0006.
           MOVE X'123A' TO SX-0006
           MOVE S-0006 TO R-0006
           MOVE 'ORAC02.pmove.A' TO HEX-ID
           MOVE G-0006 TO HEX-IN
           MOVE LENGTH OF G-0006 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0007.
           MOVE X'123B' TO SX-0007
           MOVE S-0007 TO R-0007
           MOVE 'ORAC02.pmove.B' TO HEX-ID
           MOVE G-0007 TO HEX-IN
           MOVE LENGTH OF G-0007 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0008.
           MOVE X'123C' TO SX-0008
           MOVE S-0008 TO R-0008
           MOVE 'ORAC02.pmove.C' TO HEX-ID
           MOVE G-0008 TO HEX-IN
           MOVE LENGTH OF G-0008 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0009.
           MOVE X'123D' TO SX-0009
           MOVE S-0009 TO R-0009
           MOVE 'ORAC02.pmove.D' TO HEX-ID
           MOVE G-0009 TO HEX-IN
           MOVE LENGTH OF G-0009 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0010.
           MOVE X'123E' TO SX-0010
           MOVE S-0010 TO R-0010
           MOVE 'ORAC02.pmove.E' TO HEX-ID
           MOVE G-0010 TO HEX-IN
           MOVE LENGTH OF G-0010 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0011.
           MOVE X'123F' TO SX-0011
           MOVE S-0011 TO R-0011
           MOVE 'ORAC02.pmove.F' TO HEX-ID
           MOVE G-0011 TO HEX-IN
           MOVE LENGTH OF G-0011 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0012.
           MOVE X'1F' TO SX-0012
           MOVE X'1C' TO TX-0012
           IF S-0012 < T-0012
           MOVE 'L' TO R-0012
           ELSE
           IF S-0012 = T-0012
           MOVE 'E' TO R-0012
           ELSE
           MOVE 'G' TO R-0012
           END-IF
           END-IF
           MOVE 'ORAC02.pcmp.0' TO HEX-ID
           MOVE G-0012 TO HEX-IN
           MOVE LENGTH OF G-0012 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0013.
           MOVE X'0D' TO SX-0013
           MOVE X'0C' TO TX-0013
           IF S-0013 < T-0013
           MOVE 'L' TO R-0013
           ELSE
           IF S-0013 = T-0013
           MOVE 'E' TO R-0013
           ELSE
           MOVE 'G' TO R-0013
           END-IF
           END-IF
           MOVE 'ORAC02.pcmp.1' TO HEX-ID
           MOVE G-0013 TO HEX-IN
           MOVE LENGTH OF G-0013 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0014.
           MOVE X'1A' TO SX-0014
           MOVE X'1C' TO TX-0014
           IF S-0014 < T-0014
           MOVE 'L' TO R-0014
           ELSE
           IF S-0014 = T-0014
           MOVE 'E' TO R-0014
           ELSE
           MOVE 'G' TO R-0014
           END-IF
           END-IF
           MOVE 'ORAC02.pcmp.2' TO HEX-ID
           MOVE G-0014 TO HEX-IN
           MOVE LENGTH OF G-0014 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0015.
           MOVE X'1C' TO SX-0015
           MOVE X'2C' TO TX-0015
           IF S-0015 < T-0015
           MOVE 'L' TO R-0015
           ELSE
           IF S-0015 = T-0015
           MOVE 'E' TO R-0015
           ELSE
           MOVE 'G' TO R-0015
           END-IF
           END-IF
           MOVE 'ORAC02.pcmp.3' TO HEX-ID
           MOVE G-0015 TO HEX-IN
           MOVE LENGTH OF G-0015 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0016.
           MOVE X'123C' TO UX-0016
           ADD 1 TO U-0016
           MOVE X'123A' TO VX-0016
           ADD 1 TO V-0016
           MOVE 'ORAC02.sign.0' TO HEX-ID
           MOVE G-0016 TO HEX-IN
           MOVE LENGTH OF G-0016 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0017.
           MOVE X'F140F3' TO ZX-0017
           ADD 1 TO Z-0017
           MOVE 'ORAC02.zoned.0' TO HEX-ID
           MOVE G-0017 TO HEX-IN
           MOVE LENGTH OF G-0017 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0018.
           MOVE X'F1F2C3' TO ZX-0018
           ADD 1 TO Z-0018
           MOVE 'ORAC02.zoned.1' TO HEX-ID
           MOVE G-0018 TO HEX-IN
           MOVE LENGTH OF G-0018 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0019.
           COMPUTE B-0019 = V-0019
           MOVE 'ORAC02.trunc.0' TO HEX-ID
           MOVE G-0019 TO HEX-IN
           MOVE LENGTH OF G-0019 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0020.
           COMPUTE B-0020 = V-0020
           MOVE 'ORAC02.trunc.1' TO HEX-ID
           MOVE G-0020 TO HEX-IN
           MOVE LENGTH OF G-0020 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0021.
           COMPUTE B-0021 = V-0021
           MOVE 'ORAC02.trunc.2' TO HEX-ID
           MOVE G-0021 TO HEX-IN
           MOVE LENGTH OF G-0021 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0022.
           COMPUTE B-0022 = V-0022
           MOVE 'ORAC02.trunc.3' TO HEX-ID
           MOVE G-0022 TO HEX-IN
           MOVE LENGTH OF G-0022 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0023.
           COMPUTE B-0023 = V-0023
           MOVE 'ORAC02.trunc.4' TO HEX-ID
           MOVE G-0023 TO HEX-IN
           MOVE LENGTH OF G-0023 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0024.
           MOVE V-0024 TO B-0024
           MOVE 'ORAC02.trunc.5' TO HEX-ID
           MOVE G-0024 TO HEX-IN
           MOVE LENGTH OF G-0024 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0025.
           COMPUTE R-0025 = A-0025 * B-0025 / C-0025
           MOVE 'ORAC02.arith.0' TO HEX-ID
           MOVE G-0025 TO HEX-IN
           MOVE LENGTH OF G-0025 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0026.
           COMPUTE R-0026 = A-0026 * B-0026 / C-0026
           MOVE 'ORAC02.arith.1' TO HEX-ID
           MOVE G-0026 TO HEX-IN
           MOVE LENGTH OF G-0026 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0027.
           MOVE X'4120000000000000' TO AX-0027
           MOVE X'4130000000000000' TO BX-0027
           COMPUTE D-0027 = A-0027 / B-0027
           MOVE 'ORAC02.hfp.0' TO HEX-ID
           MOVE G-0027 TO HEX-IN
           MOVE LENGTH OF G-0027 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0028.
           MOVE X'4110000000000000' TO AX-0028
           MOVE X'4130000000000000' TO BX-0028
           COMPUTE D-0028 = A-0028 / B-0028 * B-0028
           MOVE 'ORAC02.hfp.1' TO HEX-ID
           MOVE G-0028 TO HEX-IN
           MOVE LENGTH OF G-0028 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0029.
           MOVE X'4112345680000000' TO AX-0029
           MOVE X'4110000000000000' TO BX-0029
           MOVE A-0029 TO F-0029
           MOVE 'ORAC02.hfp.2' TO HEX-ID
           MOVE G-0029 TO HEX-IN
           MOVE LENGTH OF G-0029 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0030.
           COMPUTE D-0030 = P-0030
           MOVE 'ORAC02.hfp.3' TO HEX-ID
           MOVE G-0030 TO HEX-IN
           MOVE LENGTH OF G-0030 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0031.
           MOVE X'4019999999999999' TO AX-0031
           MOVE X'4110000000000000' TO BX-0031
           COMPUTE Q-0031 = A-0031
           MOVE 'ORAC02.hfp.4' TO HEX-ID
           MOVE G-0031 TO HEX-IN
           MOVE LENGTH OF G-0031 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0032.
           MOVE X'4019999999999999' TO AX-0032
           MOVE X'4110000000000000' TO BX-0032
           COMPUTE Q-0032 ROUNDED = A-0032
           MOVE 'ORAC02.hfp.5' TO HEX-ID
           MOVE G-0032 TO HEX-IN
           MOVE LENGTH OF G-0032 TO HEX-LEN
           PERFORM DUMP-CASE.
       FILL-ALL-BYTES.
           PERFORM VARYING HEX-I FROM 1 BY 1 UNTIL HEX-I > 256
               MOVE FUNCTION CHAR(HEX-I) TO ALL-BYTES(HEX-I:1)
           END-PERFORM.
       DUMP-CASE.
           PERFORM VARYING HEX-I FROM 1 BY 32 UNTIL HEX-I > HEX-LEN
               MOVE SPACES TO HEX-LINE
               PERFORM VARYING HEX-J FROM 0 BY 1
                       UNTIL HEX-J > 31 OR HEX-I + HEX-J > HEX-LEN
                   COMPUTE HEX-N =
                       FUNCTION ORD(HEX-IN(HEX-I + HEX-J:1)) - 1
                   DIVIDE HEX-N BY 16 GIVING HEX-HI REMAINDER HEX-LO
                   MOVE HEX-DIGITS(HEX-HI + 1:1)
                     TO HEX-LINE(HEX-J * 2 + 1:1)
                   MOVE HEX-DIGITS(HEX-LO + 1:1)
                     TO HEX-LINE(HEX-J * 2 + 2:1)
               END-PERFORM
               COMPUTE HEX-OFF = HEX-I - 1
               DISPLAY 'CASE ' HEX-ID ' ' HEX-OFF ' ' HEX-LINE
           END-PERFORM.
