      * C15: ACCEPT at the end of SYSIN; run with no input.
       IDENTIFICATION DIVISION.
       PROGRAM-ID. ACCEND.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  N PIC 9(3) VALUE 7.
       01  P PIC S9(3) COMP-3 VALUE 7.
       01  B PIC S9(4) COMP VALUE 7.
       01  X PIC X(4) VALUE 'QQQQ'.
       01  E PIC ZZ9 VALUE 5.
       PROCEDURE DIVISION.
           ACCEPT N
           ACCEPT P
           ACCEPT B
           ACCEPT X
           ACCEPT E
           DISPLAY '[' N '][' P '][' B '][' X '][' E ']'
           GOBACK.
