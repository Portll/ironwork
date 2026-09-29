       CBL TRUNC(STD),NUMPROC(NOPFD),ARITH(COMPAT)
       IDENTIFICATION DIVISION.
       PROGRAM-ID. ORAC01.
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
           05 N-0001 PIC N(256) USAGE NATIONAL.
       01  G-0002.
           05 N-0002 PIC N(256) USAGE NATIONAL.
       01  G-0003.
           05 N-0003 PIC N(256) USAGE NATIONAL.
       01  G-0004.
           05 N-0004 PIC N(256) USAGE NATIONAL.
       01  G-0005.
           05 N-0005 PIC N(256) USAGE NATIONAL.
       01  G-0006.
           05 N-0006 PIC N(256) USAGE NATIONAL.
       01  G-0007.
           05 N-0007 PIC N(256) USAGE NATIONAL.
       01  G-0008.
           05 N-0008 PIC N(256) USAGE NATIONAL.
       01  G-0009.
           05 N-0009 PIC N(256) USAGE NATIONAL.
       01  G-0010.
           05 N-0010 PIC N(256) USAGE NATIONAL.
       01  G-0011.
           05 N-0011 PIC N(256) USAGE NATIONAL.
       01  G-0012.
           05 N-0012 PIC N(256) USAGE NATIONAL.
       01  G-0013.
           05 N-0013 PIC N(256) USAGE NATIONAL.
       01  G-0014.
           05 N-0014 PIC N(256) USAGE NATIONAL.
       01  G-0015.
           05 N-0015 PIC N(256) USAGE NATIONAL.
       01  G-0016.
           05 N-0016 PIC N(256) USAGE NATIONAL.
       01  G-0017.
           05 N-0017 PIC N(256) USAGE NATIONAL.
       01  G-0018.
           05 N-0018 PIC N(256) USAGE NATIONAL.
       01  G-0019.
           05 N-0019 PIC N(256) USAGE NATIONAL.
       01  G-0020.
           05 N-0020 PIC N(256) USAGE NATIONAL.
       01  G-0021.
           05 N-0021 PIC N(256) USAGE NATIONAL.
       01  G-0022.
           05 A-0022 PIC X(1).
           05 B-0022 PIC X(1).
           05 R-0022 PIC X.
       01  G-0023.
           05 A-0023 PIC X(1).
           05 B-0023 PIC X(1).
           05 R-0023 PIC X.
       01  G-0024.
           05 A-0024 PIC X(1).
           05 B-0024 PIC X(3).
           05 R-0024 PIC X.
       01  G-0025.
           05 A-0025 PIC X(1).
           05 B-0025 PIC X(2).
           05 R-0025 PIC X.
       01  G-0026.
           05 A-0026 PIC X(1).
           05 B-0026 PIC X(1).
           05 R-0026 PIC X.
       01  G-0027.
           05 S-0027 PIC S9(3) COMP-3.
           05 SX-0027 REDEFINES S-0027 PIC X(2).
           05 R-0027 PIC S9(3) COMP-3.
       01  G-0028.
           05 S-0028 PIC S9(3) COMP-3.
           05 SX-0028 REDEFINES S-0028 PIC X(2).
           05 R-0028 PIC S9(3) COMP-3.
       01  G-0029.
           05 S-0029 PIC S9(3) COMP-3.
           05 SX-0029 REDEFINES S-0029 PIC X(2).
           05 R-0029 PIC S9(3) COMP-3.
       01  G-0030.
           05 S-0030 PIC S9(3) COMP-3.
           05 SX-0030 REDEFINES S-0030 PIC X(2).
           05 R-0030 PIC S9(3) COMP-3.
       01  G-0031.
           05 S-0031 PIC S9(3) COMP-3.
           05 SX-0031 REDEFINES S-0031 PIC X(2).
           05 R-0031 PIC S9(3) COMP-3.
       01  G-0032.
           05 S-0032 PIC S9(3) COMP-3.
           05 SX-0032 REDEFINES S-0032 PIC X(2).
           05 R-0032 PIC S9(3) COMP-3.
       01  G-0033.
           05 S-0033 PIC S9 COMP-3.
           05 SX-0033 REDEFINES S-0033 PIC X.
           05 T-0033 PIC S9 COMP-3.
           05 TX-0033 REDEFINES T-0033 PIC X.
           05 R-0033 PIC X.
       01  G-0034.
           05 S-0034 PIC S9 COMP-3.
           05 SX-0034 REDEFINES S-0034 PIC X.
           05 T-0034 PIC S9 COMP-3.
           05 TX-0034 REDEFINES T-0034 PIC X.
           05 R-0034 PIC X.
       01  G-0035.
           05 S-0035 PIC S9 COMP-3.
           05 SX-0035 REDEFINES S-0035 PIC X.
           05 T-0035 PIC S9 COMP-3.
           05 TX-0035 REDEFINES T-0035 PIC X.
           05 R-0035 PIC X.
       01  G-0036.
           05 S-0036 PIC S9 COMP-3.
           05 SX-0036 REDEFINES S-0036 PIC X.
           05 T-0036 PIC S9 COMP-3.
           05 TX-0036 REDEFINES T-0036 PIC X.
           05 R-0036 PIC X.
       01  G-0037.
           05 U-0037 PIC 9(3) COMP-3.
           05 UX-0037 REDEFINES U-0037 PIC X(2).
           05 V-0037 PIC S9(3) COMP-3.
           05 VX-0037 REDEFINES V-0037 PIC X(2).
       01  G-0038.
           05 Z-0038 PIC 9(3).
           05 ZX-0038 REDEFINES Z-0038 PIC X(3).
       01  G-0039.
           05 Z-0039 PIC 9(3).
           05 ZX-0039 REDEFINES Z-0039 PIC X(3).
       01  G-0040.
           05 B-0040 PIC 9(4) COMP.
           05 V-0040 PIC S9(5) COMP-3 VALUE 12345.
       01  G-0041.
           05 B-0041 PIC 9(4) COMP.
           05 V-0041 PIC S9(5) COMP-3 VALUE 9999.
       01  G-0042.
           05 B-0042 PIC 9(4) COMP.
           05 V-0042 PIC S9(5) COMP-3 VALUE 70000.
       01  G-0043.
           05 B-0043 PIC S9(4) COMP.
           05 V-0043 PIC S9(5) COMP-3 VALUE 40000.
       01  G-0044.
           05 B-0044 PIC S9(4) COMP.
           05 V-0044 PIC S9(5) COMP-3 VALUE -12345.
       01  G-0045.
           05 B-0045 PIC 9(4) COMP.
           05 V-0045 PIC S9(5) COMP-3 VALUE 12345.
       01  O-0046.
           05 A-0046 PIC S9(18) COMP-3 VALUE 999999999999999999.
           05 B-0046 PIC S9(18) COMP-3 VALUE 999999999999999999.
           05 C-0046 PIC S9(18) COMP-3 VALUE 999999999999999999.
       01  G-0046.
           05 R-0046 PIC S9(18) COMP-3.
       01  O-0047.
           05 A-0047 PIC S9(10)V9(8) COMP-3 VALUE 9999999999.99999999.
           05 B-0047 PIC S9(10)V9(8) COMP-3 VALUE 9999999999.99999999.
           05 C-0047 PIC S9(10)V9(8) COMP-3 VALUE 9999999999.99999999.
       01  G-0047.
           05 R-0047 PIC S9(10)V9(8) COMP-3.
       01  O-0048.
           05 A-0048 COMP-2.
           05 AX-0048 REDEFINES A-0048 PIC X(8).
           05 B-0048 COMP-2.
           05 BX-0048 REDEFINES B-0048 PIC X(8).
       01  G-0048.
           05 D-0048 COMP-2.
       01  O-0049.
           05 A-0049 COMP-2.
           05 AX-0049 REDEFINES A-0049 PIC X(8).
           05 B-0049 COMP-2.
           05 BX-0049 REDEFINES B-0049 PIC X(8).
       01  G-0049.
           05 D-0049 COMP-2.
       01  O-0050.
           05 A-0050 COMP-2.
           05 AX-0050 REDEFINES A-0050 PIC X(8).
           05 B-0050 COMP-2.
           05 BX-0050 REDEFINES B-0050 PIC X(8).
       01  G-0050.
           05 F-0050 COMP-1.
       01  O-0051.
           05 P-0051 PIC S9V9 COMP-3 VALUE 0.1.
       01  G-0051.
           05 D-0051 COMP-2.
       01  O-0052.
           05 A-0052 COMP-2.
           05 AX-0052 REDEFINES A-0052 PIC X(8).
           05 B-0052 COMP-2.
           05 BX-0052 REDEFINES B-0052 PIC X(8).
       01  G-0052.
           05 Q-0052 PIC S9V99 COMP-3.
       01  O-0053.
           05 A-0053 COMP-2.
           05 AX-0053 REDEFINES A-0053 PIC X(8).
           05 B-0053 COMP-2.
           05 BX-0053 REDEFINES B-0053 PIC X(8).
       01  G-0053.
           05 Q-0053 PIC S9V99 COMP-3.
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
           PERFORM CASE-0033
           PERFORM CASE-0034
           PERFORM CASE-0035
           PERFORM CASE-0036
           PERFORM CASE-0037
           PERFORM CASE-0038
           PERFORM CASE-0039
           PERFORM CASE-0040
           PERFORM CASE-0041
           PERFORM CASE-0042
           PERFORM CASE-0043
           PERFORM CASE-0044
           PERFORM CASE-0045
           PERFORM CASE-0046
           PERFORM CASE-0047
           PERFORM CASE-0048
           PERFORM CASE-0049
           PERFORM CASE-0050
           PERFORM CASE-0051
           PERFORM CASE-0052
           PERFORM CASE-0053
           GOBACK.
       CASE-0001.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 37) TO N-0001
           MOVE 'ORAC01.nat.37' TO HEX-ID
           MOVE G-0001 TO HEX-IN
           MOVE LENGTH OF G-0001 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0002.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 273) TO N-0002
           MOVE 'ORAC01.nat.273' TO HEX-ID
           MOVE G-0002 TO HEX-IN
           MOVE LENGTH OF G-0002 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0003.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 277) TO N-0003
           MOVE 'ORAC01.nat.277' TO HEX-ID
           MOVE G-0003 TO HEX-IN
           MOVE LENGTH OF G-0003 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0004.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 278) TO N-0004
           MOVE 'ORAC01.nat.278' TO HEX-ID
           MOVE G-0004 TO HEX-IN
           MOVE LENGTH OF G-0004 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0005.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 280) TO N-0005
           MOVE 'ORAC01.nat.280' TO HEX-ID
           MOVE G-0005 TO HEX-IN
           MOVE LENGTH OF G-0005 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0006.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 284) TO N-0006
           MOVE 'ORAC01.nat.284' TO HEX-ID
           MOVE G-0006 TO HEX-IN
           MOVE LENGTH OF G-0006 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0007.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 285) TO N-0007
           MOVE 'ORAC01.nat.285' TO HEX-ID
           MOVE G-0007 TO HEX-IN
           MOVE LENGTH OF G-0007 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0008.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 297) TO N-0008
           MOVE 'ORAC01.nat.297' TO HEX-ID
           MOVE G-0008 TO HEX-IN
           MOVE LENGTH OF G-0008 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0009.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 500) TO N-0009
           MOVE 'ORAC01.nat.500' TO HEX-ID
           MOVE G-0009 TO HEX-IN
           MOVE LENGTH OF G-0009 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0010.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 871) TO N-0010
           MOVE 'ORAC01.nat.871' TO HEX-ID
           MOVE G-0010 TO HEX-IN
           MOVE LENGTH OF G-0010 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0011.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 1047) TO N-0011
           MOVE 'ORAC01.nat.1047' TO HEX-ID
           MOVE G-0011 TO HEX-IN
           MOVE LENGTH OF G-0011 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0012.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 1140) TO N-0012
           MOVE 'ORAC01.nat.1140' TO HEX-ID
           MOVE G-0012 TO HEX-IN
           MOVE LENGTH OF G-0012 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0013.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 1141) TO N-0013
           MOVE 'ORAC01.nat.1141' TO HEX-ID
           MOVE G-0013 TO HEX-IN
           MOVE LENGTH OF G-0013 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0014.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 1142) TO N-0014
           MOVE 'ORAC01.nat.1142' TO HEX-ID
           MOVE G-0014 TO HEX-IN
           MOVE LENGTH OF G-0014 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0015.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 1143) TO N-0015
           MOVE 'ORAC01.nat.1143' TO HEX-ID
           MOVE G-0015 TO HEX-IN
           MOVE LENGTH OF G-0015 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0016.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 1144) TO N-0016
           MOVE 'ORAC01.nat.1144' TO HEX-ID
           MOVE G-0016 TO HEX-IN
           MOVE LENGTH OF G-0016 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0017.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 1145) TO N-0017
           MOVE 'ORAC01.nat.1145' TO HEX-ID
           MOVE G-0017 TO HEX-IN
           MOVE LENGTH OF G-0017 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0018.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 1146) TO N-0018
           MOVE 'ORAC01.nat.1146' TO HEX-ID
           MOVE G-0018 TO HEX-IN
           MOVE LENGTH OF G-0018 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0019.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 1147) TO N-0019
           MOVE 'ORAC01.nat.1147' TO HEX-ID
           MOVE G-0019 TO HEX-IN
           MOVE LENGTH OF G-0019 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0020.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 1148) TO N-0020
           MOVE 'ORAC01.nat.1148' TO HEX-ID
           MOVE G-0020 TO HEX-IN
           MOVE LENGTH OF G-0020 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0021.
           MOVE FUNCTION NATIONAL-OF(ALL-BYTES, 1149) TO N-0021
           MOVE 'ORAC01.nat.1149' TO HEX-ID
           MOVE G-0021 TO HEX-IN
           MOVE LENGTH OF G-0021 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0022.
           MOVE X'C1' TO A-0022
           MOVE X'81' TO B-0022
           IF A-0022 < B-0022
           MOVE 'L' TO R-0022
           ELSE
           IF A-0022 = B-0022
           MOVE 'E' TO R-0022
           ELSE
           MOVE 'G' TO R-0022
           END-IF
           END-IF
           MOVE 'ORAC01.col.0' TO HEX-ID
           MOVE G-0022 TO HEX-IN
           MOVE LENGTH OF G-0022 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0023.
           MOVE X'E9' TO A-0023
           MOVE X'F0' TO B-0023
           IF A-0023 < B-0023
           MOVE 'L' TO R-0023
           ELSE
           IF A-0023 = B-0023
           MOVE 'E' TO R-0023
           ELSE
           MOVE 'G' TO R-0023
           END-IF
           END-IF
           MOVE 'ORAC01.col.1' TO HEX-ID
           MOVE G-0023 TO HEX-IN
           MOVE LENGTH OF G-0023 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0024.
           MOVE X'C1' TO A-0024
           MOVE X'C14040' TO B-0024
           IF A-0024 < B-0024
           MOVE 'L' TO R-0024
           ELSE
           IF A-0024 = B-0024
           MOVE 'E' TO R-0024
           ELSE
           MOVE 'G' TO R-0024
           END-IF
           END-IF
           MOVE 'ORAC01.col.2' TO HEX-ID
           MOVE G-0024 TO HEX-IN
           MOVE LENGTH OF G-0024 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0025.
           MOVE X'C1' TO A-0025
           MOVE X'C100' TO B-0025
           IF A-0025 < B-0025
           MOVE 'L' TO R-0025
           ELSE
           IF A-0025 = B-0025
           MOVE 'E' TO R-0025
           ELSE
           MOVE 'G' TO R-0025
           END-IF
           END-IF
           MOVE 'ORAC01.col.3' TO HEX-ID
           MOVE G-0025 TO HEX-IN
           MOVE LENGTH OF G-0025 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0026.
           MOVE X'40' TO A-0026
           MOVE X'00' TO B-0026
           IF A-0026 < B-0026
           MOVE 'L' TO R-0026
           ELSE
           IF A-0026 = B-0026
           MOVE 'E' TO R-0026
           ELSE
           MOVE 'G' TO R-0026
           END-IF
           END-IF
           MOVE 'ORAC01.col.4' TO HEX-ID
           MOVE G-0026 TO HEX-IN
           MOVE LENGTH OF G-0026 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0027.
           MOVE X'123A' TO SX-0027
           MOVE S-0027 TO R-0027
           MOVE 'ORAC01.pmove.A' TO HEX-ID
           MOVE G-0027 TO HEX-IN
           MOVE LENGTH OF G-0027 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0028.
           MOVE X'123B' TO SX-0028
           MOVE S-0028 TO R-0028
           MOVE 'ORAC01.pmove.B' TO HEX-ID
           MOVE G-0028 TO HEX-IN
           MOVE LENGTH OF G-0028 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0029.
           MOVE X'123C' TO SX-0029
           MOVE S-0029 TO R-0029
           MOVE 'ORAC01.pmove.C' TO HEX-ID
           MOVE G-0029 TO HEX-IN
           MOVE LENGTH OF G-0029 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0030.
           MOVE X'123D' TO SX-0030
           MOVE S-0030 TO R-0030
           MOVE 'ORAC01.pmove.D' TO HEX-ID
           MOVE G-0030 TO HEX-IN
           MOVE LENGTH OF G-0030 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0031.
           MOVE X'123E' TO SX-0031
           MOVE S-0031 TO R-0031
           MOVE 'ORAC01.pmove.E' TO HEX-ID
           MOVE G-0031 TO HEX-IN
           MOVE LENGTH OF G-0031 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0032.
           MOVE X'123F' TO SX-0032
           MOVE S-0032 TO R-0032
           MOVE 'ORAC01.pmove.F' TO HEX-ID
           MOVE G-0032 TO HEX-IN
           MOVE LENGTH OF G-0032 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0033.
           MOVE X'1F' TO SX-0033
           MOVE X'1C' TO TX-0033
           IF S-0033 < T-0033
           MOVE 'L' TO R-0033
           ELSE
           IF S-0033 = T-0033
           MOVE 'E' TO R-0033
           ELSE
           MOVE 'G' TO R-0033
           END-IF
           END-IF
           MOVE 'ORAC01.pcmp.0' TO HEX-ID
           MOVE G-0033 TO HEX-IN
           MOVE LENGTH OF G-0033 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0034.
           MOVE X'0D' TO SX-0034
           MOVE X'0C' TO TX-0034
           IF S-0034 < T-0034
           MOVE 'L' TO R-0034
           ELSE
           IF S-0034 = T-0034
           MOVE 'E' TO R-0034
           ELSE
           MOVE 'G' TO R-0034
           END-IF
           END-IF
           MOVE 'ORAC01.pcmp.1' TO HEX-ID
           MOVE G-0034 TO HEX-IN
           MOVE LENGTH OF G-0034 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0035.
           MOVE X'1A' TO SX-0035
           MOVE X'1C' TO TX-0035
           IF S-0035 < T-0035
           MOVE 'L' TO R-0035
           ELSE
           IF S-0035 = T-0035
           MOVE 'E' TO R-0035
           ELSE
           MOVE 'G' TO R-0035
           END-IF
           END-IF
           MOVE 'ORAC01.pcmp.2' TO HEX-ID
           MOVE G-0035 TO HEX-IN
           MOVE LENGTH OF G-0035 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0036.
           MOVE X'1C' TO SX-0036
           MOVE X'2C' TO TX-0036
           IF S-0036 < T-0036
           MOVE 'L' TO R-0036
           ELSE
           IF S-0036 = T-0036
           MOVE 'E' TO R-0036
           ELSE
           MOVE 'G' TO R-0036
           END-IF
           END-IF
           MOVE 'ORAC01.pcmp.3' TO HEX-ID
           MOVE G-0036 TO HEX-IN
           MOVE LENGTH OF G-0036 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0037.
           MOVE X'123C' TO UX-0037
           ADD 1 TO U-0037
           MOVE X'123A' TO VX-0037
           ADD 1 TO V-0037
           MOVE 'ORAC01.sign.0' TO HEX-ID
           MOVE G-0037 TO HEX-IN
           MOVE LENGTH OF G-0037 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0038.
           MOVE X'F140F3' TO ZX-0038
           ADD 1 TO Z-0038
           MOVE 'ORAC01.zoned.0' TO HEX-ID
           MOVE G-0038 TO HEX-IN
           MOVE LENGTH OF G-0038 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0039.
           MOVE X'F1F2C3' TO ZX-0039
           ADD 1 TO Z-0039
           MOVE 'ORAC01.zoned.1' TO HEX-ID
           MOVE G-0039 TO HEX-IN
           MOVE LENGTH OF G-0039 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0040.
           COMPUTE B-0040 = V-0040
           MOVE 'ORAC01.trunc.0' TO HEX-ID
           MOVE G-0040 TO HEX-IN
           MOVE LENGTH OF G-0040 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0041.
           COMPUTE B-0041 = V-0041
           MOVE 'ORAC01.trunc.1' TO HEX-ID
           MOVE G-0041 TO HEX-IN
           MOVE LENGTH OF G-0041 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0042.
           COMPUTE B-0042 = V-0042
           MOVE 'ORAC01.trunc.2' TO HEX-ID
           MOVE G-0042 TO HEX-IN
           MOVE LENGTH OF G-0042 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0043.
           COMPUTE B-0043 = V-0043
           MOVE 'ORAC01.trunc.3' TO HEX-ID
           MOVE G-0043 TO HEX-IN
           MOVE LENGTH OF G-0043 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0044.
           COMPUTE B-0044 = V-0044
           MOVE 'ORAC01.trunc.4' TO HEX-ID
           MOVE G-0044 TO HEX-IN
           MOVE LENGTH OF G-0044 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0045.
           MOVE V-0045 TO B-0045
           MOVE 'ORAC01.trunc.5' TO HEX-ID
           MOVE G-0045 TO HEX-IN
           MOVE LENGTH OF G-0045 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0046.
           COMPUTE R-0046 = A-0046 * B-0046 / C-0046
           MOVE 'ORAC01.arith.0' TO HEX-ID
           MOVE G-0046 TO HEX-IN
           MOVE LENGTH OF G-0046 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0047.
           COMPUTE R-0047 = A-0047 * B-0047 / C-0047
           MOVE 'ORAC01.arith.1' TO HEX-ID
           MOVE G-0047 TO HEX-IN
           MOVE LENGTH OF G-0047 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0048.
           MOVE X'4120000000000000' TO AX-0048
           MOVE X'4130000000000000' TO BX-0048
           COMPUTE D-0048 = A-0048 / B-0048
           MOVE 'ORAC01.hfp.0' TO HEX-ID
           MOVE G-0048 TO HEX-IN
           MOVE LENGTH OF G-0048 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0049.
           MOVE X'4110000000000000' TO AX-0049
           MOVE X'4130000000000000' TO BX-0049
           COMPUTE D-0049 = A-0049 / B-0049 * B-0049
           MOVE 'ORAC01.hfp.1' TO HEX-ID
           MOVE G-0049 TO HEX-IN
           MOVE LENGTH OF G-0049 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0050.
           MOVE X'4112345680000000' TO AX-0050
           MOVE X'4110000000000000' TO BX-0050
           MOVE A-0050 TO F-0050
           MOVE 'ORAC01.hfp.2' TO HEX-ID
           MOVE G-0050 TO HEX-IN
           MOVE LENGTH OF G-0050 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0051.
           COMPUTE D-0051 = P-0051
           MOVE 'ORAC01.hfp.3' TO HEX-ID
           MOVE G-0051 TO HEX-IN
           MOVE LENGTH OF G-0051 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0052.
           MOVE X'4019999999999999' TO AX-0052
           MOVE X'4110000000000000' TO BX-0052
           COMPUTE Q-0052 = A-0052
           MOVE 'ORAC01.hfp.4' TO HEX-ID
           MOVE G-0052 TO HEX-IN
           MOVE LENGTH OF G-0052 TO HEX-LEN
           PERFORM DUMP-CASE.
       CASE-0053.
           MOVE X'4019999999999999' TO AX-0053
           MOVE X'4110000000000000' TO BX-0053
           COMPUTE Q-0053 ROUNDED = A-0053
           MOVE 'ORAC01.hfp.5' TO HEX-ID
           MOVE G-0053 TO HEX-IN
           MOVE LENGTH OF G-0053 TO HEX-LEN
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
