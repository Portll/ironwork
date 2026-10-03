      * C180: a shorter description of an EXTERNAL record.
       IDENTIFICATION DIVISION.
       PROGRAM-ID. MAIN.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  SHARED PIC X(9) EXTERNAL.
       PROCEDURE DIVISION.
           MOVE 'ALPHA0010' TO SHARED
           CALL 'SUB'
           DISPLAY 'MAIN AFTER [' SHARED ']'
           GOBACK.
       END PROGRAM MAIN.
       IDENTIFICATION DIVISION.
       PROGRAM-ID. SUB.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  SHARED IS EXTERNAL.
           05  S-NAME PIC X(5).
           05  S-COUNT PIC 9(3).
       PROCEDURE DIVISION.
           DISPLAY 'SUB SEES [' SHARED ']'
           MOVE 'OMEGA' TO S-NAME
           ADD 1 TO S-COUNT
           GOBACK.
       END PROGRAM SUB.
