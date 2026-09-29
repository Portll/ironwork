       IDENTIFICATION DIVISION.
       PROGRAM-ID. PACKED.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  N-ITER PIC 9(8) COMP VALUE 500000.
       01  I              PIC 9(8) COMP.
       01  A              PIC S9(7)V99 COMP-3 VALUE 1234.56.
       01  B              PIC S9(5)V9(4) COMP-3 VALUE 3.1415.
       01  C              PIC S9(9)V99 COMP-3 VALUE 0.
       01  D              PIC S9(9)V99 COMP-3 VALUE 0.
       01  ACC            PIC S9(13)V99 COMP-3 VALUE 0.
       01  SHOW           PIC -9(13).99.
       PROCEDURE DIVISION.
       MAIN.
           PERFORM VARYING I FROM 1 BY 1 UNTIL I > N-ITER
               COMPUTE C ROUNDED = A * B + I / 7
               ADD C TO ACC
               MULTIPLY 1.0007 BY C ROUNDED
               ADD 1 TO D
               COMPUTE D ROUNDED = D + C / 3
               IF D > 1000000
                   SUBTRACT 1000000 FROM D
               END-IF
               ADD D TO ACC
               IF ACC > 5000000000
                   SUBTRACT 5000000000 FROM ACC
               END-IF
               COMPUTE A ROUNDED = A + 0.01
               IF A > 9000000
                   MOVE 1234.56 TO A
               END-IF
           END-PERFORM
           MOVE ACC TO SHOW
           DISPLAY 'PACKED ACC=' SHOW
           MOVE D TO SHOW
           DISPLAY 'PACKED D=' SHOW
           STOP RUN.
