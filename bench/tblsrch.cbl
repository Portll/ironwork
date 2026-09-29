       IDENTIFICATION DIVISION.
       PROGRAM-ID. TBLSRCH.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  N-PASSES PIC 9(8) COMP VALUE 20000.
       01  TBL.
           05 ENT OCCURS 500 TIMES
                 ASCENDING KEY IS ENT-KEY
                 INDEXED BY IX.
              10 ENT-KEY  PIC 9(6).
              10 ENT-VAL  PIC 9(6).
       01  I              PIC 9(8) COMP.
       01  K              PIC 9(6).
       01  HITS           PIC 9(9) VALUE 0.
       01  VSUM           PIC 9(12) VALUE 0.
       PROCEDURE DIVISION.
       MAIN.
           PERFORM VARYING I FROM 1 BY 1 UNTIL I > 500
               COMPUTE ENT-KEY (I) = I * 3
               COMPUTE ENT-VAL (I) = I * 11 + 5
           END-PERFORM
           PERFORM VARYING I FROM 1 BY 1 UNTIL I > N-PASSES
               COMPUTE K = FUNCTION MOD(I * 37, 1600)
               SET IX TO 1
               SEARCH ENT
                   AT END CONTINUE
                   WHEN ENT-KEY (IX) = K
                       ADD 1 TO HITS
                       ADD ENT-VAL (IX) TO VSUM
               END-SEARCH
               SEARCH ALL ENT
                   AT END CONTINUE
                   WHEN ENT-KEY (IX) = K
                       ADD 1 TO HITS
                       ADD ENT-VAL (IX) TO VSUM
               END-SEARCH
           END-PERFORM
           DISPLAY 'TBLSRCH HITS=' HITS ' VSUM=' VSUM
           STOP RUN.
