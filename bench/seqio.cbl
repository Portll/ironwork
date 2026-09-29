       IDENTIFICATION DIVISION.
       PROGRAM-ID. SEQIO.
       ENVIRONMENT DIVISION.
       INPUT-OUTPUT SECTION.
       FILE-CONTROL.
           SELECT BENCHF ASSIGN TO BENCHF
               ORGANIZATION IS SEQUENTIAL
               FILE STATUS IS FS.
       DATA DIVISION.
       FILE SECTION.
       FD  BENCHF
           RECORDING MODE IS F.
       01  REC.
           05 REC-ID      PIC 9(8).
           05 REC-NAME    PIC X(20).
           05 REC-AMOUNT  PIC 9(7)V99.
           05 REC-FILL    PIC X(23).
       WORKING-STORAGE SECTION.
       01  N-RECS PIC 9(8) COMP VALUE 1000000.
       01  FS             PIC XX.
       01  I              PIC 9(8) COMP.
       01  TOTAL          PIC 9(13)V99 VALUE 0.
       01  IDSUM          PIC 9(15) VALUE 0.
       01  EOF-FLAG       PIC X VALUE 'N'.
       PROCEDURE DIVISION.
       MAIN.
           OPEN OUTPUT BENCHF
           PERFORM VARYING I FROM 1 BY 1 UNTIL I > N-RECS
               MOVE I TO REC-ID
               MOVE 'CUSTOMER RECORD' TO REC-NAME
               COMPUTE REC-AMOUNT = FUNCTION MOD(I * 7919, 1000003)
                   / 100
               MOVE SPACES TO REC-FILL
               WRITE REC
           END-PERFORM
           CLOSE BENCHF
           OPEN INPUT BENCHF
           PERFORM UNTIL EOF-FLAG = 'Y'
               READ BENCHF
                   AT END MOVE 'Y' TO EOF-FLAG
                   NOT AT END
                       ADD REC-AMOUNT TO TOTAL
                       ADD REC-ID TO IDSUM
               END-READ
           END-PERFORM
           CLOSE BENCHF
           DISPLAY 'SEQIO TOTAL=' TOTAL ' IDSUM=' IDSUM
           STOP RUN.
