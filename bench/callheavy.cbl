       IDENTIFICATION DIVISION.
       PROGRAM-ID. CALLHVY.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  N-CALLS PIC 9(8) COMP VALUE 1500000.
       01  I              PIC 9(8) COMP.
       01  X              PIC 9(9) COMP.
       01  Y              PIC 9(9) COMP.
       01  ACC            PIC 9(15) VALUE 0.
       PROCEDURE DIVISION.
       MAIN.
           PERFORM VARYING I FROM 1 BY 1 UNTIL I > N-CALLS
               MOVE I TO X
               CALL 'SUBPGM' USING BY REFERENCE X Y
               ADD Y TO ACC
           END-PERFORM
           DISPLAY 'CALLHVY ACC=' ACC
           STOP RUN.
       END PROGRAM CALLHVY.
       IDENTIFICATION DIVISION.
       PROGRAM-ID. SUBPGM.
       DATA DIVISION.
       LINKAGE SECTION.
       01  LX             PIC 9(9) COMP.
       01  LY             PIC 9(9) COMP.
       PROCEDURE DIVISION USING LX LY.
       MAIN.
           COMPUTE LY = FUNCTION MOD(LX * 31 + 7, 10007)
           ADD 3 TO LY
           GOBACK.
       END PROGRAM SUBPGM.
