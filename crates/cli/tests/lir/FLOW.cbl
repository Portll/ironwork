       IDENTIFICATION DIVISION.
       PROGRAM-ID. FLOW.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  I PIC 9(2) VALUE 0.
       01  TOTAL PIC 9(4) VALUE 0.
       01  FLAG PIC X VALUE 'N'.
           88 DONE VALUE 'Y'.
       PROCEDURE DIVISION.
       MAIN.
           PERFORM ADD-ONE 3 TIMES.
           PERFORM VARYING I FROM 1 BY 1 UNTIL I > 5
               ADD I TO TOTAL
           END-PERFORM.
           IF TOTAL > 10 AND NOT DONE
               SET DONE TO TRUE
               PERFORM SHOW THRU SHOW-EXIT
           ELSE
               DISPLAY 'SMALL'
           END-IF.
           EVALUATE TRUE
               WHEN TOTAL = 0 GO TO FINISH
               WHEN OTHER CONTINUE
           END-EVALUATE.
       FINISH.
           GOBACK.
       ADD-ONE.
           ADD 1 TO TOTAL.
       SHOW.
           DISPLAY 'TOTAL=' TOTAL.
       SHOW-EXIT.
           EXIT.
