      * C262: unsigned zoned items compared with zero and with each
      * other when their bytes are not all digits.
       IDENTIFICATION DIVISION.
       PROGRAM-ID. ZONECMP.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  VALUE0 PIC X(4) VALUE '00 0'.
       01  VALUE1 REDEFINES VALUE0 PIC 9(4).
       01  W PIC 9(4) VALUE 0.
       PROCEDURE DIVISION.
           IF VALUE1 = ZERO DISPLAY 'ZERO' ELSE DISPLAY 'ZONES' END-IF
           IF VALUE1 = W DISPLAY 'W' ELSE DISPLAY 'NOT W' END-IF
           GOBACK.
