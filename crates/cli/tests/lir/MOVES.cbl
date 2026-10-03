       IDENTIFICATION DIVISION.
       PROGRAM-ID. MOVES.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  A PIC S9(5)V99 COMP-3 VALUE 12.5.
       01  B PIC 9(3) VALUE 7.
       01  C PIC S9(7)V99.
       01  NAME PIC X(10).
       01  T.
           05 E PIC X(2) OCCURS 5.
       01  I PIC 9 VALUE 2.
       01  IN-REC.
           05 AMT PIC 9(3) VALUE 10.
       01  OUT-REC.
           05 AMT PIC 9(5).
       PROCEDURE DIVISION.
       MAIN.
           COMPUTE C ROUNDED = A * B + 1
               ON SIZE ERROR DISPLAY 'TOO BIG'
           END-COMPUTE.
           DIVIDE B INTO A GIVING C REMAINDER B.
           MOVE 'HELLO' TO NAME.
           MOVE NAME(2:3) TO E(I).
           MOVE AMT OF IN-REC TO AMT OF OUT-REC.
           MOVE ZERO TO C.
           ADD 1 TO B.
           DISPLAY 'C=' C ' ' NAME.
           STOP RUN.
